//! # OpenLink API — HTTP 接口层
//!
//! 基于 Axum 构建的 HTTP API，提供短链管理、重定向、路由规则管理等功能。
//!
//! 核心路径：GET /:code → 302 重定向（零配置开箱即用）

pub mod router;
pub mod state;
pub mod config;
pub mod handlers;
pub mod middleware;

pub use router::build_app;
