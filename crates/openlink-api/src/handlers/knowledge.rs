//! # 知识体系 API Handlers — Phase 3 新增
//!
//! 知识体系一键接入 API：
//! - GET /.well-known/agent.json — Agent 发现端点
//! - POST /api/v1/knowledge/join — 加入知识体系
//! - GET /api/v1/knowledge/entry — 入口文档
//! - GET /api/v1/knowledge/role/{name} — 角色 RULES.md
//! - GET /api/v1/knowledge/project/{name} — 项目 INDEX.md
//! - GET /api/v1/knowledge/script/{name} — 脚本内容
//! - GET /api/v1/knowledge/hot-rules/{role} — 角色热规则

use axum::{
    extract::{State, Path, Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::path::PathBuf;
use std::fmt::Write;

use crate::state::AppState;
use openlink_core::Context;

// ─── Agent Discovery ─────────────────────────────────────────

/// Agent 发现响应
#[derive(Debug, Serialize)]
pub struct AgentDiscoveryResponse {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub knowledge_join_url: String,
    pub endpoints: AgentEndpoints,
}

/// Agent 端点信息
#[derive(Debug, Serialize)]
pub struct AgentEndpoints {
    pub join: String,
    pub entry: String,
    pub roles: String,
    pub projects: String,
    pub scripts: String,
}

/// GET /.well-known/agent.json
///
/// 返回 OpenLink 能力描述，包含知识体系入口。
/// 这是标准化的 Agent 发现协议。
pub async fn agent_discovery(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AgentDiscoveryResponse>, (StatusCode, String)> {
    let base_url = &state.config.knowledge.base_url;
    
    let response = AgentDiscoveryResponse {
        name: "OpenLink".to_string(),
        version: "0.2.0".to_string(),
        description: "智能体时代的通用路由与编排协议".to_string(),
        capabilities: vec![
            "shortlink".to_string(),
            "routing".to_string(),
            "knowledge_join".to_string(),
            "file_transfer".to_string(),
        ],
        knowledge_join_url: format!("{}/api/v1/knowledge/join", base_url),
        endpoints: AgentEndpoints {
            join: format!("{}/api/v1/knowledge/join", base_url),
            entry: format!("{}/api/v1/knowledge/entry", base_url),
            roles: format!("{}/api/v1/knowledge/role/{{name}}", base_url),
            projects: format!("{}/api/v1/knowledge/project/{{name}}", base_url),
            scripts: format!("{}/api/v1/knowledge/script/{{name}}", base_url),
        },
    };

    tracing::info!("Agent discovery requested");
    Ok(Json(response))
}

// ─── Knowledge Join ────────────────────────────────────────

/// Agent 类型
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Llm,
    Robot,
    Service,
    Custom,
}

impl Default for AgentType {
    fn default() -> Self {
        AgentType::Custom
    }
}

/// 加入知识体系请求
#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub invite_code: String,
    pub agent_name: String,
    #[serde(default)]
    pub agent_type: AgentType,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Agent Token 信息
#[derive(Debug, Serialize)]
pub struct AgentToken {
    pub token: String,
    pub expires_at: String,
}

/// 入口信息
#[derive(Debug, Serialize)]
pub struct EntryInfo {
    pub quick_start: String,
    pub full_guide: String,
}

/// 角色信息
#[derive(Debug, Serialize)]
pub struct RoleInfo {
    pub name: String,
    pub description: String,
    pub rules_url: String,
    pub hot_rules_url: Option<String>,
}

/// 项目信息
#[derive(Debug, Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub description: String,
    pub index_url: String,
}

/// 脚本信息
#[derive(Debug, Serialize)]
pub struct ScriptsInfo {
    pub act: String,
    pub handover: String,
}

/// 知识全景包
#[derive(Debug, Serialize)]
pub struct KnowledgePackage {
    pub version: String,
    pub system: String,
    pub entry: EntryInfo,
    pub roles: Vec<RoleInfo>,
    pub projects: Vec<ProjectInfo>,
    pub scripts: ScriptsInfo,
    pub token: AgentToken,
}

/// POST /api/v1/knowledge/join
///
/// 加入知识体系，验证邀请码后返回知识全景包。
pub async fn join_knowledge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<JoinRequest>,
) -> Result<Json<KnowledgePackage>, (StatusCode, String)> {
    // 验证邀请码（MVP: 写死在配置中）
    if !state.config.knowledge.enabled {
        return Err((StatusCode::SERVICE_UNAVAILABLE, 
            "Knowledge system is not enabled".to_string()));
    }
    
    if !state.config.knowledge.invite_codes.contains(&req.invite_code) {
        return Err((StatusCode::FORBIDDEN, 
            "Invalid invite code".to_string()));
    }

    let base_url = &state.config.knowledge.base_url;
    let repo_path = state.knowledge_repo_path.as_ref()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, 
            "Knowledge repository path not configured".to_string()))?;

    // 构建知识全景包
    let package = build_knowledge_package(repo_path, base_url)?;
    
    tracing::info!(
        agent_name = %req.agent_name,
        agent_type = ?req.agent_type,
        "Agent joined knowledge system"
    );
    
    Ok(Json(package))
}

