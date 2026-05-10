//! # OpenLink SDK — Rust Agent SDK
//!
//! 为智能体提供简洁的 API，支持：
//! - **LinkClient**: 创建/查询/解析短链（含重试+熔断）
//! - **FileClient**: 上传/下载/分享文件
//! - **BatchClient**: 批量操作（批量创建/解析/删除+并发控制）
//! - **EventClient**: 事件订阅（访问/Webhook/文件变化）
//! - **LinkClientBuilder**: Builder模式创建客户端
//! - **中间件链**: 认证/日志/指标
//! - **智能重试**: 指数退避/固定间隔/自定义策略
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use openlink_sdk::LinkClientBuilder;
//! use openlink_sdk::retry::RetryPolicy;
//!
//! let client = LinkClientBuilder::new()
//!     .url("https://api.openlink.dev")
//!     .api_key("my-token")
//!     .timeout(60)
//!     .retry_policy(RetryPolicy::exponential_backoff(3, 100, 10_000))
//!     .build()
//!     .expect("Failed to build client");
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod models;
pub mod batch;
pub mod event;
pub mod builder;
pub mod retry;
pub mod middleware;

pub use client::{LinkClient, FileClient, ClientBuilder};
pub use config::{Config, RetryConfig, CircuitBreakerConfig, CircuitBreaker, CircuitState};
pub use error::SdkError;
pub use models::*;
pub use batch::BatchClient;
pub use event::{EventClient, EventFilter, EventType, Event, SubscribeResponse};
pub use builder::LinkClientBuilder;
pub use retry::{RetryPolicy, RetryCondition};
pub use middleware::{
    Middleware, MiddlewareChain, AuthMiddleware, LoggingMiddleware,
    MetricsMiddleware, RequestMetrics, RequestContext, ResponseContext,
};
