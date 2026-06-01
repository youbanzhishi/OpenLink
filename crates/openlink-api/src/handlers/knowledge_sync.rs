//! # KnowledgeSync Handler — Agent 间知识同步 HTTP API
//!
//! - POST /api/v1/knowledge/auth — 认证
//! - POST /api/v1/knowledge/query — 查询知识
//! - GET /api/v1/knowledge/read/:id — 读取知识文档
//! - POST /api/v1/knowledge/write — 写入知识
//! - POST /api/v1/knowledge/callback — 注册回调

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use openlink_core::knowledge_sync::{
    KnowledgeAuthRequest, KnowledgeAuthResponse, KnowledgeCallbackRequest, KnowledgeCallbackResponse,
    KnowledgeQueryRequest, KnowledgeQueryResponse, KnowledgeReadResponse, KnowledgeScope, KnowledgeWriteRequest,
    KnowledgeWriteResponse,
};
use std::sync::Arc;

use crate::state::AppState;

/// 认证
///
/// POST /api/v1/knowledge/auth
///
/// KnowledgeSync Phase 2: 支持两种认证模式：
/// - api_key: 简化模式，适用于自托管/内网场景
/// - authorization_code: OAuth 2.1+PKCE（后续实现）
pub async fn knowledge_auth(
    State(state): State<Arc<AppState>>,
    Json(req): Json<KnowledgeAuthRequest>,
) -> Result<Json<KnowledgeAuthResponse>, (StatusCode, String)> {
    let ks = state.knowledge_sync.read().await;

    match ks.authenticate(&req) {
        Ok(response) => {
            tracing::info!(
                client_id = %req.client_id,
                grant_type = ?req.grant_type,
                "Knowledge auth succeeded"
            );
            Ok(Json(response))
        }
        Err(e) => {
            tracing::warn!(
                client_id = %req.client_id,
                error = %e,
                "Knowledge auth failed"
            );
            Err((StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))
        }
    }
}

/// 查询知识
///
/// POST /api/v1/knowledge/query
///
/// KnowledgeSync Phase 3 (read): 语义搜索知识库，返回匹配文档的摘要和相关性评分。
/// 对标 ima 的"选取资料"，但用语义搜索替代手动勾选。
///
/// Headers:
/// - Authorization: Bearer <access_token>
pub async fn knowledge_query(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<KnowledgeQueryRequest>,
) -> Result<Json<KnowledgeQueryResponse>, (StatusCode, String)> {
    // 验证认证
    validate_bearer_token(&headers, &state, &KnowledgeScope::KnowledgeRead)?;

    let ks = state.knowledge_sync.read().await;

    match ks.query(&req).await {
        Ok(response) => {
            tracing::info!(
                query = %req.query,
                total = response.total,
                "Knowledge query completed"
            );
            Ok(Json(response))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Query failed: {}", e))),
    }
}

/// 读取知识文档
///
/// GET /api/v1/knowledge/read/:id
///
/// KnowledgeSync Phase 3 (read): 获取完整知识文档。
pub async fn knowledge_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeReadResponse>, (StatusCode, String)> {
    validate_bearer_token(&headers, &state, &KnowledgeScope::KnowledgeRead)?;

    let ks = state.knowledge_sync.read().await;

    match ks.read(&id).await {
        Ok(Some(response)) => {
            tracing::info!(doc_id = %id, "Knowledge document read");
            Ok(Json(response))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, format!("Document '{}' not found", id))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Read failed: {}", e))),
    }
}

/// 写入知识
///
/// POST /api/v1/knowledge/write
///
/// KnowledgeSync Phase 3 (write): 写入知识文档。
/// 对标 ima 的"产物回传"，但支持任意知识写入，不限于任务产物。
pub async fn knowledge_write(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<KnowledgeWriteRequest>,
) -> Result<Json<KnowledgeWriteResponse>, (StatusCode, String)> {
    validate_bearer_token(&headers, &state, &KnowledgeScope::KnowledgeWrite)?;

    let ks = state.knowledge_sync.read().await;

    match ks.write(&req).await {
        Ok(response) => {
            tracing::info!(
                collection = %req.collection,
                title = %req.title,
                doc_id = %response.id,
                status = ?response.status,
                "Knowledge written"
            );
            Ok(Json(response))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Write failed: {}", e))),
    }
}

/// 注册回调
///
/// POST /api/v1/knowledge/callback
///
/// KnowledgeSync Phase 4: 注册知识变更回调。
/// 对标 ima 的闭环，但更通用——支持任意知识变更通知。
pub async fn knowledge_callback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<KnowledgeCallbackRequest>,
) -> Result<Json<KnowledgeCallbackResponse>, (StatusCode, String)> {
    validate_bearer_token(&headers, &state, &KnowledgeScope::KnowledgeRead)?;

    let ks = state.knowledge_sync.read().await;
    let response = ks.register_callback(req);

    tracing::info!(
        sub_id = %response.subscription_id,
        events = ?response.events,
        "Knowledge callback registered"
    );

    Ok(Json(response))
}

// ─── Helper ────────────────────────────────────────────────────

/// 验证 Bearer Token（POC 简化版）
///
/// 生产环境需要完整的 Token 验证（JWT 解析 + scope 检查 + 过期验证）。
/// POC 阶段只检查 token 是否存在且以 "ks_" 开头。
fn validate_bearer_token(
    headers: &HeaderMap,
    _state: &AppState,
    _required_scope: &KnowledgeScope,
) -> Result<(), (StatusCode, String)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Missing Authorization header".to_string()))?;

    if !auth_header.starts_with("Bearer ") {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid Authorization header format".to_string(),
        ));
    }

    let token = &auth_header[7..];
    if !token.starts_with("ks_") {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token format".to_string()));
    }

    // POC: 通过基本验证
    Ok(())
}
