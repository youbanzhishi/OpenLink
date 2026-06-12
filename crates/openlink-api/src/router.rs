//! # HTTP 路由
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
//! - /.well-known/agent.json — Agent 发现（Phase 3 新增）
//! - /v1/knowledge/* — 知识体系 API（Phase 3 新增）
//!
//! Phase 2: 管理API需要Token认证，重定向API不需要认证
//! Phase 3: Agent API使用 X-Agent-ID Header认证

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

use crate::handlers;
use crate::state::AppState;
use crate::web_ui;

/// Web UI handler: Dashboard
async fn ui_dashboard() -> axum::response::Html<String> {
    web_ui::dashboard_page()
}

/// Web UI handler: Links page
async fn ui_links() -> axum::response::Html<String> {
    web_ui::links_page()
}

/// Web UI handler: Links table fragment (HTMX)
async fn ui_links_table() -> axum::response::Html<String> {
    web_ui::links_table_html()
}

/// Web UI handler: Routes page
async fn ui_routes() -> axum::response::Html<String> {
    web_ui::routes_page()
}

/// Web UI handler: Extensions page
async fn ui_extensions() -> axum::response::Html<String> {
    web_ui::extensions_page()
}

/// Web UI handler: Extensions list fragment (HTMX)
async fn ui_extensions_list() -> axum::response::Html<String> {
    web_ui::extensions_list_html()
}

/// Web UI handler: Agent page
async fn ui_agent() -> axum::response::Html<String> {
    web_ui::agent_page()
}

/// 构建 Axum 应用
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // ===== Web UI Pages (must be before /:code catch-all) =====
        .route("/", get(ui_dashboard))
        .route("/ui/links", get(ui_links))
        .route("/ui/links-table", get(ui_links_table))
        .route("/ui/routes", get(ui_routes))
        .route("/ui/extensions", get(ui_extensions))
        .route("/ui/extensions-list", get(ui_extensions_list))
        .route("/ui/agent", get(ui_agent))
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
        // API v1 - Resolve (单条解析短链)
        .route("/api/v1/resolve/:code", get(handlers::link::resolve_link))
        // API v1 - Batch (批量查询短链)
        .route("/api/v1/links/batch", get(handlers::link::batch_links))
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
        // API v1 - Agent Join (→ 转发到知识体系一键接入)
        .route("/api/v1/agent/join", post(handlers::knowledge::join_knowledge))
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
        // API v1 - Extension Search (三桥模式, Phase 10)
        .route(
            "/api/v1/extensions/search",
            post(handlers::extension_search::search_extensions),
        )
        .route(
            "/api/v1/extensions/execute",
            post(handlers::extension_search::execute_extension),
        )
        .route(
            "/api/v1/extensions/:name/schema",
            get(handlers::extension_search::get_extension_schema),
        )
        // API v1 - KnowledgeSync (ADR-009, Phase 10)
        .route("/api/v1/knowledge/auth", post(handlers::knowledge_sync::knowledge_auth))
        .route(
            "/api/v1/knowledge/query",
            post(handlers::knowledge_sync::knowledge_query),
        )
        .route(
            "/api/v1/knowledge/read/:id",
            get(handlers::knowledge_sync::knowledge_read),
        )
        .route(
            "/api/v1/knowledge/write",
            post(handlers::knowledge_sync::knowledge_write),
        )
        .route(
            "/api/v1/knowledge/callback",
            post(handlers::knowledge_sync::knowledge_callback),
        )
        // API v1 - Knowledge 知识体系一键接入
        // 一条短链入口 — GET /join?code=xxx
        .route("/join", get(handlers::knowledge::knowledge_short_entry))
        .route("/api/v1/knowledge/join", post(handlers::knowledge::join_knowledge))
        .route("/api/v1/knowledge/entry", get(handlers::knowledge::get_entry))
        .route("/api/v1/knowledge/role/:name", get(handlers::knowledge::get_role_rules))
        .route(
            "/api/v1/knowledge/project/:name",
            get(handlers::knowledge::get_project_index),
        )
        .route("/api/v1/knowledge/script/:name", get(handlers::knowledge::get_script))
        .route(
            "/api/v1/knowledge/hot-rules/:role",
            get(handlers::knowledge::get_role_hot_rules),
        )
        .route(
            "/api/v1/knowledge/markdown",
            get(handlers::knowledge::get_knowledge_markdown),
        )
        // Knowledge Sync — 推送后自动同步知识仓库
        .route("/api/v1/knowledge/sync", post(handlers::knowledge::sync_knowledge))
        .with_state(state)
}
