//! # OpenLink API Server — 主入口
//!
//! 启动流程：
//! 1. 加载配置
//! 2. 初始化 tracing
//! 3. 创建 SQLite 存储
//! 4. 构建 Extension Registry 并注册所有扩展
//! 5. 创建 Routing Engine
//! 6. 启动 Axum HTTP 服务
//!
//! Phase 2: 注册条件路由/Webhook/Hook/JSON扩展
//! Phase 3: 注册知识体系扩展
//! Phase 5: 健康检查

use openlink_api::{build_app, config::AppConfig, state::AppState};
use openlink_core::ExtensionRegistry;
use openlink_core::RoutingEngine;
use openlink_store::SqliteStore;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 1. 加载配置
    let config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("Failed to load config: {}, using defaults", e);
        AppConfig::default()
    });

    // 2. 初始化 tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "openlink=info".into()),
        )
        .init();

    tracing::info!("OpenLink starting...");
    tracing::info!(
        addr = format!("{}:{}", config.server.host, config.server.port),
        "Listening on"
    );
    tracing::info!(auth_enabled = config.auth.enabled, "Auth configuration");

    // 3. 创建 SQLite 存储
    let store = SqliteStore::new(&config.store.database_url())
        .await
        .expect("Failed to initialize SQLite store");

    // 4. 构建 Extension Registry
    let mut registry = ExtensionRegistry::new();

    // 注意：扩展注册在 Phase 6 中实现

    // Phase 3: 知识体系扩展
    ext_knowledge_join::register(&mut registry).expect("Failed to register knowledge join extension");

    tracing::info!(
        actions = ?registry.list_actions(),
        "Extension registry initialized"
    );

    // 5. 创建 Routing Engine
    let engine = RoutingEngine::new(Arc::new(registry));

    // 6. 构建 AppState
    let state = AppState::new(Arc::new(store), Arc::new(engine), Arc::new(config));

    // 日志：知识体系源
    if state.config.knowledge.enabled {
        let sources = state.config.knowledge.resolved_sources();
        if sources.is_empty() {
            tracing::info!("Knowledge system enabled but no sources configured");
        } else {
            for src in &sources {
                tracing::info!(source = %src.name, repo = %src.repo_path, codes = %src.invite_codes.len(), "Knowledge source configured");
            }
        }
    } else {
        tracing::info!("Knowledge system disabled");
    }

    // 7. 获取监听地址
    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);

    // 8. 构建 Axum App 并启动
    let app = build_app(Arc::new(state));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    tracing::info!("OpenLink server ready at {}", addr);
    tracing::info!("Phase 5 features: health checks, monitoring");
    tracing::info!("Phase 3 features: knowledge join, file transfer, agent API");
    axum::serve(listener, app).await.expect("Server error");
}
