//! # LinkClientBuilder — Builder Pattern for SDK Client
//!
//! Provides a fluent builder API for constructing `LinkClient` instances.
//!
//! ## Example
//!
//! ```rust,ignore
//! use openlink_sdk::LinkClientBuilder;
//! use openlink_sdk::retry::RetryPolicy;
//!
//! let client = LinkClientBuilder::new()
//!     .url("https://api.openlink.dev")
//!     .api_key("my-api-key")
//!     .timeout(60)
//!     .retry_policy(RetryPolicy::exponential_backoff(3, 100, 10_000))
//!     .edge_mode(true)
//!     .build()
//!     .expect("Failed to build LinkClient");
//! ```

use crate::client::LinkClient;
use crate::config::{Config, RetryConfig, CircuitBreakerConfig};
use crate::retry::RetryPolicy;

/// Builder for constructing `LinkClient` with a fluent API.
#[derive(Debug, Clone)]
pub struct LinkClientBuilder {
    base_url: String,
    api_key: Option<String>,
    agent_id: Option<String>,
    agent_type: Option<String>,
    device_id: Option<String>,
    timeout_secs: u64,
    retry_policy: RetryPolicy,
    circuit_breaker_enabled: bool,
    circuit_breaker_failure_threshold: u32,
    circuit_breaker_reset_timeout_secs: u64,
    edge_mode: bool,
    tls_enabled: bool,
}

impl Default for LinkClientBuilder {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            api_key: None,
            agent_id: None,
            agent_type: None,
            device_id: None,
            timeout_secs: 30,
            retry_policy: RetryPolicy::default(),
            circuit_breaker_enabled: false,
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_reset_timeout_secs: 60,
            edge_mode: false,
            tls_enabled: false,
        }
    }
}

impl LinkClientBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the base URL for the OpenLink API.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the API key for authentication.
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set the request timeout in seconds.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set the agent ID header.
    pub fn agent_id(mut self, id: impl Into<String>) -> Self {
        self.agent_id = Some(id.into());
        self
    }

    /// Set the agent type header.
    pub fn agent_type(mut self, type_: impl Into<String>) -> Self {
        self.agent_type = Some(type_.into());
        self
    }

    /// Set the device ID header.
    pub fn device_id(mut self, id: impl Into<String>) -> Self {
        self.device_id = Some(id.into());
        self
    }

    /// Set the retry policy.
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Enable circuit breaker with the given configuration.
    pub fn circuit_breaker(mut self, failure_threshold: u32, reset_timeout_secs: u64) -> Self {
        self.circuit_breaker_enabled = true;
        self.circuit_breaker_failure_threshold = failure_threshold;
        self.circuit_breaker_reset_timeout_secs = reset_timeout_secs;
        self
    }

    /// Enable edge mode (optimizations for edge deployments).
    pub fn edge_mode(mut self, enabled: bool) -> Self {
        self.edge_mode = enabled;
        self
    }

    /// Enable TLS.
    pub fn tls(mut self, enabled: bool) -> Self {
        self.tls_enabled = enabled;
        self
    }

    /// Build the `LinkClient`.
    ///
    /// Returns an error if the configuration is invalid.
    pub fn build(self) -> Result<LinkClient, String> {
        // Validate base URL
        if self.base_url.is_empty() {
            return Err("Base URL cannot be empty".to_string());
        }

        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            return Err(format!(
                "Base URL must start with http:// or https://, got: {}",
                self.base_url
            ));
        }

        let mut config = Config::new(&self.base_url);

        if let Some(key) = self.api_key {
            config.api_token = Some(key);
        }
        if let Some(id) = self.agent_id {
            config.agent_id = Some(id);
        }
        if let Some(type_) = self.agent_type {
            config.agent_type = Some(type_);
        }
        if let Some(id) = self.device_id {
            config.device_id = Some(id);
        }

        config.timeout_secs = self.timeout_secs;
        config.tls_enabled = self.tls_enabled;

        // Apply retry policy
        config.retry = RetryConfig {
            max_retries: self.retry_policy.max_retries(),
            initial_backoff_ms: self.retry_policy.initial_backoff_ms(),
            max_backoff_ms: self.retry_policy.max_backoff_ms(),
            backoff_multiplier: self.retry_policy.backoff_multiplier(),
        };

        // Apply circuit breaker config
        config.circuit_breaker = CircuitBreakerConfig {
            enabled: self.circuit_breaker_enabled,
            failure_threshold: self.circuit_breaker_failure_threshold,
            reset_timeout_secs: self.circuit_breaker_reset_timeout_secs,
        };

        // In edge mode, reduce timeout and disable circuit breaker by default
        if self.edge_mode {
            config.timeout_secs = config.timeout_secs.min(10);
        }

        Ok(LinkClient::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retry::RetryPolicy;

    #[test]
    fn test_builder_default() {
        let builder = LinkClientBuilder::new();
        assert_eq!(builder.base_url, "http://localhost:8080");
        assert!(builder.api_key.is_none());
        assert_eq!(builder.timeout_secs, 30);
        assert!(!builder.edge_mode);
    }

    #[test]
    fn test_builder_chained_calls() {
        let client = LinkClientBuilder::new()
            .url("https://api.example.com")
            .api_key("test-key")
            .timeout(60)
            .agent_id("agent-1")
            .agent_type("assistant")
            .device_id("device-001")
            .edge_mode(true)
            .build()
            .expect("build should succeed");

        assert_eq!(client.config().base_url, "https://api.example.com");
        assert_eq!(client.config().api_token.as_deref(), Some("test-key"));
        // Edge mode caps timeout at 10
        assert_eq!(client.config().timeout_secs, 10);
    }

    #[test]
    fn test_builder_empty_url_fails() {
        let result = LinkClientBuilder::new()
            .url("")
            .build();
        assert!(result.is_err());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.contains("empty"));
    }

    #[test]
    fn test_builder_invalid_url_scheme_fails() {
        let result = LinkClientBuilder::new()
            .url("ftp://example.com")
            .build();
        assert!(result.is_err());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.contains("http://"));
    }

    #[test]
    fn test_builder_with_circuit_breaker() {
        let client = LinkClientBuilder::new()
            .url("https://api.example.com")
            .circuit_breaker(3, 30)
            .build()
            .expect("build should succeed");

        assert!(client.config().circuit_breaker.enabled);
        assert_eq!(client.config().circuit_breaker.failure_threshold, 3);
        assert_eq!(client.config().circuit_breaker.reset_timeout_secs, 30);
    }

    #[test]
    fn test_builder_with_retry_policy() {
        let policy = RetryPolicy::exponential_backoff(5, 200, 60_000);
        let client = LinkClientBuilder::new()
            .url("https://api.example.com")
            .retry_policy(policy)
            .build()
            .expect("build should succeed");

        assert_eq!(client.config().retry.max_retries, 5);
        assert_eq!(client.config().retry.initial_backoff_ms, 200);
        assert_eq!(client.config().retry.max_backoff_ms, 60_000);
    }

    #[test]
    fn test_builder_edge_mode_caps_timeout() {
        let client = LinkClientBuilder::new()
            .url("https://api.example.com")
            .timeout(120)
            .edge_mode(true)
            .build()
            .expect("build should succeed");

        assert_eq!(client.config().timeout_secs, 10);
    }

    #[test]
    fn test_builder_https_url_valid() {
        let result = LinkClientBuilder::new()
            .url("https://api.openlink.dev")
            .build();
        assert!(result.is_ok());
    }
}
