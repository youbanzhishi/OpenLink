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
                    .and_then(|r| serde_json::to_value(&r.default_target).ok());

                let action = route.as_ref().map(|r| r.default_target.action.as_str().to_string());

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

    let mut ctx = Context::from_request(headers.get("user-agent").and_then(|v| v.to_str().ok()), None);

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
    State(_state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let config_path = std::path::Path::new("config/agent.json");

    let content = if config_path.exists() {
        fs::read_to_string(config_path).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read agent.json: {}", e),
            )
        })?
    } else {
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
        serde_json::to_string_pretty(&default).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
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
    let agent_id = headers
        .get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous");

    if agent_id == "anonymous" {
        return Err((StatusCode::UNAUTHORIZED, "X-Agent-ID header required".into()));
    }

    let service_id = req.service.id.clone();
    let mut results = Vec::new();

    for action in &req.auto_config {
        let result = match action.as_str() {
            "verify" => {
                if let Some(endpoint) = &req.service.endpoint {
                    match reqwest::Client::new()
                        .head(endpoint)
                        .timeout(std::time::Duration::from_secs(10))
                        .send()
                        .await
                    {
                        Ok(resp) => ConfigActionResult {
                            action: "verify".into(),
                            status: if resp.status().is_success() {
                                "completed".into()
                            } else {
                                "failed".into()
                            },
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
                tracing::info!(
                    agent_id = agent_id,
                    service_id = %service_id,
                    "Auto-config notification for new service"
                );
                ConfigActionResult {
                    action: "notify".into(),
                    status: "completed".into(),
                    message: Some(format!("Notified for service {}", service_id)),
                }
            }
            "index" | "scan" | "clone" => ConfigActionResult {
                action: action.clone(),
                status: "pending".into(),
                message: Some(format!("{} requires external API call, queued", action)),
            },
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
                            let exists = arr
                                .iter()
                                .any(|s| s.get("id").and_then(|v| v.as_str()) == Some(&service_id));
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

    tracing::info!(
        agent_id = agent_id,
        service_id = %service_id,
        action = %req.action,
        "Service config completed"
    );

    Ok(Json(ConfigResponse {
        status: "ok".into(),
        service_id,
        auto_config_results: results,
    }))
}

// ─── Join Knowledge System ────────────────────────────────────

/// 加入知识体系请求
#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    /// 智能体自报的身份
    pub agent_name: String,
    /// 智能体类型：assistant / tool / robot
    #[serde(default = "default_agent_type")]
    pub agent_type: String,
    /// 智能体希望承担的角色（可选，不填则返回所有角色清单）
    #[serde(default)]
    pub desired_role: Option<String>,
    /// 智能体的能力列表（帮助匹配角色）
    #[serde(default)]
    pub capabilities: Vec<String>,
}

fn default_agent_type() -> String {
    "assistant".into()
}

/// 角色摘要
#[derive(Debug, Serialize)]
pub struct RoleSummary {
    pub name: String,
    pub description: String,
    pub domain: String,
    pub skills: Vec<String>,
    pub rules_path: String,
}

/// 协议摘要
#[derive(Debug, Serialize)]
pub struct ProtocolSummary {
    pub name: String,
    pub steps: Vec<String>,
    pub description: String,
}

/// 加入知识体系响应
#[derive(Debug, Serialize)]
pub struct JoinResponse {
    pub status: String,
    pub schema_version: String,
    /// 知识体系仓库信息
    pub knowledge_repo: KnowledgeRepoInfo,
    /// 入口文档内容
    pub entry_content: String,
    /// 可选角色清单
    pub available_roles: Vec<RoleSummary>,
    /// 核心协议摘要
    pub protocols: Vec<ProtocolSummary>,
    /// 下一步指引
    pub next_steps: Vec<String>,
    /// 加入凭证（只读token，用于后续访问受保护服务）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
}

/// 知识体系仓库信息
#[derive(Debug, Serialize)]
pub struct KnowledgeRepoInfo {
    pub repo_url: String,
    pub branch: String,
    pub entry_file: String,
    pub init_script: String,
    pub clone_command: String,
}

/// GET /api/v1/agent/join — 智能体加入知识体系（只读加入）
///
/// 外部智能体调用此端点，获取知识体系的完整概览：
/// - 仓库地址和克隆命令
/// - 入口文档内容
/// - 可选角色清单
/// - 核心协议摘要
/// - 下一步操作指引
///
/// 不需要Bearer token（公开层），但只返回只读信息。
/// 读写加入需要通过主代理（小龙）授权。
pub async fn join_knowledge(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<JoinRequest>,
) -> Result<Json<JoinResponse>, (StatusCode, String)> {
    let agent_id = headers
        .get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous");

    tracing::info!(
        agent_id = agent_id,
        agent_name = %req.agent_name,
        agent_type = %req.agent_type,
        desired_role = ?req.desired_role,
        "Agent requesting to join knowledge system"
    );

    // 1. 构建仓库信息
    let knowledge_repo = KnowledgeRepoInfo {
        repo_url: "https://github.com/youbanzhishi/open-knowledge-system.git".into(),
        branch: "master".into(),
        entry_file: "入口.md".into(),
        init_script: "scripts/init.sh".into(),
        clone_command: "git clone -b master https://github.com/youbanzhishi/open-knowledge-system.git".to_string(),
    };

    // 2. 读取入口文档预览
    let entry_content = if let Ok(content) = tokio::fs::read_to_string("config/entry-preview.md").await {
        content
    } else {
        // 本地无缓存时返回简要指引
        "# 知识体系入口预览

        > 知识不等于行为，行为不能靠自觉，要靠机制

        ## 加入流程
        1. git clone -b master https://github.com/youbanzhishi/open-knowledge-system.git
        2. 读 入口.md 了解全局
        3. 选角色 → 读 角色/{角色名}/RULES.md
        4. bash scripts/init.sh 初始化
        5. 之后所有任务走五步门/六步门

        ## 核心协议
        - 五步门：查→干→验→记→交
        - 六步门(开发)：编译→测试→构建→CI→桌面→文档
        - 铁律：只add自己的/禁stash/交付物必须完整/文档完整性/DR必检"
            .into()
    };

    // 3. 构建角色清单
    let available_roles = vec![
        RoleSummary {
            name: "系统开发者".into(),
            description: "跨项目系统开发方法论+经验积累".into(),
            domain: "系统开发（网络协议/基础设施/存储引擎）".into(),
            skills: vec!["Rust".into(), "Python".into(), "C/C++".into(), "Docker".into()],
            rules_path: "角色/系统开发者/RULES.md".into(),
        },
        RoleSummary {
            name: "前端开发".into(),
            description: "前端开发+UI/UX实现".into(),
            domain: "前端开发（Web/移动端/桌面）".into(),
            skills: vec!["TypeScript".into(), "React".into(), "Vue".into(), "Tauri".into()],
            rules_path: "角色/前端开发/RULES.md".into(),
        },
        RoleSummary {
            name: "ECS运维".into(),
            description: "ECS服务器运维+Docker部署".into(),
            domain: "服务器运维".into(),
            skills: vec!["Docker".into(), "Linux".into(), "Nginx".into()],
            rules_path: "角色/ECS运维/RULES.md".into(),
        },
        RoleSummary {
            name: "自媒体运营".into(),
            description: "多平台自媒体内容运营".into(),
            domain: "自媒体运营".into(),
            skills: vec!["小红书".into(), "头条".into(), "知乎".into(), "微博".into()],
            rules_path: "角色/自媒体运营/RULES.md".into(),
        },
        RoleSummary {
            name: "网文写手".into(),
            description: "网文创作+IP运营".into(),
            domain: "网文创作".into(),
            skills: vec!["小说创作".into(), "剧情设计".into(), "角色塑造".into()],
            rules_path: "角色/网文写手/RULES.md".into(),
        },
        RoleSummary {
            name: "AI调教师".into(),
            description: "AI训练+知识体系维护".into(),
            domain: "AI调教".into(),
            skills: vec!["Prompt工程".into(), "知识管理".into(), "体系优化".into()],
            rules_path: "角色/AI调教师/RULES.md".into(),
        },
        RoleSummary {
            name: "混音母带工程师".into(),
            description: "音频混音+母带处理".into(),
            domain: "音频制作".into(),
            skills: vec!["REAPER".into(), "混音".into(), "母带".into()],
            rules_path: "角色/混音母带工程师/RULES.md".into(),
        },
        RoleSummary {
            name: "动画导演".into(),
            description: "动画项目总控+分镜设计".into(),
            domain: "动画制作".into(),
            skills: vec!["分镜".into(), "剧本".into(), "角色设计".into()],
            rules_path: "角色/动画导演/RULES.md".into(),
        },
        RoleSummary {
            name: "动画设计师".into(),
            description: "动画制作+美术设计".into(),
            domain: "动画美术".into(),
            skills: vec!["动画制作".into(), "美术设计".into(), "角色原画".into()],
            rules_path: "角色/动画设计师/RULES.md".into(),
        },
        RoleSummary {
            name: "游戏开发工程师".into(),
            description: "游戏开发+引擎编程".into(),
            domain: "游戏开发".into(),
            skills: vec!["Godot".into(), "Rust".into(), "游戏引擎".into()],
            rules_path: "角色/游戏开发工程师/RULES.md".into(),
        },
        RoleSummary {
            name: "游戏美术设计师".into(),
            description: "游戏美术+UI设计".into(),
            domain: "游戏美术".into(),
            skills: vec!["像素画".into(), "UI设计".into(), "角色设计".into()],
            rules_path: "角色/游戏美术设计师/RULES.md".into(),
        },
        RoleSummary {
            name: "短视频运营".into(),
            description: "短视频内容制作+平台运营".into(),
            domain: "短视频运营".into(),
            skills: vec!["抖音".into(), "视频剪辑".into(), "脚本编写".into()],
            rules_path: "角色/短视频运营/RULES.md".into(),
        },
        RoleSummary {
            name: "本地运维".into(),
            description: "本地开发环境运维".into(),
            domain: "本地环境运维".into(),
            skills: vec!["Docker".into(), "Homebrew".into(), "开发环境".into()],
            rules_path: "角色/本地运维/RULES.md".into(),
        },
        RoleSummary {
            name: "任务助手".into(),
            description: "通用任务执行+信息整理".into(),
            domain: "通用助手".into(),
            skills: vec!["信息搜索".into(), "文档整理".into(), "任务执行".into()],
            rules_path: "角色/任务助手/RULES.md".into(),
        },
    ];

    // 4. 核心协议摘要
    let protocols = vec![
        ProtocolSummary {
            name: "五步门".into(),
            steps: vec![
                "① 查后定方案".into(),
                "② 执行方案".into(),
                "③ 测试验证".into(),
                "④ 反哺沉淀".into(),
                "⑤ 交付闭环".into(),
            ],
            description: "每次任务必走，跳过=不合格。通用工作流程。".into(),
        },
        ProtocolSummary {
            name: "六步门".into(),
            steps: vec![
                "① 编译0错误".into(),
                "② 测试0失败".into(),
                "③ 本地构建验证".into(),
                "④ CI全平台构建".into(),
                "⑤ 桌面构建(仅GUI)".into(),
                "⑥ 文档同步更新".into(),
            ],
            description: "开发交付专用。六步全过才算开发完成。".into(),
        },
    ];

    // 5. 下一步指引
    let next_steps = vec![
        format!("1. 克隆知识体系仓库：{}", knowledge_repo.clone_command),
        "2. 阅读 入口.md 了解体系全局结构".into(),
        if let Some(ref role) = req.desired_role {
            format!("3. 阅读 角色/{}/RULES.md 了解你的角色规则", role)
        } else {
            "3. 根据你的能力选择一个角色，阅读对应 RULES.md".into()
        },
        "4. 执行 scripts/init.sh 完成环境初始化".into(),
        "5. 之后所有任务按五步门/六步门执行".into(),
        "6. 如需读写权限（push到仓库），联系主代理小龙获取授权".into(),
    ];

    // 6. 如果智能体指定了角色，尝试生成只读token
    let access_token = if req.desired_role.is_some() {
        // 生成一个只读token（JWT或随机token，后续对接认证系统）
        Some(format!("read-only-{}-{}", req.agent_name, uuid::Uuid::new_v4()))
    } else {
        None
    };

    tracing::info!(
        agent_name = %req.agent_name,
        role = ?req.desired_role,
        "Agent joined knowledge system (read-only)"
    );

    Ok(Json(JoinResponse {
        status: "joined".into(),
        schema_version: "0.3.0".into(),
        knowledge_repo,
        entry_content,
        available_roles,
        protocols,
        next_steps,
        access_token,
    }))
}
