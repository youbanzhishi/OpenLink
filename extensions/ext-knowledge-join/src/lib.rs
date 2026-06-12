//! # ext-knowledge-join — 知识体系接入 Action 扩展
//!
//! Phase 3 核心扩展，实现知识体系一键接入功能。
//!
//! 提供两个 Action：
//! - `knowledge_join`: 返回 JSON 知识包给全能 Agent
//! - `knowledge_serve`: 返回 Markdown 知识全文给只读 Agent
//!
//! 动态路由逻辑（根据访问者类型返回不同格式）：
//! - 只读Agent（Accept: text/markdown）→ Markdown 知识全文
//! - 全能Agent（User-Agent含bot/agent/curl）→ JSON 知识包
//! - 浏览器 → HTML 介绍页（通过 redirect 跳转）

use async_trait::async_trait;
use openlink_core::{ActionHandler, ActionResult, Context, CoreError, ExtensionRegistry, Target};
use std::sync::Arc;

/// 知识接入 Action Handler
///
/// 返回 JSON 格式的知识包给全能 Agent。
/// 适用于能 clone 仓库、跑命令的 Agent（如 Coze Agent）。
struct KnowledgeJoinHandler;

#[async_trait]
impl ActionHandler for KnowledgeJoinHandler {
    async fn execute(&self, ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        // 从 target.params 提取知识包信息
        let invite_code = target
            .params
            .get("invite_code")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        let repo_url = target.params.get("repo_url").and_then(|v| v.as_str()).map(String::from);

        let base_url = target
            .params
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:3000")
            .to_string();

        // 构建知识包
        let knowledge_package = serde_json::json!({
            "type": "knowledge_package",
            "version": "1.0",
            "system": "OpenClaw知识体系",
            "invite_code": invite_code,
            "entry": {
                "quick_start": format!("{}/api/v1/knowledge/entry", base_url),
                "full_guide": format!("{}/api/v1/knowledge/entry?full=true", base_url),
                "markdown": format!("{}/api/v1/knowledge/markdown", base_url),
            },
            "roles": {
                "list": format!("{}/api/v1/knowledge/roles", base_url),
                "base_path": "/api/v1/knowledge/role",
            },
            "projects": {
                "list": format!("{}/api/v1/knowledge/projects", base_url),
                "base_path": "/api/v1/knowledge/project",
            },
            "scripts": {
                "base_path": "/api/v1/knowledge/script",
                "act": format!("{}/api/v1/knowledge/script/act.sh", base_url),
                "handover": format!("{}/api/v1/knowledge/script/handover.sh", base_url),
            },
            "instructions": {
                "clone": if let Some(url) = repo_url.as_ref() {
                    format!("git clone {}", url)
                } else {
                    "Repository URL not provided".to_string()
                },
                "act_command": "bash scripts/act.sh \"意图\" . \"角色/角色名\"",
            }
        });

        tracing::debug!(
            identity_type = ?ctx.identity.identity_type,
            "Knowledge join: returning JSON package"
        );

        Ok(ActionResult::Json(knowledge_package))
    }

    fn name(&self) -> &str {
        "knowledge_join"
    }
}

/// 知识服务 Action Handler
///
/// 返回 Markdown 格式的知识全文给只读 Agent。
/// 适用于元宝/豆包/ChatGPT 等只读型 Agent。
struct KnowledgeServeHandler;

#[async_trait]
impl ActionHandler for KnowledgeServeHandler {
    async fn execute(&self, ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        // 从 target.params 提取配置
        let repo_path = target
            .params
            .get("repo_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CoreError::InvalidInput("Knowledge serve action requires 'repo_path' parameter".to_string())
            })?
            .to_string();

        let base_url = target
            .params
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:3000")
            .to_string();

        // 构建 Markdown 内容
        let markdown = build_knowledge_markdown(&repo_path, &base_url)?;

        let content_type = "text/markdown; charset=utf-8".to_string();

        tracing::debug!(
            identity_type = ?ctx.identity.identity_type,
            "Knowledge serve: returning Markdown content"
        );

        Ok(ActionResult::Custom {
            content_type,
            body: markdown,
        })
    }

    fn name(&self) -> &str {
        "knowledge_serve"
    }
}

