//! # 统计处理器
//!
//! GET /v1/links/:code/stats — 链接访问统计
//! GET /v1/stats/overview — 全局统计概览
//!
//! Phase 2: 增强版统计，包含设备分布、身份分布

use axum::{
    extract::{State, Path},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::state::AppState;

/// 获取链接访问统计
///
/// GET /v1/links/:code/stats
pub async fn get_link_stats(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let link = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", code)))?;

    let stats = state
        .store
        .get_link_stats(&link.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::to_value(stats).unwrap_or_default()))
}

/// 获取全局统计概览
///
/// GET /v1/stats/overview
pub async fn get_overview_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let stats = state
        .store
        .get_overview_stats()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::to_value(stats).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    // Handler tests are covered by integration tests
}
