//! # Metrics 模块 — 统一指标收集与导出
//!
//! Phase 7: 可观测性增强
//!
//! - `MetricsCollector` trait: 统一的指标收集接口
//! - `PrometheusExporter`: Prometheus 格式导出
//! - 内置指标：请求总数、延迟分布(P50/P95/P99)、缓存命中率、活跃链接数、错误率
//! - `MetricsMiddleware`: HTTP 中间件自动收集指标

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ─── MetricsCollector Trait ─────────────────────────────────

/// 统一的指标收集接口
///
/// 任何后端（Prometheus、StatsD、OpenTelemetry）都实现此 trait，
/// 上层代码只依赖 trait，不依赖具体实现。
#[async_trait]
pub trait MetricsCollector: Send + Sync {
    /// 递增计数器
    fn increment_counter(&self, name: &str, labels: &HashMap<&str, &str>, delta: f64);

    /// 设置 Gauge 值
    fn set_gauge(&self, name: &str, labels: &HashMap<&str, &str>, value: f64);

    /// 递增 Gauge
    fn increment_gauge(&self, name: &str, labels: &HashMap<&str, &str>, delta: f64);

    /// 递减 Gauge
    fn decrement_gauge(&self, name: &str, labels: &HashMap<&str, &str>, delta: f64);

    /// 记录 Histogram 观察
    fn observe_histogram(&self, name: &str, labels: &HashMap<&str, &str>, value: f64);

    /// 记录请求（便捷方法）
    fn record_request(&self, method: &str, endpoint: &str, status: u16, duration: Duration) {
        let mut labels = HashMap::new();
        labels.insert("method", method);
        labels.insert("endpoint", endpoint);
        let status_str = Box::leak(status.to_string().into_boxed_str());
        labels.insert("status", status_str);
        self.increment_counter("requests_total", &labels, 1.0);

        let mut latency_labels = HashMap::new();
        latency_labels.insert("method", method);
        latency_labels.insert("endpoint", endpoint);
        self.observe_histogram("request_duration_seconds", &latency_labels, duration.as_secs_f64());
    }

    /// 记录错误（便捷方法）
    fn record_error(&self, error_type: &str, endpoint: &str) {
        let mut labels = HashMap::new();
        labels.insert("type", error_type);
        labels.insert("endpoint", endpoint);
        self.increment_counter("errors_total", &labels, 1.0);
    }

    /// 记录缓存命中
    fn record_cache_hit(&self) {
        let labels = HashMap::new();
        self.increment_counter("cache_hits_total", &labels, 1.0);
    }

    /// 记录缓存未命中
    fn record_cache_miss(&self) {
        let labels = HashMap::new();
        self.increment_counter("cache_misses_total", &labels, 1.0);
    }

    /// 活跃请求开始
    fn request_started(&self) {
        let labels = HashMap::new();
        self.increment_gauge("active_requests", &labels, 1.0);
    }

    /// 活跃请求结束
    fn request_finished(&self) {
        let labels = HashMap::new();
        self.decrement_gauge("active_requests", &labels, 1.0);
    }

    /// 设置活跃链接数
    fn set_active_links(&self, count: f64) {
        let labels = HashMap::new();
        self.set_gauge("active_links", &labels, count);
    }

    /// 导出指标（格式由实现决定）
    async fn export(&self) -> String;
}

// ─── InMemoryMetrics ────────────────────────────────────────

/// 进程内指标收集器（用于测试和简单部署）
pub struct InMemoryMetrics {
    counters: parking_lot::RwLock<HashMap<String, AtomicU64>>,
    gauges: parking_lot::RwLock<HashMap<String, AtomicI64>>,
    histograms: parking_lot::RwLock<HashMap<String, parking_lot::Mutex<Vec<f64>>>>,
    start_time: Instant,
}

impl InMemoryMetrics {
    pub fn new() -> Self {
        Self {
            counters: parking_lot::RwLock::new(HashMap::new()),
            gauges: parking_lot::RwLock::new(HashMap::new()),
            histograms: parking_lot::RwLock::new(HashMap::new()),
            start_time: Instant::now(),
        }
    }

    /// 生成指标键
    fn metric_key(name: &str, labels: &HashMap<&str, &str>) -> String {
        let mut parts: Vec<String> = labels.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        parts.sort();
        if parts.is_empty() {
            name.to_string()
        } else {
            format!("{}{{{}}}", name, parts.join(","))
        }
    }

    /// 计算 histogram 百分位
    fn percentile(data: &[f64], p: f64) -> f64 {
        if data.is_empty() {
            return 0.0;
        }
        let mut sorted = data.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).floor() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}

