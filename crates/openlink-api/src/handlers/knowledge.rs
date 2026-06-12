//! # 知识体系 API Handlers — Phase 3 新增
//!
//! 知识体系一键接入 API：
//! - POST /api/v1/knowledge/join — 加入知识体系
//! - GET /api/v1/knowledge/entry — 入口文档
//! - GET /api/v1/knowledge/role/{name} — 角色 RULES.md
//! - GET /api/v1/knowledge/project/{name} — 项目 INDEX.md
//! - GET /api/v1/knowledge/script/{name} — 脚本内容
//! - GET /api/v1/knowledge/hot-rules/{role} — 角色热规则

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;

use crate::state::AppState;

// ─── Knowledge Join ────────────────────────────────────────

/// Agent 类型
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Llm,
    Robot,
    Service,
    #[default]
    Custom,
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
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Knowledge system is not enabled".to_string(),
        ));
    }

    if !state.config.knowledge.invite_codes.contains(&req.invite_code) {
        return Err((StatusCode::FORBIDDEN, "Invalid invite code".to_string()));
    }

    let base_url = &state.config.knowledge.base_url;
    let repo_path = (!state.config.knowledge.repo_path.is_empty())
        .then(|| &state.config.knowledge.repo_path)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Knowledge repository path not configured".to_string(),
            )
        })?;

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
    let roles = list_roles_from_repo(repo_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let projects =
        list_projects_from_repo(repo_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
        token: AgentToken { token, expires_at },
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
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

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
                rules_url: format!("/api/v1/knowledge/role/{}", urlencoding_encode(&name)),
                hot_rules_url: Some(format!("/api/v1/knowledge/hot-rules/{}", urlencoding_encode(&name))),
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
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

                // 检查是否有 INDEX.md
                let index_path = path.join("INDEX.md");
                if index_path.exists() {
                    projects.push(ProjectInfo {
                        name: name.clone(),
                        description: "".to_string(),
                        index_url: format!("/api/v1/knowledge/project/{}", urlencoding_encode(&name)),
                    });
                }
            }
        }
    }

    Ok(projects)
}

/// 从 RULES.md 提取描述（取第一段非空文字）
fn extract_description_from_rules(rules_path: &PathBuf) -> Result<String, String> {
    let content = std::fs::read_to_string(rules_path).map_err(|e| e.to_string())?;

    // 取第一行非空、非标题的内容作为描述
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('>') {
            // 截取前100字符（按char边界，避免UTF-8 panic）
            let desc = if trimmed.chars().count() > 100 {
                format!("{}...", trimmed.chars().take(100).collect::<String>())
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
    let repo_path = (!state.config.knowledge.repo_path.is_empty())
        .then(|| &state.config.knowledge.repo_path)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Knowledge repository path not configured".to_string(),
            )
        })?;

    // 根据 full 参数决定读取哪个文件
    let is_full = params.get("full").and_then(|v| v.as_bool()).unwrap_or(false);

    let filename = if is_full { "入口.md" } else { "入口-快速启动.md" };

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
    let repo_path = (!state.config.knowledge.repo_path.is_empty())
        .then(|| &state.config.knowledge.repo_path)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Knowledge repository path not configured".to_string(),
            )
        })?;

    // Axum Path 已自动做 URL 解码
    let name_decoded = name;
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
    let repo_path = (!state.config.knowledge.repo_path.is_empty())
        .then(|| &state.config.knowledge.repo_path)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Knowledge repository path not configured".to_string(),
            )
        })?;

    // Axum Path 已自动做 URL 解码
    let role_decoded = role;
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
    let repo_path = (!state.config.knowledge.repo_path.is_empty())
        .then(|| &state.config.knowledge.repo_path)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Knowledge repository path not configured".to_string(),
            )
        })?;

    // Axum Path 已自动做 URL 解码
    let name_decoded = name;

    // 尝试两个位置
    let index_paths = vec![
        PathBuf::from(repo_path)
            .join("项目")
            .join(&name_decoded)
            .join("INDEX.md"),
        PathBuf::from(repo_path)
            .join("项目文档")
            .join(&name_decoded)
            .join("INDEX.md"),
    ];

    #[allow(unused_assignments)]
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
    let repo_path = (!state.config.knowledge.repo_path.is_empty())
        .then(|| &state.config.knowledge.repo_path)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Knowledge repository path not configured".to_string(),
            )
        })?;

    // Axum Path 已自动做 URL 解码
    let name_decoded = name;

    // 自动添加 .sh 后缀（如果需要）
    let script_name = if name_decoded.ends_with(".sh") {
        name_decoded
    } else {
        format!("{}.sh", name_decoded)
    };

    let script_path = PathBuf::from(repo_path).join("scripts").join(&script_name);

    let content = read_knowledge_file(&script_path)?;

    Ok((StatusCode::OK, [("Content-Type", "text/plain; charset=utf-8")], content))
}

