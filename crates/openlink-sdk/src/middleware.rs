//! # Middleware — Request/Response Middleware Chain
//!
//! Provides a composable middleware system for intercepting HTTP requests and responses.
//!
//! ## Built-in Middleware
//!
//! - **AuthMiddleware**: Automatically injects authentication tokens
//! - **LoggingMiddleware**: Logs request/response details
//! - **MetricsMiddleware**: Tracks request duration and status codes
//!
//! ## Example
//!
//! ```rust,ignore
//! use openlink_sdk::middleware::{MiddlewareChain, AuthMiddleware, LoggingMiddleware};
//!
//! let mut chain = MiddlewareChain::new();
//! chain.add(AuthMiddleware::new("my-api-key".to_string()));
//! chain.add(LoggingMiddleware::new());
//!
//! // Apply before_request to add headers, etc.
//! // Apply after_response to log, collect metrics, etc.
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

// ─── Middleware Trait ──────────────────────────────────────────────────────

/// Context passed through the middleware chain for before_request.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Request URL.
    pub url: String,
    /// Request method (GET, POST, etc.).
    pub method: String,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Request body (if any).
    pub body: Option<String>,
}

/// Context passed through the middleware chain for after_response.
#[derive(Debug, Clone)]
pub struct ResponseContext {
    /// Response status code.
    pub status: u16,
    /// Response headers.
    pub headers: HashMap<String, String>,
    /// Response body (if captured).
    pub body: Option<String>,
    /// Request duration.
    pub duration_ms: u64,
}

/// Trait for implementing request/response middleware.
pub trait Middleware: Send + Sync {
    /// Called before the request is sent. Can modify the request context.
    fn before_request(&self, ctx: &mut RequestContext);

    /// Called after the response is received. Can inspect the response context.
    fn after_response(&self, ctx: &ResponseContext);

    /// Middleware name for identification.
    fn name(&self) -> &str;
}

// ─── Auth Middleware ───────────────────────────────────────────────────────

/// Middleware that automatically injects authentication tokens into requests.
pub struct AuthMiddleware {
    api_key: String,
    agent_id: Option<String>,
    device_id: Option<String>,
}

impl AuthMiddleware {
    /// Create a new auth middleware with an API key.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            agent_id: None,
            device_id: None,
        }
    }

    /// Set the agent ID header.
    pub fn with_agent_id(mut self, agent_id: String) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    /// Set the device ID header.
    pub fn with_device_id(mut self, device_id: String) -> Self {
        self.device_id = Some(device_id);
        self
    }
}

impl Middleware for AuthMiddleware {
    fn before_request(&self, ctx: &mut RequestContext) {
        ctx.headers
            .insert("Authorization".to_string(), format!("Bearer {}", self.api_key));
        if let Some(ref agent_id) = self.agent_id {
            ctx.headers.insert("X-Agent-ID".to_string(), agent_id.clone());
        }
        if let Some(ref device_id) = self.device_id {
            ctx.headers.insert("X-Device-ID".to_string(), device_id.clone());
        }
    }

    fn after_response(&self, _ctx: &ResponseContext) {
        // No post-processing needed for auth
    }

    fn name(&self) -> &str {
        "auth"
    }
}

// ─── Logging Middleware ────────────────────────────────────────────────────

/// Middleware that logs request and response details.
pub struct LoggingMiddleware {
    log_bodies: bool,
}

impl LoggingMiddleware {
    /// Create a new logging middleware (without body logging).
    pub fn new() -> Self {
        Self { log_bodies: false }
    }

    /// Enable body logging.
    pub fn with_bodies(mut self) -> Self {
        self.log_bodies = true;
        self
    }
}

impl Default for LoggingMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for LoggingMiddleware {
    fn before_request(&self, ctx: &mut RequestContext) {
        let body_info = if self.log_bodies {
            ctx.body
                .as_ref()
                .map(|b| format!(" body={}bytes", b.len()))
                .unwrap_or_default()
        } else {
            String::new()
        };
        tracing::info!("[SDK] -> {} {} {}", ctx.method, ctx.url, body_info);
    }

    fn after_response(&self, ctx: &ResponseContext) {
        let body_info = if self.log_bodies {
            ctx.body
                .as_ref()
                .map(|b| format!(" body={}bytes", b.len()))
                .unwrap_or_default()
        } else {
            String::new()
        };
        tracing::info!("[SDK] <- {} ({}ms){}", ctx.status, ctx.duration_ms, body_info);
    }

    fn name(&self) -> &str {
        "logging"
    }
}

// ─── Metrics Middleware ────────────────────────────────────────────────────