/// 构建知识全景包
fn build_knowledge_package(repo_path: &str, base_url: &str) -> Result<KnowledgePackage, (StatusCode, String)> {
    let roles = list_roles_from_repo(repo_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let projects = list_projects_from_repo(repo_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 生成 Agent Token（简化实现）
    let token = format!("agt_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339();

    Ok(KnowledgePackage {
        version: "1.0".to_string(),
        system: "OpenClaw知识体系".to_string(),
        entry: EntryInfo {
            quick_start: format!("{}/api/v1/knowledge/entry", base_url),
            full_guide: format!("{}/api/v1/knowledge/entry?full=true", base_url),
        },
        roles,
        projects,
        scripts: ScriptsInfo {
            act: format!("{}/api/v1/knowledge/script/act.sh", base_url),
            handover: format!("{}/api/v1/knowledge/script/handover.sh", base_url),
        },
        token: AgentToken {
            token,
            expires_at,
        },
    })
}

/// 从知识仓库列出角色
fn list_roles_from_repo(repo_path: &str) -> Result<Vec<RoleInfo>, String> {
    let roles_dir = PathBuf::from(repo_path).join("角色");
    if !roles_dir.exists() {
        return Ok(vec![]);
    }

    let mut roles = Vec::new();
    for entry in std::fs::read_dir(roles_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            
            // 读取 RULES.md 获取描述
            let rules_path = path.join("RULES.md");
            let description = if rules_path.exists() {
                extract_description_from_rules(&rules_path)?
            } else {
                name.clone()
            };

            roles.push(RoleInfo {
                name: name.clone(),
                description,
                rules_url: format!("/api/v1/knowledge/role/{}", 
                    urlencoding_encode(&name)),
                hot_rules_url: Some(format!("/api/v1/knowledge/hot-rules/{}", 
                    urlencoding_encode(&name))),
            });
        }
    }

    Ok(roles)
}

/// 从知识仓库列出项目
fn list_projects_from_repo(repo_path: &str) -> Result<Vec<ProjectInfo>, String> {
    // 尝试两个位置：项目/ 和 项目文档/
    let projects_dirs = vec![
        PathBuf::from(repo_path).join("项目"),
        PathBuf::from(repo_path).join("项目文档"),
    ];

    let mut projects = Vec::new();
    
    for projects_dir in projects_dirs {
        if !projects_dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(projects_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                // 检查是否有 INDEX.md
                let index_path = path.join("INDEX.md");
                if index_path.exists() {
                    projects.push(ProjectInfo {
                        name: name.clone(),
                        description: "".to_string(),
                        index_url: format!("/api/v1/knowledge/project/{}", 
                            urlencoding_encode(&name)),
                    });
                }
            }
        }
    }

    Ok(projects)
}

/// 从 RULES.md 提取描述（取第一段非空文字）
fn extract_description_from_rules(rules_path: &PathBuf) -> Result<String, String> {
    let content = std::fs::read_to_string(rules_path)
        .map_err(|e| e.to_string())?;
    
    // 取第一行非空、非标题的内容作为描述
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('>') {
            // 截取前100字符
            let desc = if trimmed.len() > 100 {
                format!("{}...", &trimmed[..100])
            } else {
                trimmed.to_string()
            };
            return Ok(desc);
        }
    }
    
    Ok("".to_string())
}

/// URL 编码辅助函数
fn urlencoding_encode(s: &str) -> String {
    let mut encoded = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                encoded.push(c);
            }
            _ => {
                for byte in c.to_string().as_bytes() {
                    write!(&mut encoded, "%{:02X}", byte).unwrap();
                }
            }
        }
    }
    encoded
}

// ─── Knowledge Resource Endpoints ────────────────────────────

/// 获取入口文档
///
/// GET /api/v1/knowledge/entry
pub async fn get_entry(
    State(state): State<Arc<AppState>>,
    Query(params): Query<serde_json::Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo_path = state.knowledge_repo_path.as_ref()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, 
            "Knowledge repository path not configured".to_string()))?;

    // 根据 full 参数决定读取哪个文件
    let is_full = params.get("full")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let filename = if is_full {
        "入口.md"
    } else {
        "入口-快速启动.md"
    };

    let entry_path = PathBuf::from(repo_path).join(filename);
    let content = read_knowledge_file(&entry_path)?;
    
    Ok((
        StatusCode::OK,
        [("Content-Type", "text/markdown; charset=utf-8")],
        content,
    ))
}

