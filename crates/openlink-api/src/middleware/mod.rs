//! # 中间件
//!
//! Hook 中间件实现，包括请求日志、认证等。

pub mod auth;
pub mod logging;

pub use auth::require_auth;
