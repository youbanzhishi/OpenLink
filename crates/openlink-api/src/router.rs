//! # 路由定义
//!
//! Axum 路由配置：
//! - /v1/links — 短链 CRUD
//! - /v1/links/:code/routes — 路由规则管理
//! - /v1/links/:code/stats — 访问统计
//! - /v1/extensions — 扩展管理
//! - /:code — 核心重定向路径（最快）

use std::sync::Arc;
use axum::{
    Router,
    routing::{get, post, put, delete},
    middleware,
};
use crate::state::AppState;
use crate::handlers;
use crate::middleware::logging::request_logging;

/// 构建 Axum App
///
/// 路由结构：
/// - GET /:code → 重定向（核心路径，最频繁）
/// - POST/GET /v1/links → 创建/列出短链
/// - GET/PUT/DELETE /v1/links/:code → 查询/更新/删除短链
/// - GET /v1/links/:code/stats → 访问统计
/// - POST /v1/links/:code/routes → 创建路由规则
/// - PUT/DELETE /v1/links/:code/routes/:route_id → 更新/删除路由规则
/// - POST/GET /v1/extensions → 注册/列出扩展
/// - DELETE /v1/extensions/:name → 卸载扩展
pub fn build_app(state: AppState) -> Router {
    Router::new()
        // ─── 核心路径：/:code 重定向（最高频，必须最快）──────────────
        .route("/:code", get(handlers::redirect::redirect))

        // ─── 短链 CRUD ─────────────────────────────────────────────
        .route("/v1/links", post(handlers::link::create_link).get(handlers::link::list_links))
        .route("/v1/links/:code", get(handlers::link::get_link).put(handlers::link::update_link).delete(handlers::link::delete_link))
        .route("/v1/links/:code/stats", get(handlers::link::get_stats))

        // ─── 路由规则管理 ──────────────────────────────────────────
        .route("/v1/links/:code/routes", post(handlers::route::create_route))
        .route("/v1/links/:code/routes/:route_id", put(handlers::route::update_route).delete(handlers::route::delete_route))

        // ─── 扩展管理 ──────────────────────────────────────────────
        .route("/v1/extensions", post(handlers::extension::register_extension).get(handlers::extension::list_extensions))
        .route("/v1/extensions/:name", delete(handlers::extension::delete_extension))

        // ─── 中间件 ────────────────────────────────────────────────
        .layer(middleware::from_fn(request_logging))
        .with_state(Arc::new(state))
}
