//! # HTTP 路由

use axum::{
    routing::{get, post, put, delete},
    Router,
};
use std::sync::Arc;

use crate::state::AppState;
use crate::handlers;

/// 构建 Axum 应用
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // 重定向核心路径
        .route("/:code", get(handlers::redirect::redirect))
        .route("/s/:share_code", get(handlers::redirect::share_redirect))
        
        // 健康检查 (Phase 5)
        .route("/health", get(handlers::monitoring::health))
        .route("/ready", get(handlers::monitoring::ready))
        .route("/metrics", get(handlers::monitoring::metrics))
        
        // API v1 - Links
        .route("/api/v1/links", get(handlers::link::list_links))
        .route("/api/v1/links", post(handlers::link::create_link))
        .route("/api/v1/links/:id", get(handlers::link::get_link))
        .route("/api/v1/links/:id", put(handlers::link::update_link))
        .route("/api/v1/links/:id", delete(handlers::link::delete_link))
        
        // API v1 - Routes
        .route("/api/v1/routes", post(handlers::route::create_route))
        .route("/api/v1/routes/:id", put(handlers::route::update_route))
        .route("/api/v1/routes/:id", delete(handlers::route::delete_route))
        
        // API v1 - Extensions
        .route("/api/v1/extensions", get(handlers::extension::list_extensions))
        .route("/api/v1/extensions", post(handlers::extension::register_extension))
        .route("/api/v1/extensions/:name", delete(handlers::extension::delete_extension))
        
        // API v1 - Stats
        .route("/api/v1/stats/overview", get(handlers::stats::get_overview_stats))
        .route("/api/v1/stats/links/:id", get(handlers::stats::get_link_stats))
        
        // API v1 - Agent
        .route("/api/v1/agent/resolve", post(handlers::agent::batch_resolve))
        .route("/api/v1/agent/discover", post(handlers::agent::discover))
        
        .with_state(state)
}