/// 获取角色 RULES.md
///
/// GET /api/v1/knowledge/role/{name}
pub async fn get_role_rules(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo_path = state.knowledge_repo_path.as_ref()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, 
            "Knowledge repository path not configured".to_string()))?;

    let name_decoded = urlencoding_decode(&name);
    let rules_path = PathBuf::from(repo_path)
        .join("角色")
        .join(&name_decoded)
        .join("RULES.md");
    
    let content = read_knowledge_file(&rules_path)?;
    
    Ok((
        StatusCode::OK,
        [("Content-Type", "text/markdown; charset=utf-8")],
        content,
    ))
}

/// 获取角色热规则
///
/// GET /api/v1/knowledge/hot-rules/{role}
pub async fn get_role_hot_rules(
    State(state): State<Arc<AppState>>,
    Path(role): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo_path = state.knowledge_repo_path.as_ref()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, 
            "Knowledge repository path not configured".to_string()))?;

    let role_decoded = urlencoding_decode(&role);
    let hot_rules_path = PathBuf::from(repo_path)
        .join("角色")
        .join(&role_decoded)
        .join("hot-rules.md");
    
    let content = read_knowledge_file(&hot_rules_path)?;
    
    Ok((
        StatusCode::OK,
        [("Content-Type", "text/markdown; charset=utf-8")],
        content,
    ))
}

/// 获取项目 INDEX.md
///
/// GET /api/v1/knowledge/project/{name}
pub async fn get_project_index(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo_path = state.knowledge_repo_path.as_ref()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, 
            "Knowledge repository path not configured".to_string()))?;

    let name_decoded = urlencoding_decode(&name);
    
    // 尝试两个位置
    let index_paths = vec![
        PathBuf::from(repo_path).join("项目").join(&name_decoded).join("INDEX.md"),
        PathBuf::from(repo_path).join("项目文档").join(&name_decoded).join("INDEX.md"),
    ];

    let mut last_error = String::new();
    for index_path in index_paths {
        match read_knowledge_file(&index_path) {
            Ok(content) => {
                return Ok((
                    StatusCode::OK,
                    [("Content-Type", "text/markdown; charset=utf-8")],
                    content,
                ));
            }
            Err((_, e)) => {
                last_error = e;
            }
        }
    }
    
    Err((StatusCode::NOT_FOUND, format!("Project '{}' not found", name_decoded)))
}

/// 获取脚本内容
///
/// GET /api/v1/knowledge/script/{name}
pub async fn get_script(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo_path = state.knowledge_repo_path.as_ref()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, 
            "Knowledge repository path not configured".to_string()))?;

    let name_decoded = urlencoding_decode(&name);
    
    // 自动添加 .sh 后缀（如果需要）
    let script_name = if name_decoded.ends_with(".sh") {
        name_decoded
    } else {
        format!("{}.sh", name_decoded)
    };

    let script_path = PathBuf::from(repo_path)
        .join("scripts")
        .join(&script_name);
    
    let content = read_knowledge_file(&script_path)?;
    
    Ok((
        StatusCode::OK,
        [("Content-Type", "text/plain; charset=utf-8")],
        content,
    ))
}

/// 读取知识文件
fn read_knowledge_file(path: &PathBuf) -> Result<String, (StatusCode, String)> {
    std::fs::read_to_string(path)
        .map_err(|_| {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            (StatusCode::NOT_FOUND, format!("File '{}' not found", filename))
        })
}

/// URL 解码辅助函数（支持 UTF-8 多字节）
fn urlencoding_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '%' {
            let mut hex = String::new();
            for _ in 0..2 {
                if let Some(h) = chars.next() {
                    hex.push(h);
                }
            }
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                bytes.push(byte);
            }
        } else if c == '+' {
            bytes.push(b' ');
        } else {
            for b in c.to_string().as_bytes() {
                bytes.push(*b);
            }
        }
    }
    
    String::from_utf8(bytes).unwrap_or_default()
}

// ─── Knowledge Markdown Response for Read-Only Agents ────────

