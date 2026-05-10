//! # Health Check 增强模块 — 组件级健康检查
//!
//! Phase 7: 可观测性增强
//!
//! - `HealthChecker`: 组件级健康检查
//! - `ReadinessProbe`: 就绪探针（数据库/缓存/上游连通性）
//! - `LivenessProbe`: 存活探针
//! - `/health`、`/ready`、`/live` 端点

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ─── HealthStatus ───────────────────────────────────────────

/// 组件健康状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    /// 健康
    Healthy,
    /// 降级（部分功能不可用）
    Degraded,
    /// 不健康
    Unhealthy,
    /// 未知
    Unknown,
}

impl ComponentStatus {
    /// 是否可用（Healthy 或 Degraded）
    pub fn is_available(&self) -> bool {
        matches!(self, ComponentStatus::Healthy | ComponentStatus::Degraded)
    }
}

// ─── HealthCheck Trait ──────────────────────────────────────

/// 健康检查接口
///
/// 每个组件（数据库、缓存、上游服务）实现此 trait。
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// 执行健康检查
    async fn check(&self) -> ComponentHealth;

    /// 组件名称
    fn name(&self) -> &str;

    /// 是否是关键组件（关键组件不健康 = 整体不健康）
    fn is_critical(&self) -> bool {
        true
    }
}

/// 组件健康结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// 组件名称
    pub name: String,
    /// 状态
    pub status: ComponentStatus,
    /// 检查耗时
    pub duration_ms: u64,
    /// 详细消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 额外元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl ComponentHealth {
    /// 健康组件
    pub fn healthy(name: &str, duration_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: ComponentStatus::Healthy,
            duration_ms,
            message: None,
            error: None,
            metadata: HashMap::new(),
        }
    }

    /// 健康组件（带消息）
    pub fn healthy_with_message(name: &str, duration_ms: u64, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: ComponentStatus::Healthy,
            duration_ms,
            message: Some(message.to_string()),
            error: None,
            metadata: HashMap::new(),
        }
    }

    /// 降级组件
    pub fn degraded(name: &str, duration_ms: u64, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: ComponentStatus::Degraded,
            duration_ms,
            message: Some(message.to_string()),
            error: None,
            metadata: HashMap::new(),
        }
    }

    /// 不健康组件
    pub fn unhealthy(name: &str, duration_ms: u64, error: &str) -> Self {
        Self {
            name: name.to_string(),
            status: ComponentStatus::Unhealthy,
            duration_ms,
            message: None,
            error: Some(error.to_string()),
            metadata: HashMap::new(),
        }
    }
}

// ─── HealthChecker ──────────────────────────────────────────

/// 健康检查器 — 管理所有组件的健康检查
pub struct HealthChecker {
    checks: Vec<Arc<dyn HealthCheck>>,
    /// 超时时间
    timeout: Duration,
    /// 启动时间
    start_time: Instant,
    /// 版本号
    version: String,
}

impl HealthChecker {
    /// 创建健康检查器
    pub fn new(version: &str) -> Self {
        Self {
            checks: Vec::new(),
            timeout: Duration::from_secs(5),
            start_time: Instant::now(),
            version: version.to_string(),
        }
    }

    /// 设置超时
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 注册健康检查组件
    pub fn register(&mut self, check: Arc<dyn HealthCheck>) {
        self.checks.push(check);
    }

    /// 执行所有组件健康检查
    pub async fn check_all(&self) -> OverallHealth {
        let mut components = Vec::new();

        for check in &self.checks {
            let name = check.name().to_string();
            let start = Instant::now();

            let result = tokio::time::timeout(self.timeout, check.check())
                .await
                .map(|r| r)
                .unwrap_or_else(|_| ComponentHealth {
                    name: name.clone(),
                    status: ComponentStatus::Unhealthy,
                    duration_ms: self.timeout.as_millis() as u64,
                    message: None,
                    error: Some("Health check timed out".to_string()),
                    metadata: HashMap::new(),
                });

            components.push(result);
        }

        let overall_status = self.compute_overall_status(&components);
        let uptime_secs = self.start_time.elapsed().as_secs();

        OverallHealth {
            status: overall_status,
            version: self.version.clone(),
            uptime_secs,
            components,
        }
    }

