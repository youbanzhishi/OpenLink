//! # 短链 CRUD 处理器
//!
//! POST/GET/PUT/DELETE /v1/links
//!
//! Phase 2: list_links 使用 Store 实现分页

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use openlink_core::{shortcode, Link};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

/// 创建短链请求
#[derive(Debug, Deserialize)]
pub struct CreateLinkRequest {
    /// 自定义短码（不填则自动生成）
    pub code: Option<String>,
    /// 链接元数据
    #[serde(default)]
    pub payload: serde_json::Value,
    /// 重定向目标 URL（便捷字段，自动写入 payload）
    pub target: Option<String>,
    /// 扩展元数据
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// 短链响应
#[derive(Debug, Serialize)]
pub struct LinkResponse {
    pub id: String,
    pub code: String,
    pub payload: serde_json::Value,
    pub owner: String,
    pub created_at: String,
    pub is_active: bool,
}

impl From<Link> for LinkResponse {
    fn from(link: Link) -> Self {
        Self {
            id: link.id,
            code: link.code,
            payload: link.payload,
            owner: link.owner,
            created_at: link.created_at.to_rfc3339(),
            is_active: link.is_active,
        }
    }
}

/// 更新短链请求
#[derive(Debug, Deserialize)]
pub struct UpdateLinkRequest {
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// 列出短链查询参数
#[derive(Debug, Deserialize)]
pub struct ListLinksQuery {
    #[serde(default = "default_offset")]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_offset() -> i64 {
    0
}

fn default_limit() -> i64 {
    20
}

/// 创建短链
///
/// POST /v1/links
pub async fn create_link(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLinkRequest>,
) -> Result<(StatusCode, Json<LinkResponse>), (StatusCode, String)> {
    // 生成短码
    let code = match req.code {
        Some(ref c) => {
            if !shortcode::is_valid(c) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Invalid short code: must be base62".to_string(),
                ));
            }
            c.clone()
        }
        None => {
            // 生成唯一短码（简单重试）
            let mut code = shortcode::generate(state.config.shortcode.length);
            let mut attempts = 0;
            while state
                .store
                .get_link_by_code(&code)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                .is_some()
            {
                if attempts > 10 {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to generate unique code".to_string(),
                    ));
                }
                code = shortcode::generate(state.config.shortcode.length);
                attempts += 1;
            }
            code
        }
    };

    // 构建链接 payload
    let mut payload = req.payload;
    if let Some(url) = req.target {
        payload["target_url"] = serde_json::Value::String(url);
    }

    let link = Link {
        id: uuid::Uuid::new_v4().to_string(),
        code: code.clone(),
        payload,
        owner: "default".to_string(), // Phase 2: 后续从 Token 中获取 owner
        created_at: chrono::Utc::now(),
        metadata: req.metadata,
        is_active: true,
    };

    let created = state.store.create_link(&link).await.map_err(|e| match e {
        openlink_store::StoreError::Duplicate(_) => (StatusCode::CONFLICT, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    tracing::info!(code = %code, "Link created");

    Ok((StatusCode::CREATED, Json(LinkResponse::from(created))))
}

/// 查询短链信息
///
/// GET /v1/links/:id
pub async fn get_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<LinkResponse>, (StatusCode, String)> {
    // 先通过 code 查找 link（兼容 id 和 code 两种传入方式）
    let link = state
        .store
        .get_link_by_code(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", id)))?;

    Ok(Json(LinkResponse::from(link)))
}

/// 列出短链（Phase 2: 支持分页）
///
/// GET /v1/links
pub async fn list_links(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListLinksQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let limit = query.limit.min(100) as usize;
    let links = state
        .store
        .list_links(None, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 获取总数
    let stats = state
        .store
        .get_overview_stats()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let link_responses: Vec<LinkResponse> = links.into_iter().map(LinkResponse::from).collect();

    Ok(Json(serde_json::json!({
        "links": link_responses,
        "total": stats.active_links,
        "offset": query.offset,
        "limit": query.limit,
    })))
}

/// 更新短链
///
/// PUT /v1/links/:id
pub async fn update_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateLinkRequest>,
) -> Result<Json<LinkResponse>, (StatusCode, String)> {
    // 先获取现有 link
    let existing = state
        .store
        .get_link(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", id)))?;

    // 更新字段
    let updated_link = Link {
        id: existing.id,
        code: existing.code,
        payload: req.payload,
        owner: existing.owner,
        created_at: existing.created_at,
        metadata: req.metadata,
        is_active: existing.is_active,
    };

    let updated = state.store.update_link(&updated_link).await.map_err(|e| match e {
        openlink_store::StoreError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    tracing::info!(id = %id, "Link updated");
    Ok(Json(LinkResponse::from(updated)))
}

/// 删除短链（软删除）
///
/// DELETE /v1/links/:id
pub async fn delete_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // 先通过 code 查找 link，获取 id 后删除
    let link = state
        .store
        .get_link_by_code(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", id)))?;
    state.store.delete_link(&link.id).await.map_err(|e| match e {
        openlink_store::StoreError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    tracing::info!(id = %id, "Link deleted");
    Ok(StatusCode::NO_CONTENT)
}

// ─── 新增：单条解析短链 ────────────────────────────────────────

use axum::extract::Query as AxumQuery;

/// 解析短链（返回目标 URL，不执行重定向）
///
/// GET /api/v1/resolve/:code
///
/// Phase 2: 新增 API，用于 Agent 或程序化调用获取短链目标
pub async fn resolve_link(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // 1. 查找 Link
    let link = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Link '{}' not found", code)))?;

    if !link.is_active {
        return Err((StatusCode::GONE, "Link is inactive".to_string()));
    }

    // 2. 获取 Route（如果有）
    let route = state
        .store
        .get_route_by_link_id(&link.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 3. 构建响应
    let target_url = route
        .as_ref()
        .and_then(|r| {
            r.default_target
                .params
                .get("url")
                .or(r.default_target.params.get("target"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .or_else(|| {
            link.payload
                .get("target_url")
                .or(link.payload.get("target"))
                .and_then(|v| v.as_str())
                .map(String::from)
        });

    let response = serde_json::json!({
        "code": code,
        "link_id": link.id,
        "target": target_url,
        "is_active": link.is_active,
        "metadata": link.metadata,
        "created_at": link.created_at.to_rfc3339(),
    });

    tracing::info!(code = %code, "Link resolved via API");
    Ok(Json(response))
}

// ─── 新增：批量查询短链 ────────────────────────────────────────

/// 批量查询短链请求
#[derive(Debug, Deserialize)]
pub struct BatchLinksQuery {
    pub codes: String, // 逗号分隔的短码列表
}

/// 批量查询短链
///
/// GET /api/v1/links/batch?codes=code1,code2,code3
///
/// Phase 2: 新增 API，用于批量查询短链信息
pub async fn batch_links(
    State(state): State<Arc<AppState>>,
    Query(query): AxumQuery<BatchLinksQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let codes: Vec<String> = query
        .codes
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(100)
        .collect();

    let requested = codes.len();
    if requested == 0 {
        return Err((StatusCode::BAD_REQUEST, "No codes provided".to_string()));
    }

    let mut results = Vec::new();

    for code in codes {
        let link = state
            .store
            .get_link_by_code(&code)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        match link {
            Some(link) => {
                let target_url = link
                    .payload
                    .get("target_url")
                    .or(link.payload.get("target"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                results.push(serde_json::json!({
                    "code": code,
                    "link_id": link.id,
                    "target": target_url,
                    "is_active": link.is_active,
                    "found": true,
                }));
            }
            None => {
                results.push(serde_json::json!({
                    "code": code,
                    "found": false,
                }));
            }
        }
    }

    let response = serde_json::json!({
        "results": results,
        "total": results.len(),
        "requested": requested,
    });

    tracing::info!(count = results.len(), "Batch links query completed");
    Ok(Json(response))
}
