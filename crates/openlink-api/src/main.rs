//! # OpenLink API Server — 主入口
//!
//! Phase 5: 健康检查

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

    tracing::info!("OpenLink starting (Phase 5)...");
    tracing::info!(addr = format!("{}:{}", config.server.host, config.server.port), "Listening on");
    tracing::info!(auth_enabled = config.auth.enabled, "Auth configuration");

    // 3. 创建 SQLite 存储
    let store = SqliteStore::new(&config.store.database_url())
        .await
        .expect("Failed to initialize SQLite store");

    // 4. 构建 Extension Registry
    let registry = ExtensionRegistry::new();
    
    // 注意：扩展注册在 Phase 6 中实现

    tracing::info!(
        actions = ?registry.list_actions(),
        "Extension registry initialized"
    );

    // 5. 创建 Routing Engine
    let engine = RoutingEngine::new(Arc::new(registry));

    // 6. 构建 AppState
    let state = AppState::new(
        Arc::new(store),
        Arc::new(engine),
        Arc::new(config),
    );

    // 7. 获取监听地址
    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);

    // 8. 构建 Axum App 并启动
    let app = build_app(Arc::new(state));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    tracing::info!("OpenLink server ready at {}", addr);
    tracing::info!("Phase 5 features: health checks, monitoring");
    axum::serve(listener, app)
        .await
        .expect("Server error");
}
