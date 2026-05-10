//! # 监控端点处理器
//!
//! Phase 5: 基础健康检查 + Prometheus 指标
//! Phase 7: 增强健康检查（组件级 + Readiness/Liveness）

use crate::state::AppState;
use axum::response::IntoResponse;
use axum::{extract::State, http::StatusCode, response::Response};
use std::sync::Arc;

/// GET /health — 整体健康检查
///
/// Phase 7: 返回组件级健康状态
pub async fn health(State(state): State<Arc<AppState>>) -> Response {
    // 检查存储是否可用
    let store_healthy = state.store.health_check().await.is_ok();
    let uptime = state.uptime_secs();

    // Phase 7: Enhanced health check with component details
    let components = serde_json::json!({
        "database": {
            "status": if store_healthy { "healthy" } else { "unhealthy" },
        },
        "cache": {
            "status": "healthy",
        }
    });

    let body = if store_healthy {
        serde_json::json!({
            "healthy": true,
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": uptime,
            "components": components,
        })
    } else {
        serde_json::json!({
            "healthy": false,
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": uptime,
            "error": "Database unavailable",
            "components": components,
        })
    };

    let status = if store_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(serde_json::to_string(&body).unwrap_or_default().into())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// GET /ready — 就绪检查
///
/// Phase 7: 检查所有关键组件是否就绪
pub async fn ready(State(state): State<Arc<AppState>>) -> Response {
    let store_ready = state.store.health_check().await.is_ok();

    if store_ready {
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(r#"{"ready":true}"#.into())
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("content-type", "application/json")
            .body(r#"{"ready":false,"reason":"Database unavailable"}"#.into())
            .unwrap()
    }
}

/// GET /live — 存活检查
///
/// Phase 7: 简单的存活探针，检查进程是否响应
pub async fn live() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(r#"{"alive":true}"#.into())
        .unwrap()
}

/// GET /metrics — Prometheus 指标
pub async fn metrics(State(state): State<Arc<AppState>>) -> Response {
    let metrics_output = state.metrics.gather().await;

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
        .body(metrics_output.into())
        .unwrap()
}
