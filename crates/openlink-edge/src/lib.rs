//! # OpenLink Edge — 边缘计算版本
//!
//! Phase 5 目标：编译为最小二进制，部署在 IoT/路由器等资源受限设备。
//!
//! ## 精简策略
//! - 仅保留核心路由 + 文件传输功能
//! - 去掉 API Server（独立部署）
//! - 去掉数据库依赖（配置从文件读取）
//! - 使用轻量 HTTP 服务器 (tiny_http)
//!
//! ## 适用场景
//! - IoT 设备上的文件共享
//! - 路由器上的直连传输
//! - 边缘节点的最小化路由

pub mod router;
pub mod config;
pub mod cache;
pub mod file_transfer;

pub use router::EdgeRouter;
pub use config::EdgeConfig;
pub use cache::EdgeCache;
pub use file_transfer::FileTransferService;
