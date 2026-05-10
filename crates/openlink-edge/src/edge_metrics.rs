//! # 边缘指标 (Phase 9)
//!
//! 请求量/延迟/错误率、缓存命中率、资源使用。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// 请求延迟统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    /// 平均延迟 (ms)
    pub avg_ms: f64,
    /// P50 延迟 (ms)
    pub p50_ms: f64,
    /// P95 延迟 (ms)
    pub p95_ms: f64,
    /// P99 延迟 (ms)
    pub p99_ms: f64,
    /// 最大延迟 (ms)
    pub max_ms: f64,
    /// 最小延迟 (ms)
    pub min_ms: f64,
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self {
            avg_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            max_ms: 0.0,
            min_ms: 0.0,
        }
    }
}

/// 资源使用占位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// 内存使用 (MB) — 占位
    pub memory_mb: f64,
    /// CPU 使用率 (%) — 占位
    pub cpu_percent: f64,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            memory_mb: 0.0,
            cpu_percent: 0.0,
        }
    }
}

/// 边缘指标快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeMetricsSnapshot {
    /// 总请求量
    pub total_requests: u64,
    /// 成功请求量
    pub success_requests: u64,
    /// 错误请求量
    pub error_requests: u64,
    /// 错误率 (0.0 - 1.0)
    pub error_rate: f64,
    /// 请求延迟统计
    pub latency: LatencyStats,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// 缓存命中次数
    pub cache_hits: u64,
    /// 缓存未命中次数
    pub cache_misses: u64,
    /// 资源使用
    pub resources: ResourceUsage,
    /// 采集时间戳
    pub timestamp: i64,
}

/// 延迟采样记录
#[derive(Debug, Clone)]
struct LatencySample {
    latency_ms: f64,
}

/// 边缘指标收集器
pub struct EdgeMetricsCollector {
    total_requests: AtomicU64,
    success_requests: AtomicU64,
    error_requests: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    /// 延迟采样（保留最近 1000 个）
    latency_samples: Arc<Mutex<Vec<LatencySample>>>,
    max_samples: usize,
}

impl EdgeMetricsCollector {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            success_requests: AtomicU64::new(0),
            error_requests: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            latency_samples: Arc::new(Mutex::new(Vec::with_capacity(1000))),
            max_samples: 1000,
        }
    }

    /// 创建带自定义采样容量的收集器
    pub fn with_max_samples(max_samples: usize) -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            success_requests: AtomicU64::new(0),
            error_requests: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            latency_samples: Arc::new(Mutex::new(Vec::with_capacity(max_samples))),
            max_samples,
        }
    }

    /// 记录请求
    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录成功请求
    pub fn record_success(&self, latency_ms: f64) {
        self.success_requests.fetch_add(1, Ordering::Relaxed);
        // 异步记录延迟采样（简化为同步推入）
        let samples = self.latency_samples.clone();
        let max = self.max_samples;
        tokio::spawn(async move {
            let mut samples = samples.lock().await;
            if samples.len() >= max {
                samples.remove(0);
            }
            samples.push(LatencySample { latency_ms });
        });
    }

    /// 记录错误请求
    pub fn record_error(&self) {
        self.error_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录缓存命中
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录缓存未命中
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取指标快照
    pub async fn snapshot(&self) -> EdgeMetricsSnapshot {
        let total = self.total_requests.load(Ordering::Relaxed);
        let success = self.success_requests.load(Ordering::Relaxed);
        let errors = self.error_requests.load(Ordering::Relaxed);
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);

        let error_rate = if total > 0 {
            errors as f64 / total as f64
        } else {
            0.0
        };

        let cache_hit_rate = if (hits + misses) > 0 {
            hits as f64 / (hits + misses) as f64
        } else {
            0.0
        };

        // 计算延迟百分位
        let latency = {
            let samples = self.latency_samples.lock().await;
            if samples.is_empty() {
                LatencyStats::default()
            } else {
                let mut sorted: Vec<f64> = samples.iter().map(|s| s.latency_ms).collect();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

                let avg = sorted.iter().sum::<f64>() / sorted.len() as f64;
                let p50 = percentile(&sorted, 50.0);
                let p95 = percentile(&sorted, 95.0);
                let p99 = percentile(&sorted, 99.0);

                LatencyStats {
                    avg_ms: avg,
                    p50_ms: p50,
                    p95_ms: p95,
                    p99_ms: p99,
                    max_ms: sorted[sorted.len() - 1],
                    min_ms: sorted[0],
                }
            }
        };

        EdgeMetricsSnapshot {
            total_requests: total,
            success_requests: success,
            error_requests: errors,
            error_rate,
            latency,
            cache_hit_rate,
            cache_hits: hits,
            cache_misses: misses,
            resources: ResourceUsage::default(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 重置指标
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.success_requests.store(0, Ordering::Relaxed);
        self.error_requests.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
    }
}

impl Default for EdgeMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// 计算百分位
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 请求计时器
pub struct RequestTimer {
    start: Instant,
}

impl RequestTimer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_micros() as f64 / 1000.0
    }
}