impl Default for InMemoryMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetricsCollector for InMemoryMetrics {
    fn increment_counter(&self, name: &str, labels: &HashMap<&str, &str>, delta: f64) {
        let key = Self::metric_key(name, labels);
        let map = self.counters.read();
        if let Some(counter) = map.get(&key) {
            counter.fetch_add(delta as u64, Ordering::Relaxed);
        } else {
            drop(map);
            let mut map = self.counters.write();
            map.entry(key).or_insert_with(|| AtomicU64::new(0))
                .fetch_add(delta as u64, Ordering::Relaxed);
        }
    }

    fn set_gauge(&self, name: &str, labels: &HashMap<&str, &str>, value: f64) {
        let key = Self::metric_key(name, labels);
        let map = self.gauges.read();
        if let Some(gauge) = map.get(&key) {
            gauge.store(value as i64, Ordering::Relaxed);
        } else {
            drop(map);
            let mut map = self.gauges.write();
            map.entry(key).or_insert_with(|| AtomicI64::new(value as i64));
        }
    }

    fn increment_gauge(&self, name: &str, labels: &HashMap<&str, &str>, delta: f64) {
        let key = Self::metric_key(name, labels);
        let map = self.gauges.read();
        if let Some(gauge) = map.get(&key) {
            gauge.fetch_add(delta as i64, Ordering::Relaxed);
        } else {
            drop(map);
            let mut map = self.gauges.write();
            map.entry(key).or_insert_with(|| AtomicI64::new(0))
                .fetch_add(delta as i64, Ordering::Relaxed);
        }
    }

    fn decrement_gauge(&self, name: &str, labels: &HashMap<&str, &str>, delta: f64) {
        let key = Self::metric_key(name, labels);
        let map = self.gauges.read();
        if let Some(gauge) = map.get(&key) {
            gauge.fetch_sub(delta as i64, Ordering::Relaxed);
        } else {
            drop(map);
            let mut map = self.gauges.write();
            map.entry(key).or_insert_with(|| AtomicI64::new(0))
                .fetch_sub(delta as i64, Ordering::Relaxed);
        }
    }

    fn observe_histogram(&self, name: &str, labels: &HashMap<&str, &str>, value: f64) {
        let key = Self::metric_key(name, labels);
        let map = self.histograms.read();
        if let Some(hist) = map.get(&key) {
            hist.lock().push(value);
        } else {
            drop(map);
            let mut map = self.histograms.write();
            map.entry(key).or_insert_with(|| parking_lot::Mutex::new(Vec::new()))
                .lock().push(value);
        }
    }

    async fn export(&self) -> String {
        let mut lines = Vec::new();

        // Counters
        for (key, counter) in self.counters.read().iter() {
            lines.push(format!("{} {}", key, counter.load(Ordering::Relaxed)));
        }

        // Gauges
        for (key, gauge) in self.gauges.read().iter() {
            lines.push(format!("{} {}", key, gauge.load(Ordering::Relaxed)));
        }

        // Histograms with percentiles
        for (key, hist) in self.histograms.read().iter() {
            let data = hist.lock();
            let count = data.len();
            let sum: f64 = data.iter().sum();
            lines.push(format!("{}_count {}", key, count));
            lines.push(format!("{}_sum {}", key, sum));
            if !data.is_empty() {
                lines.push(format!("{}_p50 {}", key, Self::percentile(&data, 50.0)));
                lines.push(format!("{}_p95 {}", key, Self::percentile(&data, 95.0)));
                lines.push(format!("{}_p99 {}", key, Self::percentile(&data, 99.0)));
            }
        }

        lines.push(format!("uptime_seconds {}", self.start_time.elapsed().as_secs()));

        lines.join("\n")
    }
}

// ─── PrometheusExporter ─────────────────────────────────────

/// Prometheus 格式指标导出器
///
/// 将 InMemoryMetrics 或其他指标数据导出为 Prometheus text exposition format。
/// 设计为可以包裹任何 `MetricsCollector` 实例，在其 export 基础上
/// 增加 Prometheus 标准格式化。
pub struct PrometheusExporter {
    inner: Arc<dyn MetricsCollector>,
}

impl PrometheusExporter {
    pub fn new(inner: Arc<dyn MetricsCollector>) -> Self {
        Self { inner }
    }

    /// 导出 Prometheus 格式文本
    pub async fn render(&self) -> String {
        let raw = self.inner.export().await;
        Self::to_prometheus_format(&raw)
    }

