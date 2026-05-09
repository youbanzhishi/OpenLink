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

use std::sync::Arc;
use openlink_api::{build_app, state::AppState, config::AppConfig};
use openlink_store::SqliteStore;
use openlink_core::ExtensionRegistry;
use openlink_core::RoutingEngine;

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
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openlink=info".into()),
        )
        .init();

    tracing::info!("OpenLink starting (Phase 2)...");
    tracing::info!(addr = format!("{}:{}", config.server.host, config.server.port), "Listening on");
    tracing::info!(auth_enabled = config.auth.enabled, "Auth configuration");

    // 3. 创建 SQLite 存储
    let store = SqliteStore::new(&config.store.database_url())
        .await
        .expect("Failed to initialize SQLite store");

    // 4. 构建 Extension Registry 并注册所有扩展
    let mut registry = ExtensionRegistry::new();

    // Phase 1: 重定向扩展
    ext_redirect::register(&mut registry).expect("Failed to register redirect extension");

    // Phase 2: 新增扩展
    ext_webhook::register(&mut registry).expect("Failed to register webhook extension");
    ext_conditions::register(&mut registry).expect("Failed to register conditions extension");
    ext_hooks::register(&mut registry).expect("Failed to register hooks extension");
    ext_json::register(&mut registry).expect("Failed to register json extension");

    tracing::info!(
        actions = ?registry.list_actions(),
        "Extension registry initialized"
    );

    // 5. 创建 Routing Engine
    let engine = RoutingEngine::new(Arc::new(registry));

    // 6. 构建 AppState
    let state = AppState {
        store: Arc::new(store),
        engine: Arc::new(engine),
        config: Arc::new(config),
    };

    // 7. 获取监听地址（在move前）
    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);

    // 8. 构建 Axum App 并启动
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    tracing::info!("OpenLink server ready at {}", addr);
    tracing::info!("Phase 2 features: conditional routing, webhook, hooks, stats, auth");
    axum::serve(listener, app)
        .await
        .expect("Server error");
}
