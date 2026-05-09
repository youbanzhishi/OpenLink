//! # 中间件
//!
//! Hook 中间件实现，包括请求日志、认证等。

pub mod logging;
pub mod auth;

pub use auth::require_auth;
