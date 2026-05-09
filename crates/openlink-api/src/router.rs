//! # 路由定义
//!
//! Axum 路由配置：
//! - /:code — 核心重定向路径（无需认证）
//! - /v1/links — 短链 CRUD（需认证）
//! - /v1/links/:code/routes — 路由规则管理（需认证）
//! - /v1/links/:code/stats — 访问统计（需认证）
//! - /v1/stats/overview — 全局概览（需认证）
//! - /v1/extensions — 扩展管理（需认证）
//! - /v1/agent/* — Agent 专用 API（Phase 3 新增）
//! - /v1/files/* — 文件传输 API（Phase 3 新增）
//!
//! Phase 2: 管理API需要Token认证，重定向API不需要认证
//! Phase 3: Agent API使用 X-Agent-ID Header认证

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
/// - GET /:code → 重定向（核心路径，最频繁，无需认证）
/// - POST/GET /v1/links → 创建/列出短链（需认证）
/// - GET/PUT/DELETE /v1/links/:code → 查询/更新/删除短链（需认证）
/// - GET /v1/links/:code/stats → 访问统计（需认证）
/// - POST /v1/links/:code/routes → 创建路由规则（需认证）
/// - PUT/DELETE /v1/links/:code/routes/:route_id → 更新/删除路由规则（需认证）
/// - GET /v1/stats/overview → 全局概览（需认证）
/// - POST/GET /v1/extensions → 注册/列出扩展（需认证）
/// - DELETE /v1/extensions/:name → 卸载扩展（需认证）
/// - POST /v1/agent/resolve → 批量解析（Agent API）
/// - POST /v1/agent/discover → 发现 Link（Agent API）
/// - POST /v1/agent/upload → 文件上传（Agent API）
/// - POST /v1/agent/download → 文件下载（Agent API）
/// - POST /v1/agent/share → 文件分享（Agent API）
pub fn build_app(state: AppState) -> Router {
    // 公开路由（无需认证）
    let public_routes = Router::new()
        // 核心路径：/:code 重定向（最高频，必须最快，无需认证）
        .route("/:code", get(handlers::redirect::redirect))
        // 分享码访问
        .route("/s/:share_code", get(handlers::redirect::share_redirect));

    // 管理路由（需认证 — Phase 2 实现 Bearer Token 中间件后启用）
    let admin_routes = Router::new()
        // 短链 CRUD
        .route("/v1/links", post(handlers::link::create_link).get(handlers::link::list_links))
        .route("/v1/links/:code", get(handlers::link::get_link).put(handlers::link::update_link).delete(handlers::link::delete_link))
        .route("/v1/links/:code/stats", get(handlers::stats::get_link_stats))

        // 路由规则管理
        .route("/v1/links/:code/routes", post(handlers::route::create_route))
        .route("/v1/links/:code/routes/:route_id", put(handlers::route::update_route).delete(handlers::route::delete_route))

        // 全局统计
        .route("/v1/stats/overview", get(handlers::stats::get_overview_stats))

        // 扩展管理
        .route("/v1/extensions", post(handlers::extension::register_extension).get(handlers::extension::list_extensions))
        .route("/v1/extensions/:name", delete(handlers::extension::delete_extension));

    // Agent API 路由（Phase 3 — 使用 X-Agent-ID Header 认证）
    let agent_routes = Router::new()
        .route("/v1/agent/resolve", post(handlers::agent::batch_resolve))
        .route("/v1/agent/discover", post(handlers::agent::discover))
        .route("/v1/agent/upload", post(handlers::agent::init_upload))
        .route("/v1/agent/download", post(handlers::agent::request_download))
        .route("/v1/agent/share", post(handlers::agent::share_file));

    // 文件 API 路由（Phase 3）
    let file_routes = Router::new()
        .route("/v1/files/upload", post(handlers::agent::init_upload))
        .route("/v1/files/:file_id/download", get(handlers::agent::request_download))
        .route("/v1/files/share", post(handlers::agent::share_file));

    Router::new()
        .merge(public_routes)
        .merge(admin_routes)
        .merge(agent_routes)
        .merge(file_routes)
        // 全局中间件：请求日志
        .layer(middleware::from_fn(request_logging))
        .with_state(Arc::new(state))
}
