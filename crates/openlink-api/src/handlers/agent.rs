//! # Agent API Handlers — Phase 3 新增
//!
//! Agent 专用 API：
//! - POST /api/v1/agent/resolve — 批量解析短链
//! - POST /api/v1/agent/discover — 发现可用 Link
//!
//! 认证使用 X-Agent-ID / X-Agent-Type Header

use axum::{
    extract::State,
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
        let link = state
            .store
            .get_link_by_code(&code)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        match link {
            Some(link) => {
                // 获取 Route - 使用正确的 Store 方法名 get_route_by_link_id
                let route = state
                    .store
                    .get_route_by_link_id(&link.id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                let target = route
                    .as_ref()
                    .map(|r| serde_json::to_value(&r.default_target).ok())
                    .flatten();

                let action = route
                    .as_ref()
                    .map(|r| r.default_target.action.as_str().to_string());

                results.push(ResolveResult {
                    code,
                    link_id: Some(link.id),
                    target: target.and_then(|t| {
                        t.get("url")
                            .or(t.get("target"))
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    }),
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
    let links = state
        .store
        .list_links(None, limit)
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
    let agent_id = headers
        .get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let agent_type = headers
        .get("x-agent-type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let device_id = headers
        .get("x-device-id")
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

        assert_eq!(
            ctx.identity.identity_type,
            openlink_core::IdentityType::Agent
        );
        assert_eq!(ctx.identity.id, "test-agent");
        assert_eq!(ctx.identity.agent_type.as_deref(), Some("assistant"));
    }
}

// ─── Person Agent Schema (v0.2.0) ────────────────────────────

use std::collections::HashMap;
use tokio::fs;

/// Person Agent identity declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonAgent {
    pub schema_version: String,
    pub identity: Identity,
    pub capabilities: Vec<Capability>,
    pub services: Services,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Preferences>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Links>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRef {
    pub name: String,
    pub endpoint: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Services {
    pub public: Vec<Service>,
    #[serde(rename = "protected")]
    pub protected_services: Vec<Service>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub service_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_priority: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_preferences: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation: Option<Delegation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegation {
    pub agent_name: String,
    pub protocol: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Links {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_world: Option<String>,
}

// ─── Auto-Config ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ConfigRequest {
    pub action: String,
    pub service: Service,
    #[serde(default)]
    pub auto_config: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub status: String,
    pub service_id: String,
    pub auto_config_results: Vec<ConfigActionResult>,
}

#[derive(Debug, Serialize)]
pub struct ConfigActionResult {
    pub action: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// GET /.well-known/agent.json — 返回人的数字身份声明
pub async fn person_agent(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let config_path = std::path::Path::new("config/agent.json");

    let content = if config_path.exists() {
        fs::read_to_string(config_path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read agent.json: {}", e)))?
    } else {
        // 最小默认schema
        let default = PersonAgent {
            schema_version: "0.2.0".into(),
            identity: Identity {
                name: "Unknown".into(),
                entity_type: "person".into(),
                bio: None,
                avatar: None,
                agent: None,
            },
            capabilities: vec![],
            services: Services {
                public: vec![],
                protected_services: vec![],
            },
            preferences: None,
            auth: None,
            links: None,
        };
        serde_json::to_string_pretty(&default)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid agent.json: {}", e)))?;

    tracing::info!("Served .well-known/agent.json");
    Ok(Json(value))
}

/// POST /api/v1/agent/config — 注册服务+触发auto-config
pub async fn config_service(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ConfigRequest>,
) -> Result<Json<ConfigResponse>, (StatusCode, String)> {
    // 验证请求者身份
    let agent_id = headers
        .get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous");

    if agent_id == "anonymous" {
        return Err((StatusCode::UNAUTHORIZED, "X-Agent-ID header required".into()));
    }

    let service_id = req.service.id.clone();
    let mut results = Vec::new();

    // 执行auto_config动作
    for action in &req.auto_config {
        let result = match action.as_str() {
            "verify" => {
                if let Some(endpoint) = &req.service.endpoint {
                    match reqwest::Client::new().head(endpoint).timeout(std::time::Duration::from_secs(10)).send().await {
                        Ok(resp) => ConfigActionResult {
                            action: "verify".into(),
                            status: if resp.status().is_success() { "completed".into() } else { "failed".into() },
                            message: Some(format!("HTTP {}", resp.status())),
                        },
                        Err(e) => ConfigActionResult {
                            action: "verify".into(),
                            status: "failed".into(),
                            message: Some(e.to_string()),
                        },
                    }
                } else {
                    ConfigActionResult {
                        action: "verify".into(),
                        status: "skipped".into(),
                        message: Some("No endpoint provided".into()),
                    }
                }
            }
            "notify" => {
                // 记录通知事件（实际webhook调用在workflow引擎中）
                tracing::info!(agent_id = agent_id, service_id = %service_id, "Auto-config notification for new service");
                ConfigActionResult {
                    action: "notify".into(),
                    status: "completed".into(),
                    message: Some(format!("Notified for service {}", service_id)),
                }
            }
            "index" | "scan" | "clone" => {
                // 外部动作，返回pending（需调用OpenMind/OpenVault/GitHub API）
                ConfigActionResult {
                    action: action.clone(),
                    status: "pending".into(),
                    message: Some(format!("{} requires external API call, queued", action)),
                }
            }
            _ => ConfigActionResult {
                action: action.clone(),
                status: "skipped".into(),
                message: Some(format!("Unknown action: {}", action)),
            },
        };
        results.push(result);
    }

    // 更新agent.json：添加服务到对应列表
    let config_path = std::path::Path::new("config/agent.json");
    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(config_path).await {
            if let Ok(mut agent) = serde_json::from_str::<serde_json::Value>(&content) {
                let service_value = serde_json::to_value(&req.service).unwrap_or_default();
                let list_key = if req.service.auth_required.unwrap_or(false) {
                    "protected"
                } else {
                    "public"
                };
                if let Some(services) = agent.get_mut("services") {
                    if let Some(list) = services.get_mut(list_key) {
                        if let Some(arr) = list.as_array_mut() {
                            // 幂等：不重复添加
                            let exists = arr.iter().any(|s| s.get("id").and_then(|v| v.as_str()) == Some(&service_id));
                            if !exists {
                                arr.push(service_value);
                            }
                        }
                    }
                }
                if let Ok(updated) = serde_json::to_string_pretty(&agent) {
                    let _ = fs::write(config_path, updated).await;
                }
            }
        }
    }

    tracing::info!(agent_id = agent_id, service_id = %service_id, action = %req.action, "Service config completed");

    Ok(Json(ConfigResponse {
        status: "ok".into(),
        service_id,
        auto_config_results: results,
    }))
}
