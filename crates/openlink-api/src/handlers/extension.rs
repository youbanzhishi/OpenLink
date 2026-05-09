//! # 扩展管理处理器
//!
//! POST/GET/DELETE /v1/extensions

use axum::{
    extract::{State, Path},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use openlink_core::Extension;
use openlink_store::Store;
use crate::state::AppState;

/// 注册扩展请求
#[derive(Debug, Deserialize)]
pub struct RegisterExtensionRequest {
    /// 扩展类型：action / condition / hook / protocol
    pub ext_type: String,
    /// 扩展名称（唯一）
    pub name: String,
    /// 扩展配置
    #[serde(default)]
    pub config: serde_json::Value,
}

/// 扩展响应
#[derive(Debug, Serialize)]
pub struct ExtensionResponse {
    pub id: String,
    pub ext_type: String,
    pub name: String,
    pub config: serde_json::Value,
    pub is_active: bool,
    pub created_at: String,
}

impl From<Extension> for ExtensionResponse {
    fn from(ext: Extension) -> Self {
        Self {
            id: ext.id,
            ext_type: ext.ext_type,
            name: ext.name,
            config: ext.config,
            is_active: ext.is_active,
            created_at: ext.created_at.to_rfc3339(),
        }
    }
}

/// 注册扩展
///
/// POST /v1/extensions
pub async fn register_extension(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterExtensionRequest>,
) -> Result<(StatusCode, Json<ExtensionResponse>), (StatusCode, String)> {
    // 检查是否已存在
    if state
        .store
        .get_extension_by_name(&req.name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_some()
    {
        return Err((StatusCode::CONFLICT, format!("Extension '{}' already exists", req.name)));
    }

    let ext = Extension {
        id: uuid::Uuid::new_v4().to_string(),
        ext_type: req.ext_type,
        name: req.name,
        config: req.config,
        is_active: true,
        created_at: chrono::Utc::now(),
    };

    let created = state
        .store
        .register_extension(&ext)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(name = %created.name, ext_type = %created.ext_type, "Extension registered");
    Ok((StatusCode::CREATED, Json(ExtensionResponse::from(created))))
}

/// 列出所有扩展
///
/// GET /v1/extensions
pub async fn list_extensions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ExtensionResponse>>, (StatusCode, String)> {
    let exts = state
        .store
        .list_extensions()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let responses: Vec<ExtensionResponse> = exts.into_iter().map(ExtensionResponse::from).collect();
    Ok(Json(responses))
}

/// 卸载扩展
///
/// DELETE /v1/extensions/:name
pub async fn delete_extension(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .store
        .delete_extension(&name)
        .await
        .map_err(|e| {
            match e {
                openlink_store::StoreError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            }
        })?;

    tracing::info!(name = %name, "Extension deleted");
    Ok(StatusCode::NO_CONTENT)
}