    /// 计算整体状态
    fn compute_overall_status(&self, components: &[ComponentHealth]) -> ComponentStatus {
        let mut has_degraded = false;

        for component in components {
            match component.status {
                ComponentStatus::Unhealthy => {
                    // Check if this component is critical
                    let is_critical = self.checks.iter()
                        .find(|c| c.name() == component.name)
                        .map(|c| c.is_critical())
                        .unwrap_or(true);

                    if is_critical {
                        return ComponentStatus::Unhealthy;
                    }
                    has_degraded = true;
                }
                ComponentStatus::Degraded => {
                    has_degraded = true;
                }
                ComponentStatus::Unknown => {
                    has_degraded = true;
                }
                ComponentStatus::Healthy => {}
            }
        }

        if has_degraded {
            ComponentStatus::Degraded
        } else {
            ComponentStatus::Healthy
        }
    }

    /// 获取运行时间
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

/// 整体健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallHealth {
    /// 整体状态
    pub status: ComponentStatus,
    /// 版本号
    pub version: String,
    /// 运行时间（秒）
    pub uptime_secs: u64,
    /// 各组件健康状态
    pub components: Vec<ComponentHealth>,
}

// ─── ReadinessProbe ─────────────────────────────────────────

/// 就绪探针 — 检查服务是否准备好接收流量
///
/// 就绪 = 所有关键组件都正常（数据库、缓存、上游连通性）
pub struct ReadinessProbe {
    health_checker: Arc<HealthChecker>,
}

impl ReadinessProbe {
    pub fn new(health_checker: Arc<HealthChecker>) -> Self {
        Self { health_checker }
    }

    /// 执行就绪检查
    pub async fn check(&self) -> ReadinessResult {
        let health = self.health_checker.check_all().await;
        let ready = health.status.is_available();

        ReadinessResult {
            ready,
            status: health.status,
            components: health.components,
        }
    }
}

/// 就绪检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResult {
    /// 是否就绪
    pub ready: bool,
    /// 整体状态
    pub status: ComponentStatus,
    /// 组件详情
    pub components: Vec<ComponentHealth>,
}

// ─── LivenessProbe ──────────────────────────────────────────

/// 存活探针 — 检查服务是否还活着
///
/// 存活 = 进程未死锁、内存未耗尽、基本逻辑可用
pub struct LivenessProbe {
    /// 上次检查时间
    last_check: parking_lot::Mutex<Instant>,
    /// 内存阈值（字节），超过则不健康
    memory_threshold: Option<usize>,
    /// 自定义存活检查
    custom_check: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl LivenessProbe {
    /// 创建存活探针
    pub fn new() -> Self {
        Self {
            last_check: parking_lot::Mutex::new(Instant::now()),
            memory_threshold: None,
            custom_check: None,
        }
    }

    /// 设置内存阈值
    pub fn with_memory_threshold(mut self, threshold: usize) -> Self {
        self.memory_threshold = Some(threshold);
        self
    }

    /// 设置自定义存活检查
    pub fn with_custom_check(mut self, check: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        self.custom_check = Some(check);
        self
    }

    /// 执行存活检查
    pub fn check(&self) -> LivenessResult {
        *self.last_check.lock() = Instant::now();

        let mut alive = true;
        let mut details = HashMap::new();

        // Check 1: Process is responsive (we're here, so yes)
        details.insert("responsive".to_string(), "true".to_string());

        // Check 2: Memory usage
        if let Some(threshold) = self.memory_threshold {
            let usage = self.estimate_memory_usage();
            details.insert("memory_usage_bytes".to_string(), usage.to_string());
            if usage > threshold {
                alive = false;
                details.insert("memory_threshold_exceeded".to_string(), "true".to_string());
            }
        }

        // Check 3: Custom check
        if let Some(ref custom_check) = self.custom_check {
            let result = custom_check();
            details.insert("custom_check".to_string(), result.to_string());
            if !result {
                alive = false;
            }
        }

        LivenessResult { alive, details }
    }

    /// 估算当前内存使用量
    fn estimate_memory_usage(&self) -> usize {
        // Use a simple heuristic based on Rust's global allocator stats
        // In production, use jemalloc stats or similar
        // For now, return 0 as a placeholder (always passes memory check)
        0
    }

    /// 获取上次检查时间
    pub fn last_check_time(&self) -> Instant {
        *self.last_check.lock()
    }
}

impl Default for LivenessProbe {
    fn default() -> Self {
        Self::new()
    }
}

/// 存活检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessResult {
    /// 是否存活
    pub alive: bool,
    /// 详细信息
    pub details: HashMap<String, String>,
}

