//! # Agent API Handlers — Phase 3 新增
//!
//! Agent 专用 API：
//! - POST /api/v1/agent/resolve — 批量解析短链
//! - POST /api/v1/agent/discover — 发现可用 Link
//!
//! 认证使用 X-Agent-ID / X-Agent-Type Header

use axum::{
    extract::{State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;
use openlink_core::Context;

// ─── Request/Response Models ────────────────────────────────

/// 批量解析请求
#[derive(Debug, Deserialize)]
pub struct BatchResolveRequest {
    pub codes: Vec<String>,
}

/// 单个解析结果
#[derive(Debug, Serialize)]
pub struct ResolveResult {
    pub code: String,
    pub link_id: Option<String>,
    pub target: Option<String>,
    pub action: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub found: bool,
}

/// 批量解析响应
#[derive(Debug, Serialize)]
pub struct BatchResolveResponse {
    pub results: Vec<ResolveResult>,
}

/// 发现请求
#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub discover_type: String,
    #[serde(default)]
    pub filters: serde_json::Value,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

/// 发现响应
#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub links: Vec<serde_json::Value>,
    pub total: usize,
}

// ─── Handlers ────────────────────────────────────────────────

/// 批量解析短链
///
/// Agent 调用此 API 批量解析多个短码，避免多次 HTTP 请求。
/// 
/// Headers:
/// - X-Agent-ID: Agent 唯一标识
/// - X-Agent-Type: Agent 类型（如 "assistant", "crawler"）
/// - X-Device-ID: 设备标识（可选）
pub async fn batch_resolve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<BatchResolveRequest>,
) -> Result<Json<BatchResolveResponse>, (StatusCode, String)> {
    // 构建 Agent Context
    let _ctx = build_agent_context(&headers);
    
    let mut results = Vec::new();
    
    for code in req.codes.into_iter().take(100) {
        // 查找 Link - 使用正确的 Store 方法名 get_link_by_code
        let link = state.store.get_link_by_code(&code).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        match link {
            Some(link) => {
                // 获取 Route - 使用正确的 Store 方法名 get_route_by_link_id
                let route = state.store.get_route_by_link_id(&link.id).await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                
                let target = route.as_ref()
                    .map(|r| serde_json::to_value(&r.default_target).ok())
                    .flatten();
                
                let action = route.as_ref()
                    .map(|r| r.default_target.action.as_str().to_string());
                
                results.push(ResolveResult {
                    code,
                    link_id: Some(link.id),
                    target: target.and_then(|t| t.get("url").or(t.get("target")).and_then(|v| v.as_str()).map(String::from)),
                    action,
                    metadata: Some(link.metadata),
                    found: true,
                });
            }
            None => {
                results.push(ResolveResult {
                    code,
                    link_id: None,
                    target: None,
                    action: None,
                    metadata: None,
                    found: false,
                });
            }
        }
    }
    
    tracing::info!(agent_id = ?headers.get("x-agent-id").and_then(|v| v.to_str().ok()), 
                    count = results.len(), "Batch resolve completed");
    
    Ok(Json(BatchResolveResponse { results }))
}

/// 发现可用 Link
///
/// 根据类型和过滤器发现可用的 Link。
pub async fn discover(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DiscoverRequest>,
) -> Result<Json<DiscoverResponse>, (StatusCode, String)> {
    let _ctx = build_agent_context(&headers);
    
    let limit = req.limit.min(100);
    
    // list_links(owner, limit)
    let links = state.store.list_links(None, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let total = links.len();
    let link_values: Vec<serde_json::Value> = links
        .into_iter()
        .map(|l| serde_json::to_value(l).unwrap_or_default())
        .collect();
    
    tracing::info!(agent_id = ?headers.get("x-agent-id").and_then(|v| v.to_str().ok()),
                    discover_type = %req.discover_type,
                    count = total, "Discover completed");
    
    Ok(Json(DiscoverResponse {
        links: link_values,
        total,
    }))
}

// ─── Helpers ────────────────────────────────────────────────

/// 从 Header 构建 Agent Context
fn build_agent_context(headers: &HeaderMap) -> Context {
    let agent_id = headers.get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    
    let agent_type = headers.get("x-agent-type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    
    let device_id = headers.get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    
    let mut ctx = Context::from_request(
        headers.get("user-agent").and_then(|v| v.to_str().ok()),
        None,
    );
    
    // 设置 Agent 身份
    if agent_id.is_some() || agent_type.is_some() {
        ctx.identity.identity_type = openlink_core::IdentityType::Agent;
        if let Some(id) = agent_id {
            ctx.identity.id = id;
        }
        ctx.identity.agent_type = agent_type;
    }
    
    // 设置设备 ID
    if let Some(id) = device_id {
        ctx.device.device_type = Some(id);
    }
    
    ctx
}

// ─── Tests ─────────────────────────────────────────────────--

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_agent_context() {
        let mut headers = HeaderMap::new();
        headers.insert("x-agent-id", "test-agent".parse().unwrap());
        headers.insert("x-agent-type", "assistant".parse().unwrap());
        
        let ctx = build_agent_context(&headers);
        
        assert_eq!(ctx.identity.identity_type, openlink_core::IdentityType::Agent);
        assert_eq!(ctx.identity.id, "test-agent");
        assert_eq!(ctx.identity.agent_type.as_deref(), Some("assistant"));
    }
}
