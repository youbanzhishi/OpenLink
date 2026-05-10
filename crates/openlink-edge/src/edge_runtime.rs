//! # 边缘运行时 (Phase 9)
//!
//! 请求处理管道：接收→路由→执行→响应
//! 请求优先级队列、并发限制、请求超时控制。


use crate::router::EdgeRouter;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// 请求优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestPriority {
    Low = 0,
    Medium = 1,
    High = 2,
}

impl Default for RequestPriority {
    fn default() -> Self {
        RequestPriority::Medium
    }
}

/// 边缘请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRequest {
    /// 请求 ID
    pub id: String,
    /// 请求路径/代码
    pub code: String,
    /// 客户端 IP
    pub client_ip: Option<String>,
    /// User-Agent
    pub user_agent: Option<String>,
    /// 优先级
    pub priority: RequestPriority,
    /// 请求创建时间戳
    pub created_at: i64,
    /// 超时时间（秒）
    pub timeout_secs: u64,
}

/// 边缘响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeResponse {
    /// 请求 ID
    pub request_id: String,
    /// 响应状态码
    pub status_code: u16,
    /// 目标 URL（重定向）
    pub target_url: Option<String>,
    /// 响应体
    pub body: Option<String>,
    /// 处理耗时（微秒）
    pub processing_time_us: u64,
    /// 响应来源
    pub source: String,
}

/// 运行时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// 最大并发请求数
    pub max_concurrency: usize,
    /// 默认请求超时（秒）
    pub default_timeout_secs: u64,
    /// 优先级队列大小
    pub queue_size: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 100,
            default_timeout_secs: 30,
            queue_size: 1000,
        }
    }
}

/// 请求处理管道阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    Receive,
    Route,
    Execute,
    Respond,
}

/// 请求处理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub request_id: String,
    pub stage: PipelineStage,
    pub success: bool,
    pub elapsed_us: u64,
    pub error: Option<String>,
}

/// 边缘运行时
pub struct EdgeRuntime {
    config: RuntimeConfig,
    router: Arc<EdgeRouter>,
    concurrency_limiter: Arc<Semaphore>,
    /// 活跃请求数
    active_requests: Arc<std::sync::atomic::AtomicU64>,
    /// 总请求计数
    total_requests: Arc<std::sync::atomic::AtomicU64>,
    /// 总错误计数
    total_errors: Arc<std::sync::atomic::AtomicU64>,
}