impl Default for RequestTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_basic() {
        let collector = EdgeMetricsCollector::new();
        collector.record_request();
        collector.record_request();
        collector.record_success(10.0);
        collector.record_error();

        let snapshot = collector.snapshot().await;
        assert_eq!(snapshot.total_requests, 2);
        assert_eq!(snapshot.error_requests, 1);
        assert!((snapshot.error_rate - 0.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_metrics_cache_tracking() {
        let collector = EdgeMetricsCollector::new();
        collector.record_cache_hit();
        collector.record_cache_hit();
        collector.record_cache_miss();

        let snapshot = collector.snapshot().await;
        assert_eq!(snapshot.cache_hits, 2);
        assert_eq!(snapshot.cache_misses, 1);
        assert!((snapshot.cache_hit_rate - 0.667).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_metrics_latency() {
        let collector = EdgeMetricsCollector::new();
        collector.record_request();
        collector.record_success(5.0);
        collector.record_request();
        collector.record_success(15.0);
        collector.record_request();
        collector.record_success(10.0);

        // Wait for async latency recording
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let snapshot = collector.snapshot().await;
        assert!(snapshot.latency.avg_ms > 0.0);
    }

    #[test]
    fn test_percentile() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let p50 = percentile(&data, 50.0);
        assert!(
            p50 >= 5.0 && p50 <= 6.0,
            "P50 should be between 5.0 and 6.0, got {}",
            p50
        );
        assert_eq!(percentile(&data, 0.0), 1.0);
        assert_eq!(percentile(&data, 100.0), 10.0);
    }

    #[test]
    fn test_request_timer() {
        let timer = RequestTimer::new();
        let ms = timer.elapsed_ms();
        assert!(ms >= 0.0);
    }

    #[tokio::test]
    async fn test_metrics_reset() {
        let collector = EdgeMetricsCollector::new();
        collector.record_request();
        collector.record_error();
        collector.reset();

        let snapshot = collector.snapshot().await;
        assert_eq!(snapshot.total_requests, 0);
        assert_eq!(snapshot.error_requests, 0);
    }

    #[tokio::test]
    async fn test_latency_percentiles() {
        let collector = EdgeMetricsCollector::with_max_samples(100);
        for i in 1..=100 {
            collector.record_request();
            collector.record_success(i as f64);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let snapshot = collector.snapshot().await;
        assert!(snapshot.latency.p50_ms > 0.0);
        assert!(snapshot.latency.p95_ms >= snapshot.latency.p50_ms);
        assert!(snapshot.latency.p99_ms >= snapshot.latency.p95_ms);
    }

    #[tokio::test]
    async fn test_error_rate_calculation() {
        let collector = EdgeMetricsCollector::new();
        for _ in 0..8 {
            collector.record_request();
            collector.record_success(10.0);
        }
        for _ in 0..2 {
            collector.record_request();
            collector.record_error();
        }

        let snapshot = collector.snapshot().await;
        assert!((snapshot.error_rate - 0.2).abs() < 0.01);
        assert_eq!(snapshot.success_requests, 8);
        assert_eq!(snapshot.error_requests, 2);
    }

    #[test]
    fn test_percentile_empty() {
        let data: Vec<f64> = vec![];
        assert_eq!(percentile(&data, 50.0), 0.0);
    }

    #[tokio::test]
    async fn test_resource_usage_default() {
        let usage = ResourceUsage::default();
        assert_eq!(usage.memory_mb, 0.0);
        assert_eq!(usage.cpu_percent, 0.0);
    }

    #[tokio::test]
    async fn test_latency_stats_default() {
        let stats = LatencyStats::default();
        assert_eq!(stats.avg_ms, 0.0);
        assert_eq!(stats.max_ms, 0.0);
    }
}
