//! # Rate Limiting 模块 — 限流器
//!
//! Phase 7: 安全加固
//!
//! - `RateLimiter` trait: 限流器接口
//! - `TokenBucketLimiter`: 令牌桶实现
//! - `SlidingWindowLimiter`: 滑动窗口实现
//! - `RateLimitConfig`: 按 IP / 按 Key / 全局 限流配置
//! - `RateLimitMiddleware`: HTTP 中间件

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

// ─── RateLimiter Trait ──────────────────────────────────────

/// 限流器接口
///
/// 所有限流算法（令牌桶、滑动窗口、漏桶等）都实现此 trait。
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// 尝试获取一个配额，返回是否允许
    async fn try_acquire(&self, key: &str) -> RateLimitResult;

    /// 尝试获取 N 个配额
    async fn try_acquire_n(&self, key: &str, n: u32) -> RateLimitResult;

    /// 重置某个 key 的配额
    async fn reset(&self, key: &str);

    /// 获取某个 key 的当前配额状态
    async fn status(&self, key: &str) -> RateLimitStatus;

    /// 获取限流器名称
    fn name(&self) -> &str;
}

/// 限流结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    /// 是否允许
    pub allowed: bool,
    /// 剩余配额
    pub remaining: u32,
    /// 重置时间（秒）
    pub reset_after_secs: f64,
    /// 限流后等待时间（秒），仅 allowed=false 时有效
    pub retry_after_secs: Option<f64>,
}

/// 限流状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    /// 总配额
    pub limit: u32,
    /// 剩余配额
    pub remaining: u32,
    /// 重置时间（秒）
    pub reset_after_secs: f64,
}

// ─── TokenBucketLimiter ─────────────────────────────────────

/// 令牌桶限流器
///
/// 每秒以固定速率向桶中添加令牌，请求消耗令牌。
/// 允许突发流量（桶中有足够令牌时）。
///
/// 适用于：允许短时突发、但平均速率受限的场景。
pub struct TokenBucketLimiter {
    /// 桶容量（最大令牌数）
    capacity: u32,
    /// 每秒添加的令牌数
    refill_rate: f64,
    /// 每个 key 的桶状态
    buckets: RwLock<HashMap<String, TokenBucketState>>,
}

/// 令牌桶内部状态
struct TokenBucketState {
    /// 当前令牌数
    tokens: f64,
    /// 上次填充时间
    last_refill: Instant,
}