// ─── Built-in Health Checks ─────────────────────────────────

/// 数据库健康检查
pub struct DatabaseHealthCheck {
    name: String,
    check_fn: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl DatabaseHealthCheck {
    pub fn new(name: &str, check_fn: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self {
            name: name.to_string(),
            check_fn,
        }
    }

    /// 创建一个简单的 ping 检查
    pub fn ping_check(name: &str) -> Self {
        Self {
            name: name.to_string(),
            check_fn: Arc::new(|| true), // Placeholder, replace with actual DB ping
        }
    }
}

#[async_trait]
impl HealthCheck for DatabaseHealthCheck {
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();
        let result = (self.check_fn)();
        let duration_ms = start.elapsed().as_millis() as u64;

        if result {
            ComponentHealth::healthy(&self.name, duration_ms)
        } else {
            ComponentHealth::unhealthy(&self.name, duration_ms, "Database ping failed")
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_critical(&self) -> bool {
        true
    }
}

/// 缓存健康检查
pub struct CacheHealthCheck {
    name: String,
    check_fn: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl CacheHealthCheck {
    pub fn new(name: &str, check_fn: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self {
            name: name.to_string(),
            check_fn,
        }
    }

    /// 非关键组件
    pub fn non_critical(name: &str, check_fn: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self {
            name: name.to_string(),
            check_fn,
        }
    }
}

#[async_trait]
impl HealthCheck for CacheHealthCheck {
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();
        let result = (self.check_fn)();
        let duration_ms = start.elapsed().as_millis() as u64;

        if result {
            ComponentHealth::healthy(&self.name, duration_ms)
        } else {
            // Cache failure = degraded, not unhealthy
            ComponentHealth::degraded(&self.name, duration_ms, "Cache not available")
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_critical(&self) -> bool {
        false // Cache failure is not critical
    }
}

/// 上游服务健康检查
pub struct UpstreamHealthCheck {
    name: String,
    url: String,
    critical: bool,
}

impl UpstreamHealthCheck {
    pub fn new(name: &str, url: &str, critical: bool) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
            critical,
        }
    }
}

#[async_trait]
impl HealthCheck for UpstreamHealthCheck {
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();

        // Simple TCP/connection check
        let result = check_upstream_connectivity(&self.url).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(()) => ComponentHealth::healthy(&self.name, duration_ms),
            Err(e) => {
                if self.critical {
                    ComponentHealth::unhealthy(&self.name, duration_ms, &e)
                } else {
                    ComponentHealth::degraded(&self.name, duration_ms, &e)
                }
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_critical(&self) -> bool {
        self.critical
    }
}

/// 检查上游连通性（TCP 连接测试）
async fn check_upstream_connectivity(url: &str) -> Result<(), String> {
    // Parse the URL to extract host and port
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    let host = parsed.host_str().ok_or("Missing host")?;
    let port = parsed.port_or_known_default().ok_or("Missing port")?;

    // Attempt TCP connection
    let addr = format!("{}:{}", host, port);
    tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    .map_err(|_| "Connection timed out".to_string())?
    .map_err(|e| format!("Connection failed: {}", e))?;

    Ok(())
}

// ─── HealthEndpoint ─────────────────────────────────────────

/// 健康检查端点 — 聚合所有探针
///
/// 提供 `/health`、`/ready`、`/live` 端点的逻辑。
pub struct HealthEndpoint {
    health_checker: Arc<HealthChecker>,
    readiness_probe: ReadinessProbe,
    liveness_probe: parking_lot::Mutex<LivenessProbe>,
}

impl HealthEndpoint {
    /// 创建健康端点
    pub fn new(health_checker: Arc<HealthChecker>) -> Self {
        let readiness_probe = ReadinessProbe::new(health_checker.clone());
        let liveness_probe = LivenessProbe::new();

        Self {
            health_checker,
            readiness_probe,
            liveness_probe: parking_lot::Mutex::new(liveness_probe),
        }
    }

    /// `/health` — 整体健康状态
    pub async fn health(&self) -> OverallHealth {
        self.health_checker.check_all().await
    }

    /// `/ready` — 就绪检查
    pub async fn ready(&self) -> ReadinessResult {
        self.readiness_probe.check().await
    }

    /// `/live` — 存活检查
    pub fn live(&self) -> LivenessResult {
        self.liveness_probe.lock().check()
    }