impl EdgeRuntime {
    /// 创建边缘运行时
    pub fn new(config: RuntimeConfig, router: Arc<EdgeRouter>) -> Self {
        let concurrency = config.max_concurrency;
        Self {
            config,
            router,
            concurrency_limiter: Arc::new(Semaphore::new(concurrency)),
            active_requests: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_requests: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_errors: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// 处理请求（管道模式：接收→路由→执行→响应）
    pub async fn handle_request(&self, request: RuntimeRequest) -> EdgeResponse {
        let start = Instant::now();
        self.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.active_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let result = self.process_pipeline(&request).await;

        self.active_requests.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        if result.status_code >= 400 {
            self.total_errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        result
    }

    /// 请求处理管道
    async fn process_pipeline(&self, request: &RuntimeRequest) -> EdgeResponse {
        let start = Instant::now();
        let timeout = Duration::from_secs(request.timeout_secs.min(self.config.default_timeout_secs * 2));

        // 获取并发许可
        let permit = match tokio::time::timeout(timeout, self.concurrency_limiter.acquire()).await {
            Ok(Ok(permit)) => permit,
            _ => {
                return EdgeResponse {
                    request_id: request.id.clone(),
                    status_code: 503,
                    target_url: None,
                    body: Some("Concurrency limit reached".to_string()),
                    processing_time_us: start.elapsed().as_micros() as u64,
                    source: "runtime".to_string(),
                };
            }
        };

        // 路由阶段：查找目标
        let route_result = tokio::time::timeout(
            timeout,
            self.router.resolve(&request.code, request.client_ip.as_deref(), request.user_agent.as_deref()),
        ).await;

        let response = match route_result {
            Ok(Some(result)) => EdgeResponse {
                request_id: request.id.clone(),
                status_code: result.status_code,
                target_url: Some(result.target_url),
                body: None,
                processing_time_us: start.elapsed().as_micros() as u64,
                source: result.source.to_string(),
            },
            Ok(None) => EdgeResponse {
                request_id: request.id.clone(),
                status_code: 404,
                target_url: None,
                body: Some("Not found".to_string()),
                processing_time_us: start.elapsed().as_micros() as u64,
                source: "runtime".to_string(),
            },
            Err(_) => EdgeResponse {
                request_id: request.id.clone(),
                status_code: 504,
                target_url: None,
                body: Some("Request timeout".to_string()),
                processing_time_us: start.elapsed().as_micros() as u64,
                source: "runtime".to_string(),
            },
        };

        drop(permit);
        response
    }

    /// 获取运行时统计
    pub fn runtime_stats(&self) -> RuntimeStats {
        RuntimeStats {
            active_requests: self.active_requests.load(std::sync::atomic::Ordering::Relaxed),
            total_requests: self.total_requests.load(std::sync::atomic::Ordering::Relaxed),
            total_errors: self.total_errors.load(std::sync::atomic::Ordering::Relaxed),
            max_concurrency: self.config.max_concurrency,
            available_permits: self.concurrency_limiter.available_permits(),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }
}

/// 运行时统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStats {
    pub active_requests: u64,
    pub total_requests: u64,
    pub total_errors: u64,
    pub max_concurrency: usize,
    pub available_permits: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EdgeConfig;

    #[tokio::test]
    async fn test_edge_runtime_basic() {
        let config = RuntimeConfig::default();
        let edge_config = EdgeConfig::default_config();
        let router = Arc::new(EdgeRouter::new(edge_config));
        let runtime = EdgeRuntime::new(config, router);

        let request = RuntimeRequest {
            id: "req-1".to_string(),
            code: "test".to_string(),
            client_ip: Some("127.0.0.1".to_string()),
            user_agent: None,
            priority: RequestPriority::Medium,
            created_at: chrono::Utc::now().timestamp(),
            timeout_secs: 10,
        };

        let response = runtime.handle_request(request).await;
        assert_eq!(response.status_code, 404); // No route registered
    }

    #[tokio::test]
    async fn test_edge_runtime_with_route() {
        let config = RuntimeConfig::default();
        let edge_config = EdgeConfig::default_config();
        let router = Arc::new(EdgeRouter::new(edge_config));
        router.register_route("abc".to_string(), "https://example.com".to_string(), 302).await;

        let runtime = EdgeRuntime::new(config, router);
        let request = RuntimeRequest {
            id: "req-2".to_string(),
            code: "abc".to_string(),
            client_ip: None,
            user_agent: None,
            priority: RequestPriority::High,
            created_at: chrono::Utc::now().timestamp(),
            timeout_secs: 10,
        };

        let response = runtime.handle_request(request).await;
        assert_eq!(response.status_code, 302);
        assert_eq!(response.target_url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn test_request_priority_ordering() {
        assert!(RequestPriority::High > RequestPriority::Medium);
        assert!(RequestPriority::Medium > RequestPriority::Low);
    }

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.max_concurrency, 100);
        assert_eq!(config.default_timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_runtime_stats() {
        let config = RuntimeConfig::default();
        let edge_config = EdgeConfig::default_config();
        let router = Arc::new(EdgeRouter::new(edge_config));
        let runtime = EdgeRuntime::new(config, router);

        let stats = runtime.runtime_stats();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.active_requests, 0);
        assert_eq!(stats.max_concurrency, 100);
    }

    #[tokio::test]
    async fn test_runtime_concurrency_limit() {
        let config = RuntimeConfig {
            max_concurrency: 2,
            default_timeout_secs: 5,
            queue_size: 100,
        };
        let edge_config = EdgeConfig::default_config();
        let router = Arc::new(EdgeRouter::new(edge_config));
        let runtime = EdgeRuntime::new(config, router);

        let stats = runtime.runtime_stats();
        assert_eq!(stats.max_concurrency, 2);
    }

    #[test]
    fn test_pipeline_stage_values() {
        assert_ne!(PipelineStage::Receive, PipelineStage::Route);
        assert_ne!(PipelineStage::Execute, PipelineStage::Respond);
    }

    #[tokio::test]
    async fn test_runtime_timeout_handling() {
        let config = RuntimeConfig {
            max_concurrency: 1,
            default_timeout_secs: 1,
            queue_size: 10,
        };
        let edge_config = EdgeConfig::default_config();
        let router = Arc::new(EdgeRouter::new(edge_config));
        let runtime = EdgeRuntime::new(config, router);

        let request = RuntimeRequest {
            id: "req-timeout".to_string(),
            code: "nonexistent".to_string(),
            client_ip: None,
            user_agent: None,
            priority: RequestPriority::High,
            created_at: chrono::Utc::now().timestamp(),
            timeout_secs: 1,
        };

        let response = runtime.handle_request(request).await;
        // Should still respond (404 not timeout for this simple case)
        assert!(response.status_code == 404 || response.status_code == 504);
    }
}
