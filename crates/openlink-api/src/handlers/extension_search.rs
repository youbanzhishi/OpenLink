use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use openlink_core::extension_search::{
    ExtensionSchema, ExtensionSearchRequest, ExtensionSearchResponse, ExtensionType,
};
use openlink_core::primitives::{Action, Context, Target};
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;

pub async fn search_extensions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ExtensionSearchRequest>,
) -> Result<Json<ExtensionSearchResponse>, (StatusCode, String)> {
    let registry = state.search_registry.read().await;
    let response = registry.search(&req.query, req.ext_type.clone(), req.limit).await;
    Ok(Json(response))
}

pub async fn get_extension_schema(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ExtensionSchema>, (StatusCode, String)> {
    let registry = state.search_registry.read().await;
    match registry.describe(&name) {
        Some(schema) => Ok(Json(schema)),
        None => Err((StatusCode::NOT_FOUND, format!("Extension '{}' not found", name))),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecuteResponse {
    pub ok: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// POST /api/v1/extensions/execute
///
/// Bridge 3: 执行 Extension。
/// 将 handler 提取出 registry guard 后再调用 async_trait 方法，
/// 避免 guard 跨 await 持有导致的 Send 约束问题。
pub async fn execute_extension(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ExecuteResponse>, (StatusCode, String)> {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let ext_type_str = body.get("ext_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let arguments = body.get("arguments").cloned().unwrap_or(serde_json::json!({}));

    let ext_type = ExtensionType::try_from_str(&ext_type_str).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let ctx = Context::from_request(None, None);

    match ext_type {
        ExtensionType::Action => {
            // 先提取 handler（Arc 克隆），释放 registry guard
            let handler: Arc<dyn openlink_core::ActionHandler> = {
                let registry = state.search_registry.read().await;
                let inner = registry.inner();
                inner
                    .get_action_handler(&name)
                    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Action '{}' not found", name)))?
            };
            let target = Target {
                action: Action::Custom(name),
                params: arguments,
            };
            let result = handler
                .execute(&ctx, &target)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", e)))?;
            Ok(Json(ExecuteResponse {
                ok: true,
                result: Some(serde_json::to_value(result).unwrap_or_default()),
                error: None,
            }))
        }
        ExtensionType::Condition => {
            let handler: Arc<dyn openlink_core::ConditionHandler> = {
                let registry = state.search_registry.read().await;
                let inner = registry.inner();
                inner
                    .get_condition_handler(&name)
                    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Condition '{}' not found", name)))?
            };
            let matched = handler
                .evaluate(&ctx, &arguments)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", e)))?;
            Ok(Json(ExecuteResponse {
                ok: true,
                result: Some(serde_json::json!({ "matched": matched })),
                error: None,
            }))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("Cannot execute {:?} directly", ext_type),
        )),
    }
}