    /// 将内部格式转换为 Prometheus exposition format
    fn to_prometheus_format(raw: &str) -> String {
        let mut output = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Detect metric type and reformat
            if line.contains("_count ") || line.contains("_sum ") {
                // Histogram bucket — already valid Prometheus
                output.push(format!("openlink_{}", line));
            } else if line.contains("_p50 ") || line.contains("_p95 ") || line.contains("_p99 ") {
                // Percentile → Prometheus summary quantile
                let parts: Vec<&str> = line.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    let metric_part = parts[0];
                    let value = parts[1];
                    // Extract quantile from suffix
                    let quantile = if metric_part.ends_with("_p50") {
                        "0.5"
                    } else if metric_part.ends_with("_p95") {
                        "0.95"
                    } else if metric_part.ends_with("_p99") {
                        "0.99"
                    } else {
                        "0.5"
                    };
                    // Strip _pXX suffix
                    let base = metric_part
                        .trim_end_matches("_p50")
                        .trim_end_matches("_p95")
                        .trim_end_matches("_p99");
                    // Reconstruct with quantile label
                    if let Some(brace) = base.find('{') {
                        let name = &base[..brace];
                        let labels = &base[brace..];
                        // Insert quantile label
                        output.push(format!(
                            "openlink_{}{{quantile=\"{}\",{}}}",
                            name,
                            quantile,
                            &labels[1..labels.len()-1] // strip { }
                        ));
                    } else {
                        output.push(format!("openlink_{}{{quantile=\"{}\"}} {}", base, quantile, value));
                    }
                }
            } else if line.contains("uptime_seconds") {
                output.push(format!("openlink_{}", line));
            } else {
                output.push(format!("openlink_{}", line));
            }
        }
        output.join("\n")
    }
}

// ─── LatencyTracker ─────────────────────────────────────────

/// 延迟追踪器 — 独立的延迟统计，计算 P50/P95/P99
pub struct LatencyTracker {
    samples: parking_lot::Mutex<Vec<f64>>,
    max_samples: usize,
}

impl LatencyTracker {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: parking_lot::Mutex::new(Vec::with_capacity(max_samples)),
            max_samples,
        }
    }

    /// 记录一次延迟观察
    pub fn observe(&self, duration: Duration) {
        let mut samples = self.samples.lock();
        samples.push(duration.as_secs_f64());
        if samples.len() > self.max_samples {
            samples.remove(0);
        }
    }

    /// 获取 P50
    pub fn p50(&self) -> Duration {
        self.percentile(50.0)
    }

    /// 获取 P95
    pub fn p95(&self) -> Duration {
        self.percentile(95.0)
    }

    /// 获取 P99
    pub fn p99(&self) -> Duration {
        self.percentile(99.0)
    }

    fn percentile(&self, p: f64) -> Duration {
        let samples = self.samples.lock();
        if samples.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).floor() as usize;
        Duration::from_secs_f64(sorted[idx.min(sorted.len() - 1)])
    }

    /// 获取样本数
    pub fn count(&self) -> usize {
        self.samples.lock().len()
    }

    /// 获取平均值
    pub fn avg(&self) -> Duration {
        let samples = self.samples.lock();
        if samples.is_empty() {
            return Duration::ZERO;
        }
        let sum: f64 = samples.iter().sum();
        Duration::from_secs_f64(sum / samples.len() as f64)
    }
}

// ─── MetricsMiddleware ──────────────────────────────────────

/// HTTP 指标中间件 — 自动收集请求指标
///
/// 用法：在每个请求处理前后自动记录请求计数、延迟、活跃连接等。
/// 此结构不直接绑定 axum/actix，而是提供通用的中间件逻辑。
pub struct MetricsMiddleware {
    collector: Arc<dyn MetricsCollector>,
}

impl MetricsMiddleware {
    pub fn new(collector: Arc<dyn MetricsCollector>) -> Self {
        Self { collector }
    }

    /// 请求开始：增加活跃请求计数，返回计时器
    pub fn on_request_start(&self, method: &str, endpoint: &str) -> RequestMetricsTimer {
        self.collector.request_started();
        RequestMetricsTimer {
            collector: self.collector.clone(),
            start: Instant::now(),
            method: method.to_string(),
            endpoint: endpoint.to_string(),
        }
    }

    /// 获取底层收集器引用
    pub fn collector(&self) -> Arc<dyn MetricsCollector> {
        self.collector.clone()
    }
}

