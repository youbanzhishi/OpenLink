//! # 监控与告警
//!
//! Prometheus 指标导出 + 健康检查端点
//!
//! ## 指标
//! - 请求量 (counter)
//! - 延迟 (histogram)
//! - 错误率 (counter)
//! - 缓存命中率 (gauge)
//! - 活跃连接数 (gauge)

use prometheus::{
    CounterVec, Encoder, Gauge, HistogramOpts, HistogramVec, Opts, Registry, TextEncoder,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 应用指标
pub struct AppMetrics {
    /// 请求计数器
    pub requests_total: CounterVec,

    /// 请求延迟
    pub request_duration: HistogramVec,

    /// 错误计数器
    pub errors_total: CounterVec,

    /// 缓存命中率
    pub cache_hits: Gauge,
    pub cache_misses: Gauge,

    /// 活跃请求数
    pub active_requests: Gauge,

    /// 注册表
    registry: Registry,
}

impl AppMetrics {
    /// 创建新指标
    pub fn new() -> Self {
        let registry = Registry::new();

        // 请求计数器
        let requests_total = CounterVec::new(
            Opts::new("openlink_requests_total", "Total requests"),
            &["method", "endpoint", "status"],
        )
        .expect("Failed to create requests counter");
        registry
            .register(Box::new(requests_total.clone()))
            .expect("Failed to register requests counter");

        // 请求延迟
        let request_duration = HistogramVec::new(
            HistogramOpts::new("openlink_request_duration_seconds", "Request duration")
                .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),
            &["method", "endpoint"],
        )
        .expect("Failed to create duration histogram");
        registry
            .register(Box::new(request_duration.clone()))
            .expect("Failed to register duration histogram");

        // 错误计数器
        let errors_total = CounterVec::new(
            Opts::new("openlink_errors_total", "Total errors"),
            &["type", "endpoint"],
        )
        .expect("Failed to create errors counter");
        registry
            .register(Box::new(errors_total.clone()))
            .expect("Failed to register errors counter");

        // 缓存命中率
        let cache_hits =
            Gauge::new("openlink_cache_hits_total", "Cache hits").expect("Failed to create cache hits gauge");
        registry
            .register(Box::new(cache_hits.clone()))
            .expect("Failed to register cache hits gauge");

        let cache_misses =
            Gauge::new("openlink_cache_misses_total", "Cache misses").expect("Failed to create cache misses gauge");
        registry
            .register(Box::new(cache_misses.clone()))
            .expect("Failed to register cache misses gauge");

        // 活跃请求
        let active_requests =
            Gauge::new("openlink_active_requests", "Active requests").expect("Failed to create active requests gauge");
        registry
            .register(Box::new(active_requests.clone()))
            .expect("Failed to register active requests gauge");

        Self {
            requests_total,
            request_duration,
            errors_total,
            cache_hits,
            cache_misses,
            active_requests,
            registry,
        }
    }

    /// 记录请求
    pub fn record_request(&self, method: &str, endpoint: &str, status: u16, duration: Duration) {
        let status_str = status.to_string();
        self.requests_total
            .with_label_values(&[method, endpoint, &status_str])
            .inc();
        self.request_duration
            .with_label_values(&[method, endpoint])
            .observe(duration.as_secs_f64());
    }

    /// 记录错误
    pub fn record_error(&self, error_type: &str, endpoint: &str) {
        self.errors_total.with_label_values(&[error_type, endpoint]).inc();
    }

    /// 记录缓存命中
    pub fn record_cache_hit(&self) {
        self.cache_hits.inc();
    }

    /// 记录缓存未命中
    pub fn record_cache_miss(&self) {
        self.cache_misses.inc();
    }

    /// 增加活跃请求
    pub fn request_started(&self) {
        self.active_requests.inc();
    }

    /// 减少活跃请求
    pub fn request_finished(&self) {
        self.active_requests.dec();
    }

    /// 获取 Prometheus 格式的指标
    pub async fn gather(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for AppMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 请求计时器
pub struct RequestTimer {
    start: Instant,
    metrics: Arc<AppMetrics>,
    method: String,
    endpoint: String,
}

impl RequestTimer {
    /// 开始计时
    pub fn new(metrics: Arc<AppMetrics>, method: &str, endpoint: &str) -> Self {
        metrics.request_started();
        Self {
            start: Instant::now(),
            metrics,
            method: method.to_string(),
            endpoint: endpoint.to_string(),
        }
    }

    /// 结束计时并记录
    pub fn finish(self, status: u16) {
        let duration = self.start.elapsed();
        self.metrics
            .record_request(&self.method, &self.endpoint, status, duration);
        self.metrics.request_finished();
    }
}

/// 健康检查状态
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub version: String,
    pub uptime_secs: u64,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
}

impl HealthStatus {
    /// 创建健康状态
    pub fn healthy(version: &str, uptime_secs: u64) -> Self {
        Self {
            healthy: true,
            version: version.to_string(),
            uptime_secs,
            checks: vec![HealthCheck {
                name: "service".to_string(),
                status: "ok".to_string(),
                message: None,
            }],
        }
    }

    /// 创建不健康状态
    pub fn unhealthy(version: &str, uptime_secs: u64, message: &str) -> Self {
        Self {
            healthy: false,
            version: version.to_string(),
            uptime_secs,
            checks: vec![HealthCheck {
                name: "service".to_string(),
                status: "error".to_string(),
                message: Some(message.to_string()),
            }],
        }
    }
}
