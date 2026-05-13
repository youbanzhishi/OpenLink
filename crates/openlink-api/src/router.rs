//! # HTTP 路由

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

use crate::handlers;
use crate::state::AppState;

/// 构建 Axum 应用
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // Person Agent Schema — 必须在 /:code 之前，否则通配路由会吞掉
        .route("/.well-known/agent.json", get(handlers::agent::person_agent))
        // 重定向核心路径
        .route("/:code", get(handlers::redirect::redirect))
        .route("/s/:share_code", get(handlers::redirect::share_redirect))
        // 健康检查 (Phase 5 + Phase 7)
        .route("/health", get(handlers::monitoring::health))
        .route("/ready", get(handlers::monitoring::ready))
        .route("/live", get(handlers::monitoring::live))
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
        .route(
            "/api/v1/extensions/:name",
            delete(handlers::extension::delete_extension),
        )
        // API v1 - Stats
        .route("/api/v1/stats/overview", get(handlers::stats::get_overview_stats))
        .route("/api/v1/stats/links/:id", get(handlers::stats::get_link_stats))
        // API v1 - Agent
        .route("/api/v1/agent/resolve", post(handlers::agent::batch_resolve))
        .route("/api/v1/agent/discover", post(handlers::agent::discover))
        // API v1 - Agent Config (Person Agent Schema v0.2.0)
        .route("/api/v1/agent/config", post(handlers::agent::config_service))
        // API v1 - Agent Join (Person Agent Schema v0.3.0)
        .route("/api/v1/agent/join", post(handlers::agent::join_knowledge))
        // API v1 - Plugins (Phase 8)
        .route("/api/v1/plugins", post(handlers::plugin::register_plugin))
        .route("/api/v1/plugins/search", post(handlers::plugin::search_plugins))
        .route("/api/v1/plugins/:id/install", post(handlers::plugin::install_plugin))
        // API v1 - Share (Phase 8)
        .route("/api/v1/share/project", post(handlers::plugin::share_project))
        .route("/api/v1/share/:id", get(handlers::plugin::get_shared_project))
        // API v1 - P2P (Phase 9)
        .route("/api/v1/p2p/peers", get(handlers::p2p::list_peers))
        .route("/api/v1/p2p/status", get(handlers::p2p::get_status))
        .route("/api/v1/p2p/connect", post(handlers::p2p::connect))
        // API v1 - Edge (Phase 9)
        .route("/api/v1/edge/metrics", get(handlers::edge::get_metrics))
        .route("/api/v1/edge/cache", get(handlers::edge::get_cache))
        // Card 渲染路径 (Phase 3.5: Identity Card)
        .route("/card/:code", get(handlers::card::render_card))
        .route("/card/:code/:platform", get(handlers::card::card_social_redirect))
        .route("/card/:code/qr", get(handlers::card::card_qr))
        // Card CRUD API (Phase 3.5: Identity Card)
        .route("/api/v1/cards", get(handlers::card::list_cards))
        .route("/api/v1/cards", post(handlers::card::create_card))
        .route("/api/v1/cards/:code", get(handlers::card::get_card))
        .route("/api/v1/cards/:code", put(handlers::card::update_card))
        .route("/api/v1/cards/:code", delete(handlers::card::delete_card))
        .with_state(state)
}