/// Metrics collected by the MetricsMiddleware.
#[derive(Debug, Clone, Default)]
pub struct RequestMetrics {
    /// Total number of requests.
    pub total_requests: u64,
    /// Number of requests by status code.
    pub by_status: HashMap<u16, u64>,
    /// Total request duration in milliseconds.
    pub total_duration_ms: u64,
    /// Number of requests by method.
    pub by_method: HashMap<String, u64>,
}

impl RequestMetrics {
    /// Create new empty metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the average request duration in milliseconds.
    pub fn avg_duration_ms(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_duration_ms as f64 / self.total_requests as f64
        }
    }

    /// Get the error rate (5xx responses / total).
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            let errors: u64 = self
                .by_status
                .iter()
                .filter(|(code, _)| **code >= 500)
                .map(|(_, count)| *count)
                .sum();
            errors as f64 / self.total_requests as f64
        }
    }
}

/// Middleware that collects request/response metrics.
pub struct MetricsMiddleware {
    metrics: Arc<Mutex<RequestMetrics>>,
}

impl MetricsMiddleware {
    /// Create a new metrics middleware.
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(RequestMetrics::new())),
        }
    }

    /// Get a snapshot of the current metrics.
    pub fn metrics(&self) -> RequestMetrics {
        self.metrics.lock().clone()
    }

    /// Reset all metrics.
    pub fn reset(&self) {
        let mut m = self.metrics.lock();
        *m = RequestMetrics::new();
    }
}

impl Default for MetricsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for MetricsMiddleware {
    fn before_request(&self, ctx: &mut RequestContext) {
        let mut m = self.metrics.lock();
        *m.by_method.entry(ctx.method.clone()).or_insert(0) += 1;
    }

    fn after_response(&self, ctx: &ResponseContext) {
        let mut m = self.metrics.lock();
        m.total_requests += 1;
        *m.by_status.entry(ctx.status).or_insert(0) += 1;
        m.total_duration_ms += ctx.duration_ms;
    }

    fn name(&self) -> &str {
        "metrics"
    }
}

// ─── Middleware Chain ──────────────────────────────────────────────────────

/// A chain of middleware that executes in order for before_request
/// and after_response.
pub struct MiddlewareChain {
    middlewares: Vec<Arc<dyn Middleware>>,
}

