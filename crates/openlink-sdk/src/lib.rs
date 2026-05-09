//! # OpenLink SDK — Rust Agent SDK
//!
//! 为智能体提供简洁的 API，支持：
//! - **LinkClient**: 创建/查询/解析短链
//! - **FileClient**: 上传/下载/分享文件
//! - **自动身份注入**: agent_id/device_id
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use openlink_sdk::{LinkClient, FileClient, Config};
//!
//! let config = Config::new("https://api.openlink.dev");
//! let client = LinkClient::new(config);
//! // 创建短链
//! // let link = client.create("https://example.com").await?;
//! // println!("Created: {}", link.code);
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod models;

pub use client::{LinkClient, FileClient, ClientBuilder};
pub use config::Config;
pub use error::SdkError;
pub use models::*;
