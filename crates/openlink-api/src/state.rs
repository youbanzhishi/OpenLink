//! # 应用状态 — Axum 共享状态
//!
//! AppState 包含所有请求处理所需的共享资源。

use std::sync::Arc;
use openlink_core::RoutingEngine;
use openlink_store::Store;
use crate::config::AppConfig;

/// 应用共享状态
///
/// 通过 Axum 的 State extractor 注入到各个 handler 中。
pub struct AppState {
    /// 存储层（通过 trait 抽象，可替换实现）
    pub store: Arc<dyn Store>,
    /// 路由引擎
    pub engine: Arc<RoutingEngine>,
    /// 应用配置
    pub config: Arc<AppConfig>,
    /// 知识体系仓库路径（Phase 3）
    pub knowledge_repo_path: Option<String>,
}