/// 知识 Markdown 格式响应（只读 Agent）
///
/// GET /api/v1/knowledge/markdown
///
/// 返回拼接的知识全文，适合元宝/豆包/ChatGPT 等只读 Agent。
pub async fn get_knowledge_markdown(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let repo_path = state.knowledge_repo_path.as_ref()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, 
            "Knowledge repository path not configured".to_string()))?;

    let mut markdown = String::new();
    
    // 添加入口文档
    markdown.push_str("# OpenClaw知识体系 — 快速启动\n\n");
    
    let entry_path = PathBuf::from(repo_path).join("入口-快速启动.md");
    if let Ok(content) = std::fs::read_to_string(&entry_path) {
        markdown.push_str(&content);
    }
    
    markdown.push_str("\n\n---\n\n## 角色\n\n");
    
    // 添加角色 RULES
    let roles_dir = PathBuf::from(repo_path).join("角色");
    if let Ok(entries) = std::fs::read_dir(roles_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let role_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                
                let rules_path = path.join("RULES.md");
                if let Ok(content) = std::fs::read_to_string(&rules_path) {
                    markdown.push_str(&format!("### {}\n\n", role_name));
                    markdown.push_str(&content);
                    markdown.push_str("\n\n---\n\n");
                }
            }
        }
    }
    
    markdown.push_str("## 项目\n\n");
    
    // 添加项目 INDEX
    let projects_dirs = vec![
        PathBuf::from(repo_path).join("项目"),
        PathBuf::from(repo_path).join("项目文档"),
    ];
    
    for projects_dir in projects_dirs {
        if let Ok(entries) = std::fs::read_dir(&projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let project_name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    
                    let index_path = path.join("INDEX.md");
                    if let Ok(content) = std::fs::read_to_string(&index_path) {
                        markdown.push_str(&format!("### {}\n\n", project_name));
                        markdown.push_str(&content);
                        markdown.push_str("\n\n---\n\n");
                    }
                }
            }
        }
    }
    
    Ok((
        StatusCode::OK,
        [("Content-Type", "text/markdown; charset=utf-8")],
        markdown,
    ))
}

// ─── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoding_encode("系统开发者"), "%E7%B3%BB%E7%BB%9F%E5%BC%80%E5%8F%91%E8%80%85");
        assert_eq!(urlencoding_encode("OpenLink"), "OpenLink");
        assert_eq!(urlencoding_encode("test-file.md"), "test-file.md");
    }

    #[test]
    fn test_url_decoding() {
        assert_eq!(urlencoding_decode("%E7%B3%BB%E7%BB%9F%E5%BC%80%E5%8F%91%E8%80%85"), "系统开发者");
        assert_eq!(urlencoding_decode("OpenLink"), "OpenLink");
        assert_eq!(urlencoding_decode("hello+world"), "hello world");
    }

    #[test]
    fn test_agent_type_default() {
        assert!(matches!(AgentType::default(), AgentType::Custom));
    }

    #[test]
    fn test_extract_description_from_rules() {
        // 这个测试需要创建临时文件
        let temp_dir = std::env::temp_dir();
        let rules_path = temp_dir.join("test_rules.md");
        
        std::fs::write(&rules_path, "# 系统开发者\n\n桥梁型角色——连接主人愿景与团队执行。\n\n## 职责\n\n- 架构设计").unwrap();
        
        let desc = extract_description_from_rules(&rules_path).unwrap();
        assert!(desc.contains("桥梁"));
        
        std::fs::remove_file(&rules_path).ok();
    }

    #[test]
    fn test_agent_discovery_response() {
        let response = AgentDiscoveryResponse {
            name: "OpenLink".to_string(),
            version: "0.2.0".to_string(),
            description: "Test".to_string(),
            capabilities: vec!["shortlink".to_string()],
            knowledge_join_url: "http://localhost/join".to_string(),
            endpoints: AgentEndpoints {
                join: "http://localhost/join".to_string(),
                entry: "http://localhost/entry".to_string(),
                roles: "http://localhost/roles".to_string(),
                projects: "http://localhost/projects".to_string(),
                scripts: "http://localhost/scripts".to_string(),
            },
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"name\":\"OpenLink\""));
        assert!(json.contains("knowledge_join_url"));
    }

    #[test]
    fn test_knowledge_package_structure() {
        let package = KnowledgePackage {
            version: "1.0".to_string(),
            system: "OpenClaw".to_string(),
            entry: EntryInfo {
                quick_start: "/entry".to_string(),
                full_guide: "/entry?full=true".to_string(),
            },
            roles: vec![RoleInfo {
                name: "系统开发者".to_string(),
                description: "Test".to_string(),
                rules_url: "/role/系统开发者".to_string(),
                hot_rules_url: Some("/hot-rules/系统开发者".to_string()),
            }],
            projects: vec![ProjectInfo {
                name: "OpenLink".to_string(),
                description: "Test".to_string(),
                index_url: "/project/OpenLink".to_string(),
            }],
            scripts: ScriptsInfo {
                act: "/script/act.sh".to_string(),
                handover: "/script/handover.sh".to_string(),
            },
            token: AgentToken {
                token: "agt_test".to_string(),
                expires_at: "2026-07-11T00:00:00Z".to_string(),
            },
        };
        
        let json = serde_json::to_string(&package).unwrap();
        assert!(json.contains("\"version\":\"1.0\""));
        assert!(json.contains("\"roles\""));
        assert!(json.contains("agt_test"));
    }
}