impl TokenBucketLimiter {
    /// 创建令牌桶限流器
    ///
    /// - `capacity`: 桶容量（最大令牌数 = 最大突发量）
    /// - `refill_rate`: 每秒填充的令牌数（平均速率）
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            refill_rate,
            buckets: RwLock::new(HashMap::new()),
        }
    }

    /// 填充令牌并获取当前令牌数
    fn refill_and_get(&self, key: &str) -> f64 {
        let mut buckets = self.buckets.write();
        let state = buckets.entry(key.to_string()).or_insert_with(|| TokenBucketState {
            tokens: self.capacity as f64,
            last_refill: Instant::now(),
        });

        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        let refill = elapsed * self.refill_rate;
        state.tokens = (state.tokens + refill).min(self.capacity as f64);
        state.last_refill = now;

        state.tokens
    }

    /// 消耗令牌
    fn consume(&self, key: &str, n: f64) -> bool {
        let mut buckets = self.buckets.write();
        let state = buckets.get_mut(key).unwrap();

        if state.tokens >= n {
            state.tokens -= n;
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl RateLimiter for TokenBucketLimiter {
    async fn try_acquire(&self, key: &str) -> RateLimitResult {
        self.try_acquire_n(key, 1).await
    }

    async fn try_acquire_n(&self, key: &str, n: u32) -> RateLimitResult {
        let tokens = self.refill_and_get(key);

        if tokens >= n as f64 {
            self.consume(key, n as f64);
            let remaining = (tokens - n as f64).floor() as u32;
            RateLimitResult {
                allowed: true,
                remaining,
                reset_after_secs: 0.0,
                retry_after_secs: None,
            }
        } else {
            let deficit = n as f64 - tokens;
            let wait_secs = deficit / self.refill_rate;
            RateLimitResult {
                allowed: false,
                remaining: tokens.floor() as u32,
                reset_after_secs: wait_secs,
                retry_after_secs: Some(wait_secs),
            }
        }
    }

    async fn reset(&self, key: &str) {
        let mut buckets = self.buckets.write();
        buckets.remove(key);
    }

    async fn status(&self, key: &str) -> RateLimitStatus {
        let tokens = self.refill_and_get(key);
        RateLimitStatus {
            limit: self.capacity,
            remaining: tokens.floor() as u32,
            reset_after_secs: if tokens < self.capacity as f64 {
                (self.capacity as f64 - tokens) / self.refill_rate
            } else {
                0.0
            },
        }
    }

    fn name(&self) -> &str {
        "token_bucket"
    }
}

// ─── SlidingWindowLimiter ───────────────────────────────────

/// 滑动窗口限流器
///
/// 在时间窗口内统计请求数，窗口随时间滑动。
/// 比固定窗口更平滑，避免窗口边界突发。
///
/// 适用于：需要精确控制请求频率的场景。
pub struct SlidingWindowLimiter {
    /// 窗口大小
    window_size: Duration,
    /// 窗口内最大请求数
    max_requests: u32,
    /// 每个 key 的窗口状态
    windows: RwLock<HashMap<String, SlidingWindowState>>,
}

/// 滑动窗口内部状态
struct SlidingWindowState {
    /// 请求时间戳列表
    timestamps: Vec<Instant>,
}

impl SlidingWindowLimiter {
    /// 创建滑动窗口限流器
    ///
    /// - `window_size`: 时间窗口大小
    /// - `max_requests`: 窗口内最大请求数
    pub fn new(window_size: Duration, max_requests: u32) -> Self {
        Self {
            window_size,
            max_requests,
            windows: RwLock::new(HashMap::new()),
        }
    }

    /// 清理过期的时间戳
    fn clean_expired(&self, state: &mut SlidingWindowState) {
        let cutoff = Instant::now() - self.window_size;
        state.timestamps.retain(|&t| t > cutoff);
    }
}

#[async_trait]
impl RateLimiter for SlidingWindowLimiter {
    async fn try_acquire(&self, key: &str) -> RateLimitResult {
        self.try_acquire_n(key, 1).await
    }

    async fn try_acquire_n(&self, key: &str, n: u32) -> RateLimitResult {
        let mut windows = self.windows.write();
        let state = windows
            .entry(key.to_string())
            .or_insert_with(|| SlidingWindowState { timestamps: Vec::new() });

        self.clean_expired(state);
        let current_count = state.timestamps.len() as u32;

        if current_count + n <= self.max_requests {
            let now = Instant::now();
            for _ in 0..n {
                state.timestamps.push(now);
            }
            let remaining = self.max_requests - current_count - n;
            RateLimitResult {
                allowed: true,
                remaining,
                reset_after_secs: 0.0,
                retry_after_secs: None,
            }
        } else {
            // 计算最早的过期时间
            let retry_after = if !state.timestamps.is_empty() {
                let earliest = state.timestamps[0];
                let reset_at = earliest + self.window_size;
                let now = Instant::now();
                if reset_at > now {
                    reset_at.duration_since(now).as_secs_f64()
                } else {
                    0.0
                }
            } else {
                0.0
            };

            RateLimitResult {
                allowed: false,
                remaining: 0,
                reset_after_secs: retry_after,
                retry_after_secs: Some(retry_after),
            }
        }
    }

    async fn reset(&self, key: &str) {
        let mut windows = self.windows.write();
        windows.remove(key);
    }

    async fn status(&self, key: &str) -> RateLimitStatus {
        let mut windows = self.windows.write();
        let state = windows
            .entry(key.to_string())
            .or_insert_with(|| SlidingWindowState { timestamps: Vec::new() });

        self.clean_expired(state);
        let current_count = state.timestamps.len() as u32;

        RateLimitStatus {
            limit: self.max_requests,
            remaining: self.max_requests.saturating_sub(current_count),
            reset_after_secs: self.window_size.as_secs_f64(),
        }
    }

    fn name(&self) -> &str {
        "sliding_window"
    }
}

// ─── RateLimitConfig ────────────────────────────────────────

/// 限流策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitStrategy {
    /// 按 IP 限流
    Ip,
    /// 按 API Key 限流
    Key,
    /// 全局限流
    Global,
}

/// 限流算法选择
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitAlgorithm {
    /// 令牌桶
    TokenBucket,
    /// 滑动窗口
    SlidingWindow,
}

/// 限流配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// 是否启用限流
    #[serde(default)]
    pub enabled: bool,

    /// 限流策略
    #[serde(default = "default_strategy")]
    pub strategy: RateLimitStrategy,

    /// 限流算法
    #[serde(default = "default_algorithm")]
    pub algorithm: RateLimitAlgorithm,

    /// 令牌桶：桶容量
    #[serde(default = "default_capacity")]
    pub capacity: u32,

    /// 令牌桶：每秒填充速率
    #[serde(default = "default_refill_rate")]
    pub refill_rate: f64,

    /// 滑动窗口：窗口大小（秒）
    #[serde(default = "default_window_secs")]
    pub window_secs: u64,

    /// 滑动窗口：窗口内最大请求数
    #[serde(default = "default_max_requests")]
    pub max_requests: u32,

    /// 白名单 IP 列表
    #[serde(default)]
    pub whitelist_ips: Vec<String>,

    /// 白名单 Key 列表
    #[serde(default)]
    pub whitelist_keys: Vec<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strategy: default_strategy(),
            algorithm: default_algorithm(),
            capacity: default_capacity(),
            refill_rate: default_refill_rate(),
            window_secs: default_window_secs(),
            max_requests: default_max_requests(),
            whitelist_ips: vec![],
            whitelist_keys: vec![],
        }
    }
}

