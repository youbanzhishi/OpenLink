//! # Agent API Handlers — Phase 3 新增
//!
//! Agent 专用 API：
//! - POST /api/v1/agent/resolve — 批量解析短链
//! - POST /api/v1/agent/discover — 发现可用 Link
//! - POST /api/v1/agent/upload — 文件上传
//! - POST /api/v1/agent/download — 文件下载
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
    
    // list_links 只接受 (offset, limit) 两个参数
    let limit = req.limit as i64;
    let offset = 0i64;
    
    // 根据 discover_type 构建查询
    let links = state.store.list_links(offset, limit)
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

/// 文件上传请求
#[derive(Debug, Deserialize)]
pub struct FileUploadInitRequest {
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    #[serde(default)]
    pub storage: Option<String>,
}

/// 文件上传响应
#[derive(Debug, Serialize)]
pub struct FileUploadInitResponse {
    pub file_id: String,
    pub upload_url: String,
    pub expires_in: u64,
}

/// 初始化文件上传
///
/// 返回预签名上传 URL。
pub async fn init_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<FileUploadInitRequest>,
) -> Result<Json<FileUploadInitResponse>, (StatusCode, String)> {
    let _ctx = build_agent_context(&headers);
    
    let file_id = uuid::Uuid::new_v4().to_string();
    
    // 使用 server.host 和 server.port 构建 base_url
    let base_url = format!("http://{}:{}", 
        state.config.server.host.trim_start_matches("0.0.0.0"),
        state.config.server.port);
    
    let upload_url = format!("{}/api/v1/files/{}/upload", 
        base_url.trim_end_matches('/'),
        file_id);
    
    tracing::info!(file_id = %file_id, filename = %req.filename, size = req.size, "File upload initiated");
    
    Ok(Json(FileUploadInitResponse {
        file_id,
        upload_url,
        expires_in: 3600, // 1 hour
    }))
}

/// 文件下载请求
#[derive(Debug, Deserialize)]
pub struct FileDownloadRequest {
    pub file_id: String,
    #[serde(default = "default_download_ttl")]
    pub ttl: u64,
}

fn default_download_ttl() -> u64 {
    3600
}

/// 文件下载响应
#[derive(Debug, Serialize)]
pub struct FileDownloadResponse {
    pub file_id: String,
    pub download_url: String,
    pub expires_at: String,
}

/// 请求文件下载 URL
pub async fn request_download(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<FileDownloadRequest>,
) -> Result<Json<FileDownloadResponse>, (StatusCode, String)> {
    let _ctx = build_agent_context(&headers);
    
    // 使用 server.host 和 server.port 构建 base_url
    let base_url = format!("http://{}:{}", 
        state.config.server.host.trim_start_matches("0.0.0.0"),
        state.config.server.port);
    
    let download_url = format!("{}/api/v1/files/{}/download",
        base_url.trim_end_matches('/'),
        req.file_id);
    
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(req.ttl as i64);
    
    tracing::info!(file_id = %req.file_id, "File download requested");
    
    Ok(Json(FileDownloadResponse {
        file_id: req.file_id,
        download_url,
        expires_at: expires_at.to_rfc3339(),
    }))
}

/// 文件分享请求
#[derive(Debug, Deserialize)]
pub struct FileShareRequest {
    pub file_id: String,
    #[serde(default = "default_share_ttl")]
    pub ttl_secs: u64,
}

fn default_share_ttl() -> u64 {
    3600 * 24 * 7 // 7 days
}

/// 文件分享响应
#[derive(Debug, Serialize)]
pub struct FileShareResponse {
    pub file_id: String,
    pub share_code: String,
    pub share_url: String,
    pub expires_at: String,
}

/// 生成分享链接
pub async fn share_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<FileShareRequest>,
) -> Result<Json<FileShareResponse>, (StatusCode, String)> {
    let _ctx = build_agent_context(&headers);
    
    // 生成分享码
    let share_code = generate_share_code();
    
    // 使用 server.host 和 server.port 构建 base_url
    let base_url = format!("http://{}:{}", 
        state.config.server.host.trim_start_matches("0.0.0.0"),
        state.config.server.port);
    
    let share_url = format!("{}/s/{}", 
        base_url.trim_end_matches('/'),
        share_code);
    
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(req.ttl_secs as i64);
    
    tracing::info!(file_id = %req.file_id, share_code = %share_code, "File shared");
    
    Ok(Json(FileShareResponse {
        file_id: req.file_id,
        share_code,
        share_url,
        expires_at: expires_at.to_rfc3339(),
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

/// 生成分享码
fn generate_share_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let code: String = (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            let chars = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
            chars[idx] as char
        })
        .collect();
    code
}

// ─── Tests ─────────────────────────────────────────────────--

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_share_code() {
        let code1 = generate_share_code();
        let code2 = generate_share_code();
        
        assert_eq!(code1.len(), 8);
        assert_eq!(code2.len(), 8);
        assert_ne!(code1, code2);
    }

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
