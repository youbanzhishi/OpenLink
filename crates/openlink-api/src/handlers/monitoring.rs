//! # 监控端点处理器

use axum::{
    extract::State,
    http::StatusCode,
    response::Response,
};
use axum::response::IntoResponse;
use std::sync::Arc;
use crate::state::AppState;

/// GET /health — 健康检查
pub async fn health(State(state): State<Arc<AppState>>) -> Response {
    // 检查存储是否可用
    let store_healthy = state.store.health_check().await.is_ok();
    let uptime = state.uptime_secs();
    
    let body = if store_healthy {
        format!(r#"{{"healthy":true,"version":"{}","uptime_secs":{}}}"#, env!("CARGO_PKG_VERSION"), uptime)
    } else {
        format!(r#"{{"healthy":false,"version":"{}","uptime_secs":{},"error":"Database unavailable"}}"#, env!("CARGO_PKG_VERSION"), uptime)
    };
    
    if store_healthy {
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(body.into())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    } else {
        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("content-type", "application/json")
            .body(body.into())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

/// GET /ready — 就绪检查
pub async fn ready(State(state): State<Arc<AppState>>) -> Response {
    let store_ready = state.store.health_check().await.is_ok();
    
    if store_ready {
        Response::builder()
            .status(StatusCode::OK)
            .body("".into())
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body("".into())
            .unwrap()
    }
}

/// GET /metrics — Prometheus 指标
pub async fn metrics(State(_state): State<Arc<AppState>>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; charset=utf-8")
        .body("openlink_api_up 1\n".into())
        .unwrap()
}
