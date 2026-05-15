//! # OpenLink API Server — HTTP 服务
//!
//! 提供完整的 REST API：
//! - Link CRUD
//! - Route 配置
//! - 重定向处理
//! - 统计查询
//! - 扩展管理
//! - 健康检查 (Phase 5)
//! - Prometheus 指标 (Phase 5)

pub mod config;
pub mod handlers;
pub mod middleware;
pub mod monitoring;
pub mod router;
pub mod state;
pub mod web_ui;

pub use config::AppConfig;
pub use monitoring::{AppMetrics, HealthCheck, HealthStatus};
pub use router::build_app;
pub use state::AppState;