/// 请求计时器 — RAII 模式，drop 时自动记录
pub struct RequestMetricsTimer {
    collector: Arc<dyn MetricsCollector>,
    start: Instant,
    method: String,
    endpoint: String,
}

impl RequestMetricsTimer {
    /// 手动结束计时并记录指标
    pub fn finish(self, status: u16) {
        let duration = self.start.elapsed();
        self.collector.record_request(&self.method, &self.endpoint, status, duration);
        self.collector.request_finished();
    }

    /// 手动结束计时并记录错误
    pub fn finish_with_error(self, status: u16, error_type: &str) {
        let duration = self.start.elapsed();
        self.collector.record_request(&self.method, &self.endpoint, status, duration);
        self.collector.record_error(error_type, &self.endpoint);
        self.collector.request_finished();
    }

    /// 获取已过时间
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

// ─── CacheMetrics ───────────────────────────────────────────

/// 缓存指标追踪器 — 独立的缓存命中率统计
pub struct CacheMetrics {
    hits: AtomicU64,
    misses: AtomicU64,
    collector: Option<Arc<dyn MetricsCollector>>,
}

impl CacheMetrics {
    pub fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            collector: None,
        }
    }

    pub fn with_collector(collector: Arc<dyn MetricsCollector>) -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            collector: Some(collector),
        }
    }

    /// 记录缓存命中
    pub fn hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
        if let Some(ref c) = self.collector {
            c.record_cache_hit();
        }
    }

    /// 记录缓存未命中
    pub fn miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
        if let Some(ref c) = self.collector {
            c.record_cache_miss();
        }
    }

    /// 获取命中率 (0.0 ~ 1.0)
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed) as f64;
        let misses = self.misses.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total == 0.0 {
            0.0
        } else {
            hits / total
        }
    }

    /// 获取命中数
    pub fn total_hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// 获取未命中数
    pub fn total_misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
}

impl Default for CacheMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ─── MetricsSnapshot ────────────────────────────────────────

/// 指标快照 — 某一时刻的指标摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// 快照时间戳
    pub timestamp: i64,
    /// 请求总数
    pub requests_total: u64,
    /// 错误总数
    pub errors_total: u64,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// 活跃请求数
    pub active_requests: i64,
    /// 活跃链接数
    pub active_links: i64,
    /// P50 延迟（秒）
    pub latency_p50: f64,
    /// P95 延迟（秒）
    pub latency_p95: f64,
    /// P99 延迟（秒）
    pub latency_p99: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_tracker() {
        let tracker = LatencyTracker::new(100);
        tracker.observe(Duration::from_millis(10));
        tracker.observe(Duration::from_millis(20));
        tracker.observe(Duration::from_millis(100));

        assert!(tracker.p50() > Duration::ZERO);
        assert!(tracker.p95() >= tracker.p50());
        assert!(tracker.p99() >= tracker.p95());
        assert_eq!(tracker.count(), 3);
    }

    #[test]
    fn test_cache_metrics() {
        let metrics = CacheMetrics::new();
        metrics.hit();
        metrics.hit();
        metrics.miss();

        assert_eq!(metrics.total_hits(), 2);
        assert_eq!(metrics.total_misses(), 1);
        // 2/3 ≈ 0.6667
        assert!(metrics.hit_rate() > 0.66 && metrics.hit_rate() < 0.67);
    }

    #[test]
    fn test_cache_metrics_empty() {
        let metrics = CacheMetrics::new();
        assert_eq!(metrics.hit_rate(), 0.0);
    }

    #[test]
    fn test_in_memory_metrics_counter() {
        let metrics = InMemoryMetrics::new();
        let labels = HashMap::new();
        metrics.increment_counter("test_counter", &labels, 1.0);
        metrics.increment_counter("test_counter", &labels, 2.0);
        // Counter should be 3 after two increments
    }

    #[test]
    fn test_in_memory_metrics_gauge() {
        let metrics = InMemoryMetrics::new();
        let labels = HashMap::new();
        metrics.set_gauge("test_gauge", &labels, 42.0);
        metrics.increment_gauge("test_gauge", &labels, 8.0);
        // Gauge should be 50
        metrics.decrement_gauge("test_gauge", &labels, 10.0);
        // Gauge should be 40
    }

    #[test]
    fn test_request_metrics_timer() {
        let metrics = Arc::new(InMemoryMetrics::new());
        let middleware = MetricsMiddleware::new(metrics.clone());
        let timer = middleware.on_request_start("GET", "/health");
        let elapsed = timer.elapsed();
        assert!(elapsed >= Duration::ZERO);
    }
}