fn default_strategy() -> RateLimitStrategy {
    RateLimitStrategy::Ip
}

fn default_algorithm() -> RateLimitAlgorithm {
    RateLimitAlgorithm::TokenBucket
}

fn default_capacity() -> u32 {
    100
}

fn default_refill_rate() -> f64 {
    10.0
}

fn default_window_secs() -> u64 {
    60
}

fn default_max_requests() -> u32 {
    100
}

impl RateLimitConfig {
    /// 根据配置创建限流器
    pub fn create_limiter(&self) -> Arc<dyn RateLimiter> {
        match self.algorithm {
            RateLimitAlgorithm::TokenBucket => Arc::new(TokenBucketLimiter::new(self.capacity, self.refill_rate)),
            RateLimitAlgorithm::SlidingWindow => Arc::new(SlidingWindowLimiter::new(
                Duration::from_secs(self.window_secs),
                self.max_requests,
            )),
        }
    }

    /// 判断 IP 是否在白名单中
    pub fn is_ip_whitelisted(&self, ip: &str) -> bool {
        self.whitelist_ips.iter().any(|w| w == ip)
    }

    /// 判断 Key 是否在白名单中
    pub fn is_key_whitelisted(&self, key: &str) -> bool {
        self.whitelist_keys.iter().any(|w| w == key)
    }

    /// 根据策略从请求中提取限流 key
    pub fn extract_key(&self, ip: Option<&str>, api_key: Option<&str>) -> String {
        match self.strategy {
            RateLimitStrategy::Ip => ip.unwrap_or("unknown").to_string(),
            RateLimitStrategy::Key => api_key.unwrap_or("anonymous").to_string(),
            RateLimitStrategy::Global => "global".to_string(),
        }
    }

    /// 检查是否应跳过限流
    pub fn should_skip(&self, ip: Option<&str>, api_key: Option<&str>) -> bool {
        if !self.enabled {
            return true;
        }
        if let Some(ip) = ip {
            if self.is_ip_whitelisted(ip) {
                return true;
            }
        }
        if let Some(key) = api_key {
            if self.is_key_whitelisted(key) {
                return true;
            }
        }
        false
    }
}

// ─── RateLimitMiddleware ────────────────────────────────────

/// 限流中间件 — HTTP 请求限流
///
/// 提供通用的限流逻辑，不绑定具体 HTTP 框架。
pub struct RateLimitMiddleware {
    limiter: Arc<dyn RateLimiter>,
    config: RateLimitConfig,
}

impl RateLimitMiddleware {
    pub fn new(limiter: Arc<dyn RateLimiter>, config: RateLimitConfig) -> Self {
        Self { limiter, config }
    }

    /// 从配置创建中间件
    pub fn from_config(config: RateLimitConfig) -> Self {
        let limiter = config.create_limiter();
        Self { limiter, config }
    }

    /// 检查请求是否允许通过
    pub async fn check(&self, ip: Option<&str>, api_key: Option<&str>) -> RateLimitResult {
        if self.config.should_skip(ip, api_key) {
            return RateLimitResult {
                allowed: true,
                remaining: u32::MAX,
                reset_after_secs: 0.0,
                retry_after_secs: None,
            };
        }

        let key = self.config.extract_key(ip, api_key);
        self.limiter.try_acquire(&key).await
    }

    /// 获取限流状态（用于响应头）
    pub async fn get_headers(&self, ip: Option<&str>, api_key: Option<&str>) -> HashMap<String, String> {
        let key = self.config.extract_key(ip, api_key);
        let status = self.limiter.status(&key).await;
        let mut headers = HashMap::new();
        headers.insert("X-RateLimit-Limit".to_string(), status.limit.to_string());
        headers.insert("X-RateLimit-Remaining".to_string(), status.remaining.to_string());
        headers.insert(
            "X-RateLimit-Reset".to_string(),
            format!("{:.0}", status.reset_after_secs),
        );
        headers
    }

    /// 获取限流器引用
    pub fn limiter(&self) -> Arc<dyn RateLimiter> {
        self.limiter.clone()
    }
}

// ─── CompositeRateLimiter ───────────────────────────────────