/// 构建 Markdown 知识内容
fn build_knowledge_markdown(repo_path: &str, base_url: &str) -> Result<String, CoreError> {
    use std::path::PathBuf;

    let mut markdown = String::new();

    // 添加入口文档
    markdown.push_str("# OpenClaw知识体系 — 快速启动\n\n");
    markdown.push_str("> 通过短链加入知识体系，获取完整角色规则和项目知识。\n\n");

    // 添加入口文档
    let entry_path = PathBuf::from(repo_path).join("入口-快速启动.md");
    if let Ok(content) = std::fs::read_to_string(&entry_path) {
        markdown.push_str("---\n\n## 快速启动\n\n");
        markdown.push_str(&content);
        markdown.push('\n');
    }

    // 添加角色
    markdown.push_str("\n---\n\n## 角色规则\n\n");

    let roles_dir = PathBuf::from(repo_path).join("角色");
    if let Ok(entries) = std::fs::read_dir(&roles_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let role_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                let rules_path = path.join("RULES.md");
                if let Ok(content) = std::fs::read_to_string(&rules_path) {
                    markdown.push_str(&format!("### {}\n\n", role_name));
                    // 只取前 2000 字符避免过长
                    let preview = if content.len() > 2000 {
                        format!("{}\n\n_[... 内容已截断 ...]_\n", &content[..2000])
                    } else {
                        content
                    };
                    markdown.push_str(&preview);
                    markdown.push_str("\n\n---\n\n");
                }
            }
        }
    }

    // 添加项目
    markdown.push_str("\n## 项目索引\n\n");

    // 尝试两个项目目录
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
                        // 只取前 1500 字符
                        let preview = if content.len() > 1500 {
                            format!("{}\n\n_[... 内容已截断 ...]_\n", &content[..1500])
                        } else {
                            content
                        };
                        markdown.push_str(&preview);
                        markdown.push_str("\n\n---\n\n");
                    }
                }
            }
        }
    }

    // 添加使用说明
    markdown.push_str("\n## 使用说明\n\n");
    markdown.push_str("访问以下端点获取完整内容：\n\n");
    markdown.push_str(&format!("- 入口文档: {}/api/v1/knowledge/entry\n", base_url));
    markdown.push_str(&format!("- 角色RULES: {}/api/v1/knowledge/role/{{角色名}}\n", base_url));
    markdown.push_str(&format!(
        "- 项目INDEX: {}/api/v1/knowledge/project/{{项目名}}\n",
        base_url
    ));
    markdown.push_str(&format!(
        "- 脚本内容: {}/api/v1/knowledge/script/{{脚本名}}\n",
        base_url
    ));

    Ok(markdown)
}

/// 注册知识接入扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    registry.register_action(Arc::new(KnowledgeJoinHandler))?;
    registry.register_action(Arc::new(KnowledgeServeHandler))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::Action;
    use openlink_core::IdentityType;

    #[tokio::test]
    async fn test_knowledge_join_handler() {
        let handler = KnowledgeJoinHandler;
        let ctx = Context::from_request(Some("curl/7.88"), Some("127.0.0.1"));
        let target = Target {
            action: Action::Custom("knowledge_join".to_string()),
            params: serde_json::json!({
                "invite_code": "test-code",
                "base_url": "https://api.example.com",
            }),
        };

        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(data) => {
                assert_eq!(data["type"], "knowledge_package");
                assert_eq!(data["version"], "1.0");
                assert_eq!(data["invite_code"], "test-code");
                assert!(data["entry"]["quick_start"].is_string());
                assert!(data["roles"]["list"].is_string());
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_knowledge_join_handler_without_code() {
        let handler = KnowledgeJoinHandler;
        let ctx = Context::from_request(Some("curl/7.88"), Some("127.0.0.1"));
        let target = Target {
            action: Action::Custom("knowledge_join".to_string()),
            params: serde_json::json!({}),
        };

        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(data) => {
                assert_eq!(data["invite_code"], "default");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_register_knowledge_actions() {
        let mut registry = ExtensionRegistry::new();
        assert!(register(&mut registry).is_ok());
        assert!(registry.get_action_handler("knowledge_join").is_some());
        assert!(registry.get_action_handler("knowledge_serve").is_some());
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let mut registry = ExtensionRegistry::new();
        assert!(register(&mut registry).is_ok());
        assert!(register(&mut registry).is_err()); // 重复注册
    }

    #[tokio::test]
    async fn test_knowledge_serve_handler_without_repo_path() {
        let handler = KnowledgeServeHandler;
        let ctx = Context::from_request(Some("curl/7.88"), Some("127.0.0.1"));
        let target = Target {
            action: Action::Custom("knowledge_serve".to_string()),
            params: serde_json::json!({}),
        };

        let result = handler.execute(&ctx, &target).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_action_names() {
        let join_handler = KnowledgeJoinHandler;
        let serve_handler = KnowledgeServeHandler;

        assert_eq!(join_handler.name(), "knowledge_join");
        assert_eq!(serve_handler.name(), "knowledge_serve");
    }
}