    /// 注册健康检查组件
    pub fn register_check(&self, check: Arc<dyn HealthCheck>) {
        // Note: This requires interior mutability on HealthChecker
        // In practice, register before creating the endpoint
        // This is a design limitation; use `register_before_endpoint` pattern
        unimplemented!("Register checks before creating HealthEndpoint")
    }

    /// 设置内存阈值
    pub fn set_memory_threshold(&self, threshold: usize) {
        self.liveness_probe.lock().memory_threshold = Some(threshold);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysHealthyCheck {
        name: String,
    }

    #[async_trait]
    impl HealthCheck for AlwaysHealthyCheck {
        async fn check(&self) -> ComponentHealth {
            ComponentHealth::healthy(&self.name, 1)
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    struct AlwaysUnhealthyCheck {
        name: String,
        critical: bool,
    }

    #[async_trait]
    impl HealthCheck for AlwaysUnhealthyCheck {
        async fn check(&self) -> ComponentHealth {
            ComponentHealth::unhealthy(&self.name, 1, "Component is down")
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn is_critical(&self) -> bool {
            self.critical
        }
    }

    #[tokio::test]
    async fn test_health_checker_all_healthy() {
        let mut checker = HealthChecker::new("0.2.0");
        checker.register(Arc::new(AlwaysHealthyCheck { name: "db".to_string() }));
        checker.register(Arc::new(AlwaysHealthyCheck { name: "cache".to_string() }));

        let health = checker.check_all().await;
        assert_eq!(health.status, ComponentStatus::Healthy);
        assert_eq!(health.components.len(), 2);
    }

    #[tokio::test]
    async fn test_health_checker_critical_unhealthy() {
        let mut checker = HealthChecker::new("0.2.0");
        checker.register(Arc::new(AlwaysHealthyCheck { name: "cache".to_string() }));
        checker.register(Arc::new(AlwaysUnhealthyCheck {
            name: "db".to_string(),
            critical: true,
        }));

        let health = checker.check_all().await;
        assert_eq!(health.status, ComponentStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_health_checker_non_critical_unhealthy() {
        let mut checker = HealthChecker::new("0.2.0");
        checker.register(Arc::new(AlwaysHealthyCheck { name: "db".to_string() }));
        checker.register(Arc::new(AlwaysUnhealthyCheck {
            name: "cache".to_string(),
            critical: false,
        }));

        let health = checker.check_all().await;
        assert_eq!(health.status, ComponentStatus::Degraded);
    }

    #[tokio::test]
    async fn test_readiness_probe() {
        let mut checker = HealthChecker::new("0.2.0");
        checker.register(Arc::new(AlwaysHealthyCheck { name: "db".to_string() }));

        let probe = ReadinessProbe::new(Arc::new(checker));
        let result = probe.check().await;
        assert!(result.ready);
    }

    #[tokio::test]
    async fn test_readiness_probe_not_ready() {
        let mut checker = HealthChecker::new("0.2.0");
        checker.register(Arc::new(AlwaysUnhealthyCheck {
            name: "db".to_string(),
            critical: true,
        }));

        let probe = ReadinessProbe::new(Arc::new(checker));
        let result = probe.check().await;
        assert!(!result.ready);
    }

    #[test]
    fn test_liveness_probe() {
        let probe = LivenessProbe::new();
        let result = probe.check();
        assert!(result.alive);
    }

    #[test]
    fn test_component_status_is_available() {
        assert!(ComponentStatus::Healthy.is_available());
        assert!(ComponentStatus::Degraded.is_available());
        assert!(!ComponentStatus::Unhealthy.is_available());
        assert!(!ComponentStatus::Unknown.is_available());
    }

    #[test]
    fn test_component_health_constructors() {
        let h = ComponentHealth::healthy("test", 10);
        assert_eq!(h.status, ComponentStatus::Healthy);

        let d = ComponentHealth::degraded("test", 10, "slow");
        assert_eq!(d.status, ComponentStatus::Degraded);

        let u = ComponentHealth::unhealthy("test", 10, "error");
        assert_eq!(u.status, ComponentStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let mut checker = HealthChecker::new("0.2.0");
        checker.register(Arc::new(AlwaysHealthyCheck { name: "db".to_string() }));

        let endpoint = HealthEndpoint::new(Arc::new(checker));
        let health = endpoint.health().await;
        assert_eq!(health.status, ComponentStatus::Healthy);

        let ready = endpoint.ready().await;
        assert!(ready.ready);

        let live = endpoint.live();
        assert!(live.alive);
    }
}
