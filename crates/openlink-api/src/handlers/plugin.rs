//! # Plugin & Share API Handlers
//!
//! 插件注册、搜索、安装和项目分享相关的 HTTP 处理器。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

// ─── Plugin Models ──────────────────────────────────────────

/// 插件注册请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterPluginRequest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub plugin_type: String,
    pub format: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub download_url: Option<String>,
    #[serde(default)]
    pub compatibility: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<DependencyDecl>,
}

/// 依赖声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyDecl {
    pub plugin_id: String,
    pub min_version: String,
}

/// 插件搜索查询
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginSearchQuery {
    pub plugin_type: Option<String>,
    pub format: Option<String>,
    pub tags: Option<Vec<String>>,
    pub author: Option<String>,
    pub keyword: Option<String>,
    pub compatibility: Option<String>,
}

/// 插件安装请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPluginRequest {
    pub plugin_id: String,
    #[serde(default)]
    pub install_dir: Option<String>,
    #[serde(default)]
    pub verify_checksum: Option<bool>,
}

/// 插件信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub plugin_type: String,
    pub format: String,
    pub version: String,
    pub author: String,
    pub tags: Vec<String>,
    pub download_url: Option<String>,
    pub compatibility: Vec<String>,
}

/// 插件安装响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPluginResponse {
    pub plugin_id: String,
    pub status: String,
    pub install_path: Option<String>,
    pub error: Option<String>,
}

// ─── Share Models ───────────────────────────────────────────

/// 项目分享请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareProjectApiRequest {
    pub project_id: String,
    pub project_name: String,
    #[serde(default)]
    pub description: String,
    pub project_url: String,
    #[serde(default = "default_permission")]
    pub permission: String,
    #[serde(default)]
    pub team_members: Vec<String>,
    #[serde(default)]
    pub ttl_secs: u64,
}

fn default_permission() -> String {
    "public".to_string()
}

/// 项目分享响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareProjectApiResponse {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub deeplink: String,
    pub share_code: String,
    pub permission: String,
}

/// 项目详情响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetailResponse {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub description: String,
    pub deeplink: String,
    pub permission: String,
    pub created_at: String,
}

// ─── Handlers ───────────────────────────────────────────────

/// POST /api/v1/plugins — 注册插件
pub async fn register_plugin(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<RegisterPluginRequest>,
) -> Result<(StatusCode, Json<PluginResponse>), (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(plugin_id = %req.id, name = %req.name, "Registering plugin");

    let response = PluginResponse {
        id: req.id,
        name: req.name,
        description: req.description,
        plugin_type: req.plugin_type,
        format: req.format,
        version: req.version,
        author: req.author,
        tags: req.tags,
        download_url: req.download_url,
        compatibility: req.compatibility,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /api/v1/plugins/search — 搜索插件
pub async fn search_plugins(
    State(_state): State<Arc<AppState>>,
    Json(query): Json<PluginSearchQuery>,
) -> Result<Json<Vec<PluginResponse>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(keyword = ?query.keyword, "Searching plugins");

    // Placeholder: in production, query the registry
    let results: Vec<PluginResponse> = vec![];

    Ok(Json(results))
}

/// POST /api/v1/plugins/:id/install — 安装插件
pub async fn install_plugin(
    State(_state): State<Arc<AppState>>,
    Path(plugin_id): Path<String>,
    Json(_req): Json<InstallPluginRequest>,
) -> Result<Json<InstallPluginResponse>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(plugin_id = %plugin_id, "Installing plugin");

    let response = InstallPluginResponse {
        plugin_id,
        status: "queued".to_string(),
        install_path: None,
        error: None,
    };

    Ok(Json(response))
}

/// POST /api/v1/share/project — 分享项目
pub async fn share_project(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ShareProjectApiRequest>,
) -> Result<(StatusCode, Json<ShareProjectApiResponse>), (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(project_id = %req.project_id, "Sharing project");

    let deeplink = if req.project_url.starts_with("http") {
        format!("opendaw://project?url={}", urlencoding::encode(&req.project_url))
    } else {
        format!("opendaw://project/{}", req.project_id)
    };

    let share_code = openlink_core::generate_default();

    let response = ShareProjectApiResponse {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: req.project_id,
        project_name: req.project_name,
        deeplink,
        share_code,
        permission: req.permission,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /api/v1/share/:id — 获取分享的项目
pub async fn get_shared_project(
    State(_state): State<Arc<AppState>>,
    Path(share_id): Path<String>,
) -> Result<Json<ProjectDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(share_id = %share_id, "Getting shared project");

    // Placeholder: in production, look up the share
    Err((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "Share not found",
            "share_id": share_id
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_plugin_request_deserialization() {
        let json = r#"{
            "id": "vst3-eq",
            "name": "Parametric EQ",
            "description": "A parametric EQ",
            "plugin_type": "effect",
            "format": "vst3",
            "version": "1.0.0",
            "author": "OpenDAW",
            "tags": ["eq", "filter"],
            "download_url": "https://example.com/eq.vst3",
            "compatibility": ["OpenDAW", "REAPER"]
        }"#;
        let req: RegisterPluginRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "vst3-eq");
        assert_eq!(req.tags.len(), 2);
    }

    #[test]
    fn test_plugin_search_query_default() {
        let query = PluginSearchQuery::default();
        assert!(query.keyword.is_none());
        assert!(query.format.is_none());
    }

    #[test]
    fn test_share_project_request_deserialization() {
        let json = r#"{
            "project_id": "proj-1",
            "project_name": "My Song",
            "project_url": "https://example.com/song.opendaw",
            "permission": "public",
            "ttl_secs": 86400
        }"#;
        let req: ShareProjectApiRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.project_id, "proj-1");
        assert_eq!(req.ttl_secs, 86400);
    }

    #[test]
    fn test_install_plugin_response_serialization() {
        let resp = InstallPluginResponse {
            plugin_id: "vst3-eq".to_string(),
            status: "queued".to_string(),
            install_path: None,
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("queued"));
    }

    #[test]
    fn test_plugin_response_serialization() {
        let resp = PluginResponse {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Desc".to_string(),
            plugin_type: "effect".to_string(),
            format: "vst3".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            tags: vec![],
            download_url: None,
            compatibility: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("vst3"));
    }
}
