//! 权限管理 API Handler
//!
//! 实现 WO-061 要求的权限校验Hook与Person Agent Schema集成
//! API端点：POST /api/v1/agent/authorize, DELETE /api/v1/agent/session/{id}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use openlink_auth::{
    AgentPermission, AgentType, Operation, PermissionChecker, PermissionError, 
    ResourceLimits, SessionConfig, PermissionStatus,
};
use openlink_core::error::CoreError;

/// 应用状态
pub struct AppState {
    pub permission_checker: Arc<PermissionChecker>,
}

/// ========== 权限相关 ==========

/// 创建权限请求
#[derive(Debug, Deserialize)]
pub struct CreatePermissionRequest {
    pub agent_id: String,
    pub agent_name: String,
    pub agent_type: AgentType,
    pub allowed_extensions: Vec<String>,
    pub allowed_operations: Vec<Operation>,
    pub resource_limits: Option<ResourceLimits>,
    pub session_config: Option<SessionConfig>,
    pub valid_days: Option<i64>,
}

/// 创建权限响应
#[derive(Debug, Serialize)]
pub struct CreatePermissionResponse {
    pub permission_id: String,
    pub status: String,
    pub message: String,
}

/// 权限详情响应
#[derive(Debug, Serialize)]
pub struct PermissionDetailResponse {
    pub permission_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub agent_type: String,
    pub allowed_extensions: Vec<String>,
    pub allowed_operations: Vec<String>,
    pub status: String,
    pub valid_from: String,
    pub valid_until: String,
}

/// 创建权限
pub async fn create_permission(
    State(state): State<AppState>,
    Json(req): Json<CreatePermissionRequest>,
) -> Result<Json<CreatePermissionResponse>, StatusCode> {
    // TODO: 实际实现需要调用 permission_store.create()
    // 这里返回模拟响应
    Ok(Json(CreatePermissionResponse {
        permission_id: format!("perm-{}", uuid::Uuid::new_v4()),
        status: "active".into(),
        message: "Permission created successfully".into(),
    }))
}

/// 获取权限列表
pub async fn list_permissions(
    State(_state): State<AppState>,
) -> Result<Json<Vec<PermissionDetailResponse>>, StatusCode> {
    // TODO: 实际实现需要调用 permission_store.list()
    Ok(Json(vec![]))
}

/// 获取单个权限详情
pub async fn get_permission(
    State(_state): State<AppState>,
    Path(permission_id): Path<String>,
) -> Result<Json<PermissionDetailResponse>, StatusCode> {
    // TODO: 实际实现需要调用 permission_store.get()
    Ok(Json(PermissionDetailResponse {
        permission_id,
        agent_id: "agent-001".into(),
        agent_name: "Test Agent".into(),
        agent_type: "guest".into(),
        allowed_extensions: vec![],
        allowed_operations: vec!["read".into()],
        status: "active".into(),
        valid_from: "2026-06-16T00:00:00Z".into(),
        valid_until: "2026-06-23T00:00:00Z".into(),
    }))
}

/// 更新权限
pub async fn update_permission(
    State(_state): State<AppState>,
    Path(permission_id): Path<String>,
    Json(req): Json<CreatePermissionRequest>,
) -> Result<Json<CreatePermissionResponse>, StatusCode> {
    Ok(Json(CreatePermissionResponse {
        permission_id,
        status: "updated".into(),
        message: "Permission updated successfully".into(),
    }))
}

/// 撤销权限
pub async fn revoke_permission(
    State(_state): State<AppState>,
    Path(permission_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

/// ========== 会话相关 ==========

/// 创建会话请求
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub permission_id: String,
    pub metadata: Option<SessionMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct SessionMetadata {
    pub source_ip: Option<String>,
    pub user_agent: Option<String>,
}

/// 创建会话响应
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: String,
}

/// 创建会话
pub async fn create_session(
    State(_state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, StatusCode> {
    // TODO: 实际实现需要调用 session_store.create() 和 token_generator
    Ok(Json(CreateSessionResponse {
        session_id: format!("sess-{}", uuid::Uuid::new_v4()),
        access_token: "access_token_placeholder".into(),
        refresh_token: "refresh_token_placeholder".into(),
        expires_in: 3600,
        token_type: "Bearer".into(),
    }))
}

/// 刷新会话请求
#[derive(Debug, Deserialize)]
pub struct RefreshSessionRequest {
    pub refresh_token: String,
}

/// 刷新会话响应
#[derive(Debug, Serialize)]
pub struct RefreshSessionResponse {
    pub access_token: String,
    pub expires_in: u64,
}

/// 刷新会话
pub async fn refresh_session(
    State(_state): State<AppState>,
    Json(req): Json<RefreshSessionRequest>,
) -> Result<Json<RefreshSessionResponse>, StatusCode> {
    // TODO: 实际实现需要验证 refresh_token 并生成新的 access_token
    Ok(Json(RefreshSessionResponse {
        access_token: "new_access_token_placeholder".into(),
        expires_in: 3600,
    }))
}

/// 终止会话
pub async fn revoke_session(
    State(_state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // TODO: 实际实现需要调用 session_store.revoke()
    Ok(StatusCode::NO_CONTENT)
}

/// ========== 路由器 ==========

/// 创建权限相关路由
pub fn create_permission_routes(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/permissions", post(create_permission))
        .route("/api/v1/permissions", get(list_permissions))
        .route("/api/v1/permissions/:id", get(get_permission))
        .route("/api/v1/permissions/:id", put(update_permission))
        .route("/api/v1/permissions/:id", delete(revoke_permission))
        .with_state(state)
}

/// 创建会话相关路由
pub fn create_session_routes(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/sessions", post(create_session))
        .route("/api/v1/sessions/refresh", post(refresh_session))
        .route("/api/v1/sessions/:id", delete(revoke_session))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_permission_request() {
        let req = CreatePermissionRequest {
            agent_id: "agent-001".into(),
            agent_name: "Test Agent".into(),
            agent_type: AgentType::Guest,
            allowed_extensions: vec!["knowledge-search".into()],
            allowed_operations: vec![Operation::Read],
            resource_limits: None,
            session_config: None,
            valid_days: Some(30),
        };
        assert_eq!(req.agent_type, AgentType::Guest);
    }

    #[test]
    fn test_create_session_response() {
        let resp = CreateSessionResponse {
            session_id: "sess-001".into(),
            access_token: "token".into(),
            refresh_token: "refresh".into(),
            expires_in: 3600,
            token_type: "Bearer".into(),
        };
        assert_eq!(resp.token_type, "Bearer");
    }
}