impl MiddlewareChain {
    /// Create a new empty middleware chain.
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// Add a middleware to the chain.
    pub fn add<M: Middleware + 'static>(&mut self, middleware: M) {
        self.middlewares.push(Arc::new(middleware));
    }

    /// Execute all before_request handlers in order.
    pub fn before_request(&self, ctx: &mut RequestContext) {
        for mw in &self.middlewares {
            mw.before_request(ctx);
        }
    }

    /// Execute all after_response handlers in order.
    pub fn after_response(&self, ctx: &ResponseContext) {
        for mw in &self.middlewares {
            mw.after_response(ctx);
        }
    }

    /// Get the number of middlewares in the chain.
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    /// Find a middleware by name.
    pub fn find_by_name(&self, name: &str) -> Option<&Arc<dyn Middleware>> {
        self.middlewares.iter().find(|mw| mw.name() == name)
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Auth Middleware Tests ─────────────────────────────────────────────

    #[test]
    fn test_auth_middleware_injects_token() {
        let mw = AuthMiddleware::new("my-api-key".to_string());
        let mut ctx = RequestContext {
            url: "https://api.example.com/v1/links".to_string(),
            method: "POST".to_string(),
            headers: HashMap::new(),
            body: None,
        };
        mw.before_request(&mut ctx);
        assert_eq!(ctx.headers.get("Authorization").unwrap(), "Bearer my-api-key");
    }

    #[test]
    fn test_auth_middleware_with_agent_and_device() {
        let mw = AuthMiddleware::new("key".to_string())
            .with_agent_id("agent-1".to_string())
            .with_device_id("device-1".to_string());
        let mut ctx = RequestContext {
            url: "https://api.example.com".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };
        mw.before_request(&mut ctx);
        assert_eq!(ctx.headers.get("X-Agent-ID").unwrap(), "agent-1");
        assert_eq!(ctx.headers.get("X-Device-ID").unwrap(), "device-1");
    }

    #[test]
    fn test_auth_middleware_name() {
        let mw = AuthMiddleware::new("key".to_string());
        assert_eq!(mw.name(), "auth");
    }

    // ─── Logging Middleware Tests ──────────────────────────────────────────

    #[test]
    fn test_logging_middleware_before_request() {
        let mw = LoggingMiddleware::new();
        let mut ctx = RequestContext {
            url: "https://api.example.com".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };
        // Should not panic
        mw.before_request(&mut ctx);
    }

    #[test]
    fn test_logging_middleware_after_response() {
        let mw = LoggingMiddleware::new();
        let ctx = ResponseContext {
            status: 200,
            headers: HashMap::new(),
            body: None,
            duration_ms: 42,
        };
        // Should not panic
        mw.after_response(&ctx);
    }

    #[test]
    fn test_logging_middleware_with_bodies() {
        let mw = LoggingMiddleware::new().with_bodies();
        let mut ctx = RequestContext {
            url: "https://api.example.com".to_string(),
            method: "POST".to_string(),
            headers: HashMap::new(),
            body: Some("{\"test\": true}".to_string()),
        };
        // Should not panic
        mw.before_request(&mut ctx);
    }

    // ─── Metrics Middleware Tests ──────────────────────────────────────────

    #[test]
    fn test_metrics_middleware_collects_data() {
        let mw = MetricsMiddleware::new();

        // Simulate before_request
        let mut req_ctx = RequestContext {
            url: "https://api.example.com".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };
        mw.before_request(&mut req_ctx);

        // Simulate after_response
        let resp_ctx = ResponseContext {
            status: 200,
            headers: HashMap::new(),
            body: None,
            duration_ms: 50,
        };
        mw.after_response(&resp_ctx);

        let metrics = mw.metrics();
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(*metrics.by_status.get(&200).unwrap(), 1);
        assert_eq!(metrics.total_duration_ms, 50);
        assert_eq!(*metrics.by_method.get("GET").unwrap(), 1);
    }

    #[test]
    fn test_metrics_avg_duration() {
        let mw = MetricsMiddleware::new();

        // Simulate 2 requests
        let resp1 = ResponseContext {
            status: 200,
            headers: HashMap::new(),
            body: None,
            duration_ms: 100,
        };
        let resp2 = ResponseContext {
            status: 200,
            headers: HashMap::new(),
            body: None,
            duration_ms: 200,
        };
        mw.after_response(&resp1);
        mw.after_response(&resp2);

        let metrics = mw.metrics();
        assert_eq!(metrics.avg_duration_ms(), 150.0);
    }

    #[test]
    fn test_metrics_error_rate() {
        let mw = MetricsMiddleware::new();

        let resp_ok = ResponseContext {
            status: 200,
            headers: HashMap::new(),
            body: None,
            duration_ms: 10,
        };
        let resp_err = ResponseContext {
            status: 500,
            headers: HashMap::new(),
            body: None,
            duration_ms: 100,
        };

        mw.after_response(&resp_ok);
        mw.after_response(&resp_ok);
        mw.after_response(&resp_err);

        let metrics = mw.metrics();
        assert!((metrics.error_rate() - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_metrics_reset() {
        let mw = MetricsMiddleware::new();
        let resp = ResponseContext {
            status: 200,
            headers: HashMap::new(),
            body: None,
            duration_ms: 10,
        };
        mw.after_response(&resp);
        assert_eq!(mw.metrics().total_requests, 1);

        mw.reset();
        assert_eq!(mw.metrics().total_requests, 0);
    }

    // ─── Middleware Chain Tests ────────────────────────────────────────────

    #[test]
    fn test_middleware_chain_execution_order() {
        let mut chain = MiddlewareChain::new();
        chain.add(AuthMiddleware::new("key".to_string()));
        chain.add(LoggingMiddleware::new());

        assert_eq!(chain.len(), 2);

        let mut ctx = RequestContext {
            url: "https://api.example.com".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };
        chain.before_request(&mut ctx);

        // Auth middleware should have injected the header
        assert!(ctx.headers.contains_key("Authorization"));
    }

    #[test]
    fn test_middleware_chain_find_by_name() {
        let mut chain = MiddlewareChain::new();
        chain.add(AuthMiddleware::new("key".to_string()));
        chain.add(LoggingMiddleware::new());
        chain.add(MetricsMiddleware::new());

        assert!(chain.find_by_name("auth").is_some());
        assert!(chain.find_by_name("logging").is_some());
        assert!(chain.find_by_name("metrics").is_some());
        assert!(chain.find_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_middleware_chain_empty() {
        let chain = MiddlewareChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);

        let mut ctx = RequestContext {
            url: "https://api.example.com".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };
        chain.before_request(&mut ctx);
        assert!(ctx.headers.is_empty());
    }

    #[test]
    fn test_request_metrics_empty() {
        let metrics = RequestMetrics::new();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.avg_duration_ms(), 0.0);
        assert_eq!(metrics.error_rate(), 0.0);
    }
}
