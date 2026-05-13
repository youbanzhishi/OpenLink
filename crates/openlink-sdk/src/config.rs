//! # SDK 配置

use serde::{Deserialize, Serialize};

use std::sync::Arc;
use std::time::{Duration, Instant};

/// SDK 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// OpenLink API 基础 URL
    pub base_url: String,
    /// API Token（用于认证）
    pub api_token: Option<String>,
    /// Agent ID（自动注入）
    pub agent_id: Option<String>,
    /// Agent 类型
    pub agent_type: Option<String>,
    /// Device ID（自动注入）
    pub device_id: Option<String>,
    /// 请求超时（秒）
    pub timeout_secs: u64,
    /// 是否启用 TLS
    pub tls_enabled: bool,
    /// 重试配置
    #[serde(default)]
    pub retry: RetryConfig,
    /// 熔断器配置
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            api_token: None,
            agent_id: None,
            agent_type: None,
            device_id: None,
            timeout_secs: 30,
            tls_enabled: false,
            retry: RetryConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

impl Config {
    /// 创建新配置
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    /// 设置 API Token
    pub fn api_token(mut self, token: impl Into<String>) -> Self {
        self.api_token = Some(token.into());
        self
    }

    /// 设置 Agent ID
    pub fn agent_id(mut self, id: impl Into<String>) -> Self {
        self.agent_id = Some(id.into());
        self
    }

    /// 设置 Agent 类型
    pub fn agent_type(mut self, type_: impl Into<String>) -> Self {
        self.agent_type = Some(type_.into());
        self
    }

    /// 设置 Device ID
    pub fn device_id(mut self, id: impl Into<String>) -> Self {
        self.device_id = Some(id.into());
        self
    }

    /// 设置超时
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 设置重试配置
    pub fn retry(mut self, max_retries: u32) -> Self {
        self.retry.max_retries = max_retries;
        self
    }

    /// 设置熔断器配置
    pub fn circuit_breaker(mut self, failure_threshold: u32, reset_timeout_secs: u64) -> Self {
        self.circuit_breaker.failure_threshold = failure_threshold;
        self.circuit_breaker.reset_timeout_secs = reset_timeout_secs;
        self
    }

    /// 获取完整 API URL
    pub fn api_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }
}

/// 重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始退避时间（毫秒）
    pub initial_backoff_ms: u64,
    /// 最大退避时间（毫秒）
    pub max_backoff_ms: u64,
    /// 退避倍数
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: 100,
            max_backoff_ms: 30_000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// 计算第 N 次重试的等待时间
    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        let backoff_ms = (self.initial_backoff_ms as f64 * self.backoff_multiplier.powi(attempt as i32)) as u64;
        let clamped = backoff_ms.min(self.max_backoff_ms);
        Duration::from_millis(clamped)
    }
}

/// 熔断器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 失败次数阈值（达到后熔断）
    pub failure_threshold: u32,
    /// 熔断后重置等待时间（秒）
    pub reset_timeout_secs: u64,
    /// 是否启用
    pub enabled: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout_secs: 60,
            enabled: false,
        }
    }
}

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    /// 关闭（正常）
    Closed,
    /// 打开（熔断中）
    Open,
    /// 半开（试探恢复）
    HalfOpen,
}

/// 熔断器实现
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<std::sync::Mutex<CircuitBreakerInner>>,
}

#[derive(Debug)]
struct CircuitBreakerInner {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
}

impl CircuitBreaker {
    /// 创建新的熔断器
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(std::sync::Mutex::new(CircuitBreakerInner {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
            })),
        }
    }

    /// 检查是否允许请求通过
    pub fn allow_request(&self) -> bool {
        if !self.config.enabled {
            return true;
        }

        let mut inner = self.state.lock().unwrap();
        match inner.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if reset timeout has elapsed
                if let Some(last_failure) = inner.last_failure_time {
                    if last_failure.elapsed() >= Duration::from_secs(self.config.reset_timeout_secs) {
                        inner.state = CircuitState::HalfOpen;
                        inner.success_count = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// 记录成功
    pub fn record_success(&self) {
        let mut inner = self.state.lock().unwrap();
        match inner.state {
            CircuitState::HalfOpen => {
                inner.success_count += 1;
                // After a few successes in half-open, close the circuit
                if inner.success_count >= 2 {
                    inner.state = CircuitState::Closed;
                    inner.failure_count = 0;
                }
            }
            CircuitState::Closed => {
                inner.failure_count = 0;
            }
            _ => {}
        }
    }

    /// 记录失败
    pub fn record_failure(&self) {
        let mut inner = self.state.lock().unwrap();
        inner.failure_count += 1;
        inner.last_failure_time = Some(Instant::now());

        match inner.state {
            CircuitState::Closed => {
                if inner.failure_count >= self.config.failure_threshold {
                    inner.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                inner.state = CircuitState::Open;
            }
            _ => {}
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> CircuitState {
        self.state.lock().unwrap().state
    }

    /// 重置熔断器
    pub fn reset(&self) {
        let mut inner = self.state.lock().unwrap();
        inner.state = CircuitState::Closed;
        inner.failure_count = 0;
        inner.success_count = 0;
        inner.last_failure_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.base_url, "http://localhost:8080");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.retry.max_retries, 0);
        assert!(!config.circuit_breaker.enabled);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::new("https://api.example.com")
            .api_token("test-token")
            .retry(3)
            .circuit_breaker(5, 60);

        assert_eq!(config.base_url, "https://api.example.com");
        assert_eq!(config.retry.max_retries, 3);
        assert_eq!(config.circuit_breaker.failure_threshold, 5);
    }

    #[test]
    fn test_retry_backoff() {
        let config = RetryConfig::default();
        let d0 = config.backoff_duration(0);
        let d1 = config.backoff_duration(1);
        let d2 = config.backoff_duration(2);

        assert!(d1 > d0);
        assert!(d2 > d1);
        assert!(d2 <= Duration::from_millis(config.max_backoff_ms));
    }

    #[test]
    fn test_retry_backoff_clamped() {
        let config = RetryConfig {
            max_backoff_ms: 500,
            ..Default::default()
        };
        // With multiplier 2.0 and initial 100ms:
        // attempt 0 = 100ms, 1 = 200ms, 2 = 400ms, 3 = 800ms → clamped to 500ms
        let d3 = config.backoff_duration(3);
        assert_eq!(d3, Duration::from_millis(500));
    }

    #[test]
    fn test_circuit_breaker_closed() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout_secs: 60,
            enabled: true,
        };
        let cb = CircuitBreaker::new(config);
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_trips() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            reset_timeout_secs: 60,
            enabled: true,
        };
        let cb = CircuitBreaker::new(config);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        assert!(!cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_disabled() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_secs: 60,
            enabled: false,
        };
        let cb = CircuitBreaker::new(config);

        // Even after failures, should allow requests when disabled
        cb.record_failure();
        cb.record_failure();
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_secs: 60,
            enabled: true,
        };
        let cb = CircuitBreaker::new(config);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_half_open_to_closed() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_secs: 0, // immediate reset for testing
            enabled: true,
        };
        let cb = CircuitBreaker::new(config);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // With 0 reset_timeout, should transition to half-open
        assert!(cb.allow_request());
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Record enough successes to close
        cb.record_success();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
