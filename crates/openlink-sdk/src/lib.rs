//! # OpenLink SDK — Rust Agent SDK
//!
//! 为智能体提供简洁的 API，支持：
//! - **LinkClient**: 创建/查询/解析短链（含重试+熔断）
//! - **FileClient**: 上传/下载/分享文件
//! - **BatchClient**: 批量操作（批量创建/解析/删除）
//! - **EventClient**: 事件订阅（访问/Webhook/文件变化）
//! - **自动身份注入**: agent_id/device_id
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use openlink_sdk::{ClientBuilder, LinkClient, FileClient, BatchClient, EventClient};
//!
//! let (link, file) = ClientBuilder::new()
//!     .base_url("https://api.openlink.dev")
//!     .api_token("my-token")
//!     .retry(3)
//!     .circuit_breaker(5, 60)
//!     .build();
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod models;
pub mod batch;
pub mod event;

pub use client::{LinkClient, FileClient, ClientBuilder};
pub use config::{Config, RetryConfig, CircuitBreakerConfig, CircuitBreaker, CircuitState};
pub use error::SdkError;
pub use models::*;
pub use batch::BatchClient;
pub use event::{EventClient, EventFilter, EventType, Event, SubscribeResponse};