/// 读取知识文件
fn read_knowledge_file(path: &PathBuf) -> Result<String, (StatusCode, String)> {
    std::fs::read_to_string(path).map_err(|_| {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        (StatusCode::NOT_FOUND, format!("File '{}' not found", filename))
    })
}

/// URL 解码辅助函数（支持 UTF-8 多字节）
#[allow(dead_code)]
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
    let repo_path = (!state.config.knowledge.repo_path.is_empty())
        .then(|| &state.config.knowledge.repo_path)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Knowledge repository path not configured".to_string(),
            )
        })?;

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
                let role_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

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
                    let project_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

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
        assert_eq!(
            urlencoding_encode("系统开发者"),
            "%E7%B3%BB%E7%BB%9F%E5%BC%80%E5%8F%91%E8%80%85"
        );
        assert_eq!(urlencoding_encode("OpenLink"), "OpenLink");
        assert_eq!(urlencoding_encode("test-file.md"), "test-file.md");
    }

    #[test]
    fn test_url_decoding() {
        assert_eq!(
            urlencoding_decode("%E7%B3%BB%E7%BB%9F%E5%BC%80%E5%8F%91%E8%80%85"),
            "系统开发者"
        );
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

        std::fs::write(
            &rules_path,
            "# 系统开发者\n\n桥梁型角色——连接主人愿景与团队执行。\n\n## 职责\n\n- 架构设计",
        )
        .unwrap();

        let desc = extract_description_from_rules(&rules_path).unwrap();
        assert!(desc.contains("桥梁"));

        std::fs::remove_file(&rules_path).ok();
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

// ─── 一条短链入口 ────────────────────────────────────────

/// GET /join?code=xxx — 一条短链，智能体访问即可加入知识体系
///
/// 根据访问者类型自动返回最合适的格式：
/// - 只读型智能体（元宝/豆包/ChatGPT）→ 纯 Markdown 文本
/// - 全能型智能体（Coze/自建Agent）→ JSON（含角色/项目/脚本URL + token）
/// - 浏览器 → HTML 引导页
pub async fn knowledge_short_entry(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<ShortEntryParams>,
) -> Result<Response, (StatusCode, String)> {
    if state.config.knowledge.repo_path.is_empty() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Knowledge system not enabled".to_string(),
        ));
    }
    let repo_path = state.config.knowledge.repo_path.clone();

    // 验证邀请码
    // 验证邀请码
    let code = params.code.clone().unwrap_or_default();
    if !state.config.knowledge.invite_codes.contains(&code) {
        return Err((
            StatusCode::FORBIDDEN,
            "无效邀请码。请在 URL 中加 ?code=你的邀请码".to_string(),
        ));
    }

    let agent_type = detect_agent_type(&headers, &params.agent);

    match agent_type {
        AgentCategory::ReadOnly => {
            // 只读智能体：返回精简 Markdown（入口+目录+URL，按需取，不塞全量）
            let base_url = &state.config.knowledge.base_url;
            let markdown = build_lightweight_markdown(&repo_path, base_url)?;
            Ok((
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
                markdown,
            )
                .into_response())
        }
        AgentCategory::FullCapability => {
            // 全能型智能体：返回结构化 JSON
            let base_url = &state.config.knowledge.base_url;
            let pkg = build_knowledge_package(&repo_path, base_url)?;
            Ok(Json(pkg).into_response())
        }
        AgentCategory::Browser => {
            let base = &state.config.knowledge.base_url;
            let html = format!(
                r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>OpenClaw 知识体系 — 加入</title>
<style>
  body {{ font-family: -apple-system,BlinkMacSystemFont,sans-serif; max-width:680px; margin:60px auto; padding:0 20px; color:#1a1a1a; line-height:1.7; }}
  h1 {{ color:#3b82f6; }} .card {{ background:#f8fafc; border:1px solid #e2e8f0; border-radius:12px; padding:20px 24px; margin:16px 0; }}
  code {{ background:#f1f5f9; padding:2px 6px; border-radius:4px; font-size:14px; }}
  .tip {{ color:#64748b; font-size:14px; }}
</style></head><body>
<h1>🐉 OpenClaw 知识体系</h1>
<p>你已通过邀请码验证，接下来根据你的身份选择加入方式：</p>
<div class="card">
  <h3>🤖 我是 AI 智能体</h3>
  <p>直接访问以下地址即可获取知识（会自动识别你的类型）：</p>
  <p><code>{base}/join?code={code}</code></p>
  <p class="tip">全能型 Agent 会收到 JSON + Token，只读型 Agent 会收到 Markdown 文本。</p>
</div>
<div class="card">
  <h3>👨‍💻 我是开发者</h3>
  <p>使用 curl 测试：</p>
  <p><code>curl {base}/join?code={code}</code></p>
  <p>指定返回格式：<code>curl -H "Accept: application/json" {base}/join?code={code}</code></p>
</div>
</body></html>"#
            );
            Ok((
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                html,
            )
                .into_response())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ShortEntryParams {
    pub code: Option<String>,
    pub agent: Option<String>,
}

enum AgentCategory {
    ReadOnly,
    FullCapability,
    Browser,
}

fn detect_agent_type(headers: &axum::http::HeaderMap, agent_param: &Option<String>) -> AgentCategory {
    // 1. 查询参数优先
    if let Some(a) = agent_param {
        match a.to_lowercase().as_str() {
            "readonly" | "llm" => return AgentCategory::ReadOnly,
            "full" | "agent" => return AgentCategory::FullCapability,
            "browser" => return AgentCategory::Browser,
            _ => {}
        }
    }

    // 2. Accept 头判断
    if let Some(accept) = headers.get("accept") {
        let accept_str = accept.to_str().unwrap_or("").to_lowercase();
        if accept_str.contains("text/markdown") || accept_str.contains("text/plain") {
            return AgentCategory::ReadOnly;
        }
        if accept_str.contains("application/json") && !accept_str.contains("text/html") {
            return AgentCategory::FullCapability;
        }
        if accept_str.contains("text/html") {
            return AgentCategory::Browser;
        }
    }

    // 3. User-Agent 猜测
    if let Some(ua) = headers.get("user-agent") {
        let ua_str = ua.to_str().unwrap_or("").to_lowercase();
        let readonly_agents = ["yuanbao", "doubao", "chatgpt", "claude", "perplexity", "bingbot"];
        for agent in readonly_agents {
            if ua_str.contains(agent) {
                return AgentCategory::ReadOnly;
            }
        }
        let full_agents = ["coze", "openai-python", "python-requests", "curl"];
        for agent in full_agents {
            if ua_str.contains(agent) {
                return AgentCategory::FullCapability;
            }
        }
        if ua_str.contains("mozilla") || ua_str.contains("safari") || ua_str.contains("chrome") {
            return AgentCategory::Browser;
        }
    }

    // 默认：只读（最安全）
    AgentCategory::ReadOnly
}

/// 构建精简知识 Markdown（入口文档+目录+URL，让只读智能体按需取）
/// 约3-5KB，对比全量446KB，响应快100倍
fn build_lightweight_markdown(repo_path: &str, base_url: &str) -> Result<String, (StatusCode, String)> {
    let mut md = String::new();
    md.push_str("# OpenClaw 知识体系\n\n");

    // 入口文档
    let entry_path = PathBuf::from(repo_path).join("入口-快速启动.md");
    if let Ok(content) = std::fs::read_to_string(&entry_path) {
        md.push_str(&content);
        md.push_str("\n\n---\n\n");
    }

    // 角色目录
    md.push_str("## 可用角色\n\n");
    md.push_str("访问对应 URL 获取角色完整规则：\n\n");
    let roles_dir = PathBuf::from(repo_path).join("角色");
    if let Ok(entries) = std::fs::read_dir(roles_dir) {
        let mut roles: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let desc = extract_description_from_rules(&path.join("RULES.md")).unwrap_or_default();
                    let url = format!("{}/api/v1/knowledge/role/{}", base_url, urlencoding_encode(name));
                    roles.push(format!("- **{}**：{} [→完整规则]({})", name, desc, url));
                }
            }
        }
        roles.sort();
        for r in roles {
            md.push_str(&r);
            md.push('\n');
        }
    }

    // 项目目录（去重）
    md.push_str("\n## 可用项目\n\n");
    md.push_str("访问对应 URL 获取项目知识索引：\n\n");
    let projects_dirs = vec![
        PathBuf::from(repo_path).join("项目"),
        PathBuf::from(repo_path).join("项目文档"),
    ];
    let mut seen = std::collections::HashSet::new();
    for pdir in &projects_dirs {
        if let Ok(entries) = std::fs::read_dir(pdir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if seen.insert(name.to_string()) {
                            let desc = extract_description_from_index(&path.join("INDEX.md")).unwrap_or_default();
                            let url = format!("{}/api/v1/knowledge/project/{}", base_url, urlencoding_encode(name));
                            md.push_str(&format!("- **{}**：{} [→项目索引]({})\n", name, desc, url));
                        }
                    }
                }
            }
        }
    }

    md.push_str(&format!(
        "\n---\n\n> 💡 需要全量知识？访问 {}/api/v1/knowledge/markdown\n",
        base_url
    ));
    Ok(md)
}

/// 从 INDEX.md 提取项目描述（首行非空非标题）
fn extract_description_from_index(path: &PathBuf) -> Result<String, ()> {
    let content = std::fs::read_to_string(path).map_err(|_| ())?;
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('>') {
            let desc = if trimmed.chars().count() > 80 {
                format!("{}...", trimmed.chars().take(80).collect::<String>())
            } else {
                trimmed.to_string()
            };
            return Ok(desc);
        }
    }
    Ok(String::new())
}

/// 构建完整知识 Markdown（提取为独立函数供短链入口复用）
#[allow(dead_code)]
fn build_full_markdown(repo_path: &str) -> Result<String, (StatusCode, String)> {
    let mut markdown = String::new();

    markdown.push_str("# OpenClaw知识体系 — 快速启动\n\n");

    let entry_path = PathBuf::from(repo_path).join("入口-快速启动.md");
    if let Ok(content) = std::fs::read_to_string(&entry_path) {
        markdown.push_str(&content);
    }

    markdown.push_str("\n\n---\n\n## 角色\n\n");

    let roles_dir = PathBuf::from(repo_path).join("角色");
    if let Ok(entries) = std::fs::read_dir(roles_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let role_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
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

    let projects_dirs = vec![
        PathBuf::from(repo_path).join("项目"),
        PathBuf::from(repo_path).join("项目文档"),
    ];

    for projects_dir in projects_dirs {
        if let Ok(entries) = std::fs::read_dir(&projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let project_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
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

    Ok(markdown)
}

// ─── Knowledge Sync ────────────────────────────────────────
// 推送后自动同步知识仓库：push.sh → curl通知ECS → git pull

/// POST /api/v1/knowledge/sync
/// 推送后通知ECS拉最新知识仓库代码
/// 认证：Bearer token，与配置中 sync_token 匹配
pub async fn sync_knowledge(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let config = &state.config.knowledge;

    if !config.enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "knowledge module not enabled"
            })),
        )
            .into_response();
    }

    // 认证：sync_token为空则不验证
    if !config.sync_token.is_empty() {
        let auth_ok = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.strip_prefix("Bearer ").unwrap_or(v) == config.sync_token)
            .unwrap_or(false);

        if !auth_ok {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "invalid or missing sync token"
                })),
            )
                .into_response();
        }
    }

    if config.repo_path.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "repo_path not configured"
            })),
        )
            .into_response();
    }

    // 执行 git pull --ff-only origin master
    let output = Command::new("git")
        .args(["-C", &config.repo_path, "pull", "--ff-only", "origin", "master"])
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let already_up_to_date = stdout.contains("Already up to date") || stderr.contains("Already up to date");

            // 获取当前commit hash
            let commit_output = Command::new("git")
                .args(["-C", &config.repo_path, "rev-parse", "--short", "HEAD"])
                .output()
                .await;

            let commit = commit_output
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8_lossy(&o.stdout).trim().to_string().into()
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());

            if out.status.success() {
                let message = if already_up_to_date {
                    "already up to date".to_string()
                } else {
                    stdout.trim().to_string()
                };

                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "synced": true,
                        "commit": commit,
                        "already_up_to_date": already_up_to_date,
                        "message": message,
                    })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "synced": false,
                        "commit": commit,
                        "error": stderr.trim(),
                    })),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "synced": false,
                "error": format!("failed to execute git: {}", e),
            })),
        )
            .into_response(),
    }
}