/// 组合限流器 — 同时使用多个限流算法
///
/// 所有限流器都通过才算通过（AND 逻辑）。
pub struct CompositeRateLimiter {
    limiters: Vec<Arc<dyn RateLimiter>>,
}

impl CompositeRateLimiter {
    pub fn new(limiters: Vec<Arc<dyn RateLimiter>>) -> Self {
        Self { limiters }
    }
}

#[async_trait]
impl RateLimiter for CompositeRateLimiter {
    async fn try_acquire(&self, key: &str) -> RateLimitResult {
        self.try_acquire_n(key, 1).await
    }

    async fn try_acquire_n(&self, key: &str, n: u32) -> RateLimitResult {
        let mut results = Vec::new();
        for limiter in &self.limiters {
            let result = limiter.try_acquire_n(key, n).await;
            if !result.allowed {
                // 如果任一限流器拒绝，立即返回
                return result;
            }
            results.push(result);
        }

        // 全部通过，返回最保守的结果
        let min_remaining = results.iter().map(|r| r.remaining).min().unwrap_or(0);
        RateLimitResult {
            allowed: true,
            remaining: min_remaining,
            reset_after_secs: 0.0,
            retry_after_secs: None,
        }
    }

    async fn reset(&self, key: &str) {
        for limiter in &self.limiters {
            limiter.reset(key).await;
        }
    }

    async fn status(&self, key: &str) -> RateLimitStatus {
        // 返回最保守的状态
        let statuses: Vec<RateLimitStatus> = {
            let mut v = Vec::new();
            for limiter in &self.limiters {
                v.push(limiter.status(key).await);
            }
            v
        };

        let min_remaining = statuses.iter().map(|s| s.remaining).min().unwrap_or(0);
        let max_reset = statuses.iter().map(|s| s.reset_after_secs).fold(0.0f64, f64::max);
        let min_limit = statuses.iter().map(|s| s.limit).min().unwrap_or(0);

        RateLimitStatus {
            limit: min_limit,
            remaining: min_remaining,
            reset_after_secs: max_reset,
        }
    }

    fn name(&self) -> &str {
        "composite"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_allows_within_capacity() {
        let limiter = TokenBucketLimiter::new(5, 1.0);
        let result = limiter.try_acquire("test").await;
        assert!(result.allowed);
        assert_eq!(result.remaining, 4);
    }

    #[tokio::test]
    async fn test_token_bucket_rejects_when_empty() {
        let limiter = TokenBucketLimiter::new(2, 0.1); // Very slow refill
        let _ = limiter.try_acquire("test").await;
        let _ = limiter.try_acquire("test").await;
        let result = limiter.try_acquire("test").await;
        assert!(!result.allowed);
        assert!(result.retry_after_secs.is_some());
    }

    #[tokio::test]
    async fn test_sliding_window_allows_within_limit() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(60), 5);
        for _ in 0..5 {
            let result = limiter.try_acquire("test").await;
            assert!(result.allowed);
        }
        let result = limiter.try_acquire("test").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limit_config_create_limiter() {
        let config = RateLimitConfig {
            enabled: true,
            algorithm: RateLimitAlgorithm::TokenBucket,
            capacity: 10,
            refill_rate: 1.0,
            ..Default::default()
        };
        let limiter = config.create_limiter();
        let result = limiter.try_acquire("test").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limit_config_whitelist() {
        let config = RateLimitConfig {
            enabled: true,
            whitelist_ips: vec!["127.0.0.1".to_string()],
            ..Default::default()
        };
        assert!(config.is_ip_whitelisted("127.0.0.1"));
        assert!(!config.is_ip_whitelisted("192.168.1.1"));
    }

    #[tokio::test]
    async fn test_rate_limit_middleware() {
        let config = RateLimitConfig {
            enabled: true,
            capacity: 5,
            refill_rate: 1.0,
            ..Default::default()
        };
        let middleware = RateLimitMiddleware::from_config(config);
        let result = middleware.check(Some("127.0.0.1"), None).await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limit_middleware_skip_disabled() {
        let config = RateLimitConfig {
            enabled: false,
            ..Default::default()
        };
        let middleware = RateLimitMiddleware::from_config(config);
        let result = middleware.check(Some("127.0.0.1"), None).await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_composite_rate_limiter() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(10, 1.0));
        let limiter2 = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(60), 5));

        let composite = CompositeRateLimiter::new(vec![limiter1, limiter2]);

        // First 5 requests should pass (limited by sliding window)
        for _ in 0..5 {
            let result = composite.try_acquire("test").await;
            assert!(result.allowed);
        }
        // 6th should be rejected (sliding window exhausted)
        let result = composite.try_acquire("test").await;
        assert!(!result.allowed);
    }
}
