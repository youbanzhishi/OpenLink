//! # Identity Card 名片处理器
//!
//! Phase 3.5: 让每个 Link 变成一张智能名片
//! - 浏览器访问看到精美 HTML 名片
//! - AI Agent 访问看到 JSON-LD 结构化身份数据
//!
//! 名片 = type=identity_card 的 Link，卡片数据存在 payload.card 字段

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use openlink_core::{shortcode, AccessLog, Action, Condition, ConditionLogic, Context, Link, Route, Rule, Target};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

// ─── 社交平台 URL 映射 ─────────────────────────────────────

/// 社交平台 URL 映射表
static SOCIAL_URL_MAP: &[(&str, &str)] = &[
    ("github", "https://github.com/{handle}"),
    ("掘金", "https://juejin.cn/user/{handle}"),
    ("小红书", "https://www.xiaohongshu.com/user/profile/{handle}"),
    ("知乎", "https://www.zhihu.com/people/{handle}"),
    ("twitter", "https://twitter.com/{handle}"),
    ("微博", "https://weibo.com/{handle}"),
    ("bilibili", "https://space.bilibili.com/{handle}"),
    ("youtube", "https://youtube.com/@{handle}"),
    ("linkedin", "https://linkedin.com/in/{handle}"),
    ("email", "mailto:{handle}"),
];

/// 社交平台 SVG 图标映射
fn get_social_icon(platform: &str) -> &'static str {
    match platform {
        "github" => "<svg viewBox=\"0 0 24 24\"><path d=\"M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z\"/></svg>",
        "掘金" => "<svg viewBox=\"0 0 24 24\"><path d=\"M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>",
        "小红书" => "<svg viewBox=\"0 0 24 24\"><path d=\"M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z\"/></svg>",
        "知乎" => "<svg viewBox=\"0 0 24 24\"><path d=\"M5.721 0C2.251 0 0 2.25 0 5.719V18.28C0 21.751 2.252 24 5.721 24h12.56C21.751 24 24 21.75 24 18.281V5.72C24 2.249 21.75 0 18.281 0zm1.964 4.078h6.191c.14-.017.265.084.282.224v3.96a.253.253 0 01-.224.282H7.685a.253.253 0 01-.282-.224V4.36a.253.253 0 01.224-.282h.058zm9.592 15.703H6.03l2.389-3.96H7.12l3.266-5.416h2.69l-2.614 4.335h2.843z\"/></svg>",
        "twitter" => "<svg viewBox=\"0 0 24 24\"><path d=\"M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z\"/></svg>",
        "微博" => "<svg viewBox=\"0 0 24 24\"><path d=\"M10.098 20.323c-3.977.391-7.414-1.406-7.672-4.02-.259-2.609 2.759-5.047 6.74-5.441 3.979-.394 7.413 1.404 7.671 4.018.259 2.6-2.759 5.049-6.737 5.439l-.002.004zM16.26 9.3c-.391-.122-.66-.205-.456-.739.446-1.17.492-2.179.004-2.901-.904-1.339-3.38-1.266-6.218-.038 0 0-.892.378-.663-.306.436-1.367.371-2.512-.252-3.171C7.026.819 3.825 2.392 1.525 5.514-1.219 9.262-1.273 14.062.671 17.076c2.697 4.18 8.765 5.327 14.461 3.288 6.047-2.162 9.134-7.689 6.686-11.445-1.19-1.825-3.33-2.816-5.559-2.619z\"/></svg>",
        "bilibili" => "<svg viewBox=\"0 0 24 24\"><path d=\"M17.813 4.653h.854c1.51.054 2.769.578 3.773 1.574 1.004.995 1.524 2.249 1.56 3.76v7.36c-.036 1.51-.556 2.769-1.56 3.773s-2.262 1.524-3.773 1.56H5.333c-1.51-.036-2.769-.556-3.773-1.56S.036 18.858 0 17.347v-7.36c.036-1.511.556-2.765 1.56-3.76 1.004-.996 2.262-1.52 3.773-1.574h.774l-1.174-1.12a1.234 1.234 0 01-.373-.906c0-.356.124-.658.373-.907l.027-.027c.267-.249.573-.373.92-.373.347 0 .653.124.92.373L9.653 4.44c.071.071.134.142.187.213h4.267a.836.836 0 01.16-.213l2.853-2.747c.267-.249.573-.373.92-.373.347 0 .662.151.929.4.267.249.391.551.391.907 0 .355-.124.657-.373.906zM5.333 7.24c-.746.018-1.373.276-1.88.773-.506.498-.769 1.13-.786 1.894v7.52c.017.764.28 1.395.786 1.893.507.498 1.134.756 1.88.773h13.334c.746-.017 1.373-.275 1.88-.773.506-.498.769-1.129.786-1.893v-7.52c-.017-.765-.28-1.396-.786-1.894-.507-.497-1.134-.755-1.88-.773zM8 11.107c.373 0 .684.124.933.373.25.249.383.569.4.96v1.173c-.017.391-.15.711-.4.96-.249.25-.56.374-.933.374s-.684-.125-.933-.374c-.25-.249-.383-.569-.4-.96V12.44c0-.373.129-.689.386-.947.258-.257.574-.386.947-.386zm8 0c.373 0 .684.124.933.373.25.249.383.569.4.96v1.173c-.017.391-.15.711-.4.96-.249.25-.56.374-.933.374s-.684-.125-.933-.374c-.25-.249-.383-.569-.4-.96V12.44c.017-.391.15-.711.4-.96.249-.249.56-.373.933-.373z\"/></svg>",
        "youtube" => "<svg viewBox=\"0 0 24 24\"><path d=\"M23.498 6.186a3.016 3.016 0 00-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 00.502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 002.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 002.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z\"/></svg>",
        "linkedin" => "<svg viewBox=\"0 0 24 24\"><path d=\"M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433c-1.144 0-2.063-.926-2.063-2.065 0-1.138.92-2.063 2.063-2.063 1.14 0 2.064.925 2.064 2.063 0 1.139-.925 2.065-2.064 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z\"/></svg>",
        "email" => "<svg viewBox=\"0 0 24 24\"><path d=\"M20 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 4l-8 5-8-5V6l8 5 8-5v2z\"/></svg>",
        _ => "<svg viewBox=\"0 0 24 24\"><path d=\"M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z\"/></svg>",
    }
}

// ─── 请求/响应模型 ──────────────────────────────────────────

/// 创建名片请求
#[derive(Debug, Deserialize)]
pub struct CreateCardRequest {
    /// 名片短码（不填则自动生成）
    pub code: Option<String>,
    /// 显示名称
    pub display_name: String,
    /// 个人简介
    #[serde(default)]
    pub bio: String,
    /// 头像 URL
    #[serde(default)]
    pub avatar: String,
    /// 社交平台链接 { "github": "handle", "掘金": "handle" }
    #[serde(default)]
    pub social: serde_json::Value,
    /// 项目列表
    #[serde(default)]
    pub projects: Vec<String>,
    /// 标签列表
    #[serde(default)]
    pub tags: Vec<String>,
    /// 主题: dark / light / minimal / gradient
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "dark".to_string()
}

/// 名片响应
#[derive(Debug, Serialize)]
pub struct CardResponse {
    pub id: String,
    pub code: String,
    pub card: serde_json::Value,
    pub created_at: String,
    pub is_active: bool,
}

/// 更新名片请求
#[derive(Debug, Deserialize)]
pub struct UpdateCardRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<String>,
    pub social: Option<serde_json::Value>,
    pub projects: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub theme: Option<String>,
}

// ─── Card CRUD API ──────────────────────────────────────────

/// 创建名片
///
/// POST /api/v1/cards
/// 底层创建 Link（type=identity_card）+ 两条路由规则
pub async fn create_card(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateCardRequest>,
) -> Result<(StatusCode, Json<CardResponse>), (StatusCode, String)> {
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

    // 构建 card 数据
    let card_data = serde_json::json!({
        "display_name": req.display_name,
        "bio": req.bio,
        "avatar": req.avatar,
        "social": req.social,
        "projects": req.projects,
        "tags": req.tags,
        "theme": req.theme,
    });

    // 构建 Link payload
    let link_payload = serde_json::json!({
        "type": "identity_card",
        "card": card_data,
    });

    let link_id = uuid::Uuid::new_v4().to_string();
    let link = Link {
        id: link_id.clone(),
        code: code.clone(),
        payload: link_payload,
        owner: "default".to_string(),
        created_at: chrono::Utc::now(),
        metadata: serde_json::Value::Null,
        is_active: true,
    };

    let created = state.store.create_link(&link).await.map_err(|e| match e {
        openlink_store::StoreError::Duplicate(_) => (StatusCode::CONFLICT, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    // 自动创建路由规则：Agent → JSON-LD，Human → 渲染名片
    let route = Route {
        id: uuid::Uuid::new_v4().to_string(),
        link_id: link_id.clone(),
        rules: vec![
            // Agent 访问 → 返回 JSON-LD
            Rule {
                condition: Condition {
                    condition_type: "identity-type".to_string(),
                    params: serde_json::json!({"type": "agent"}),
                },
                conditions: vec![],
                condition_logic: ConditionLogic::And,
                target: Target {
                    action: Action::JsonData,
                    params: serde_json::json!({"format": "json-ld"}),
                },
                priority: 10,
            },
            // Service 访问（curl等）→ 返回 JSON-LD
            Rule {
                condition: Condition {
                    condition_type: "identity-type".to_string(),
                    params: serde_json::json!({"type": "service"}),
                },
                conditions: vec![],
                condition_logic: ConditionLogic::And,
                target: Target {
                    action: Action::JsonData,
                    params: serde_json::json!({"format": "json-ld"}),
                },
                priority: 9,
            },
        ],
        // 兜底：Human 访问 → 渲染 HTML 名片
        default_target: Target {
            action: Action::Custom("render-card".to_string()),
            params: serde_json::json!({"format": "html"}),
        },
        version: 1,
        created_at: chrono::Utc::now(),
    };

    // 路由创建失败不影响名片创建，仅记录日志
    if let Err(e) = state.store.create_route(&route).await {
        tracing::warn!(link_id = %link_id, error = %e, "Failed to create auto-route for card");
    }

    tracing::info!(code = %code, "Identity card created");

    Ok((
        StatusCode::CREATED,
        Json(CardResponse {
            id: created.id,
            code: created.code,
            card: card_data,
            created_at: created.created_at.to_rfc3339(),
            is_active: created.is_active,
        }),
    ))
}

/// 查询名片
///
/// GET /api/v1/cards/:code
pub async fn get_card(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<CardResponse>, (StatusCode, String)> {
    let link = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Card '{}' not found", code)))?;

    // 验证是名片类型
    let payload_type = link.payload.get("type").and_then(|v| v.as_str());
    if payload_type != Some("identity_card") {
        return Err((StatusCode::NOT_FOUND, format!("'{}' is not an identity card", code)));
    }

    let card_data = link.payload.get("card").cloned().unwrap_or_default();

    Ok(Json(CardResponse {
        id: link.id,
        code: link.code,
        card: card_data,
        created_at: link.created_at.to_rfc3339(),
        is_active: link.is_active,
    }))
}

/// 更新名片
///
/// PUT /api/v1/cards/:code
pub async fn update_card(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Json(req): Json<UpdateCardRequest>,
) -> Result<Json<CardResponse>, (StatusCode, String)> {
    let existing = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Card '{}' not found", code)))?;

    // 验证是名片类型
    let payload_type = existing.payload.get("type").and_then(|v| v.as_str());
    if payload_type != Some("identity_card") {
        return Err((StatusCode::NOT_FOUND, format!("'{}' is not an identity card", code)));
    }

    // 合并更新 card 数据
    let mut card_data = existing.payload.get("card").cloned().unwrap_or_default();

    if let Some(display_name) = req.display_name {
        card_data["display_name"] = serde_json::Value::String(display_name);
    }
    if let Some(bio) = req.bio {
        card_data["bio"] = serde_json::Value::String(bio);
    }
    if let Some(avatar) = req.avatar {
        card_data["avatar"] = serde_json::Value::String(avatar);
    }
    if let Some(social) = req.social {
        card_data["social"] = social;
    }
    if let Some(projects) = req.projects {
        card_data["projects"] = serde_json::to_value(projects).unwrap_or_default();
    }
    if let Some(tags) = req.tags {
        card_data["tags"] = serde_json::to_value(tags).unwrap_or_default();
    }
    if let Some(theme) = req.theme {
        card_data["theme"] = serde_json::Value::String(theme);
    }

    // 更新 Link
    let mut payload = existing.payload.clone();
    payload["card"] = card_data.clone();

    let updated_link = Link {
        id: existing.id.clone(),
        code: existing.code.clone(),
        payload,
        owner: existing.owner,
        created_at: existing.created_at,
        metadata: existing.metadata,
        is_active: existing.is_active,
    };

    let updated = state
        .store
        .update_link(&updated_link)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(code = %code, "Identity card updated");

    Ok(Json(CardResponse {
        id: updated.id,
        code: updated.code,
        card: card_data,
        created_at: updated.created_at.to_rfc3339(),
        is_active: updated.is_active,
    }))
}

/// 删除名片
///
/// DELETE /api/v1/cards/:code
pub async fn delete_card(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let link = state
        .store
        .get_link_by_code(&code)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Card '{}' not found", code)))?;

    // 验证是名片类型
    let payload_type = link.payload.get("type").and_then(|v| v.as_str());
    if payload_type != Some("identity_card") {
        return Err((StatusCode::NOT_FOUND, format!("'{}' is not an identity card", code)));
    }

    // 删除关联的路由
    if let Ok(Some(route)) = state.store.get_route_by_link_id(&link.id).await {
        if let Err(e) = state.store.delete_route(&route.id).await {
            tracing::warn!(route_id = %route.id, error = %e, "Failed to delete card route");
        }
    }

    // 删除 Link
    state.store.delete_link(&link.id).await.map_err(|e| match e {
        openlink_store::StoreError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    tracing::info!(code = %code, "Identity card deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// 列出所有名片
///
/// GET /api/v1/cards
pub async fn list_cards(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let links = state
        .store
        .list_links(None, 100)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 过滤出 identity_card 类型的 Link
    let cards: Vec<CardResponse> = links
        .into_iter()
        .filter(|l| l.payload.get("type").and_then(|v| v.as_str()) == Some("identity_card"))
        .map(|l| {
            let card_data = l.payload.get("card").cloned().unwrap_or_default();
            CardResponse {
                id: l.id,
                code: l.code,
                card: card_data,
                created_at: l.created_at.to_rfc3339(),
                is_active: l.is_active,
            }
        })
        .collect();

    let total = cards.len();
    Ok(Json(serde_json::json!({
        "cards": cards,
        "total": total,
    })))
}

// ─── Card 渲染 ──────────────────────────────────────────────

/// 渲染名片 — 核心渲染路径
///
/// GET /card/:code
/// - 检查请求 Context（User-Agent / Accept header）
/// - 人类浏览器访问 → 返回 HTML 名片页面
/// - curl/AI Agent 访问 → 返回 JSON-LD 结构化身份数据
/// - Accept: application/ld+json → 强制返回 JSON-LD
/// - Accept: text/html 或浏览器 UA → 返回 HTML
pub async fn render_card(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // 查找 Link
    let link = match state.store.get_link_by_code(&code).await {
        Ok(Some(link)) => link,
        Ok(None) => return (StatusCode::NOT_FOUND, "Card not found").into_response(),
        Err(e) => {
            tracing::error!(code = %code, error = %e, "Failed to lookup card");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !link.is_active {
        return (StatusCode::GONE, "Card is inactive").into_response();
    }

    // 验证是名片类型
    let payload_type = link.payload.get("type").and_then(|v| v.as_str());
    if payload_type != Some("identity_card") {
        return (StatusCode::NOT_FOUND, "Not an identity card").into_response();
    }

    let card_data = link.payload.get("card").cloned().unwrap_or_default();

    // 判断返回类型：Accept header 优先，然后看 User-Agent
    let accept = headers.get("accept").and_then(|v| v.to_str().ok()).unwrap_or("");
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or("");

    let wants_json_ld = accept.contains("application/ld+json") || accept.contains("application/json");
    let wants_html = accept.contains("text/html");
    let is_browser_ua =
        user_agent.contains("Mozilla/") && !user_agent.contains("curl/") && !user_agent.contains("wget/");

    // 记录访问日志
    let _ = log_card_access(
        &state,
        &link,
        &headers,
        "render_card",
        start.elapsed().as_millis() as i64,
    )
    .await;

    if wants_json_ld || (!wants_html && !is_browser_ua) {
        // 返回 JSON-LD
        let json_ld = build_json_ld(&code, &card_data);
        (
            StatusCode::OK,
            [("content-type", "application/ld+json; charset=utf-8")],
            json_ld,
        )
            .into_response()
    } else {
        // 返回 HTML 名片
        let html = render_card_html(&code, &card_data);
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }
}

// ─── 社交链接路由 ────────────────────────────────────────────

/// 社交链接跳转
///
/// GET /card/:code/:platform
/// 如 /card/xiaolong/github → 302 跳转到 GitHub 主页
pub async fn card_social_redirect(
    State(state): State<Arc<AppState>>,
    Path((code, platform)): Path<(String, String)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // 查找 Link
    let link = match state.store.get_link_by_code(&code).await {
        Ok(Some(link)) => link,
        Ok(None) => return (StatusCode::NOT_FOUND, "Card not found").into_response(),
        Err(e) => {
            tracing::error!(code = %code, error = %e, "Failed to lookup card");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // 验证是名片类型
    let payload_type = link.payload.get("type").and_then(|v| v.as_str());
    if payload_type != Some("identity_card") {
        return (StatusCode::NOT_FOUND, "Not an identity card").into_response();
    }

    let card_data = link.payload.get("card").cloned().unwrap_or_default();
    let social = card_data.get("social").cloned().unwrap_or_default();

    // 查找社交平台 handle
    let handle = social.get(&platform).and_then(|v| v.as_str()).unwrap_or("");

    if handle.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            format!("Social platform '{}' not found on this card", platform),
        )
            .into_response();
    }

    // 查找 URL 模板并替换
    let target_url = SOCIAL_URL_MAP
        .iter()
        .find(|(name, _)| *name == platform)
        .map(|(_, template)| template.replace("{handle}", handle))
        .unwrap_or_else(|| format!("https://{}.com/{}", platform, handle));

    // 记录访问日志（带平台信息）
    let _ = log_card_access(
        &state,
        &link,
        &headers,
        &format!("social_redirect:{}", platform),
        start.elapsed().as_millis() as i64,
    )
    .await;

    tracing::info!(code = %code, platform = %platform, target = %target_url, "Social redirect");

    axum::response::Redirect::temporary(&target_url).into_response()
}

// ─── QR Code ────────────────────────────────────────────────

/// 名片二维码
///
/// GET /card/:code/qr
/// 返回 SVG 格式二维码
pub async fn card_qr(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // 查找 Link
    let link = match state.store.get_link_by_code(&code).await {
        Ok(Some(link)) => link,
        Ok(None) => return (StatusCode::NOT_FOUND, "Card not found").into_response(),
        Err(e) => {
            tracing::error!(code = %code, error = %e, "Failed to lookup card");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // 验证是名片类型
    let payload_type = link.payload.get("type").and_then(|v| v.as_str());
    if payload_type != Some("identity_card") {
        return (StatusCode::NOT_FOUND, "Not an identity card").into_response();
    }

    // 构建名片 URL
    let card_url = format!("/card/{}", code);

    // 生成 QR Code SVG
    let svg = generate_qr_svg(&card_url);

    // 记录访问日志
    let _ = log_card_access(&state, &link, &headers, "qr_code", start.elapsed().as_millis() as i64).await;

    (StatusCode::OK, [("content-type", "image/svg+xml")], svg).into_response()
}

// ─── 辅助函数 ───────────────────────────────────────────────

/// 构建 JSON-LD 结构化数据（Schema.org Person）
fn build_json_ld(code: &str, card: &serde_json::Value) -> String {
    let display_name = card.get("display_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
    let bio = card.get("bio").and_then(|v| v.as_str()).unwrap_or("");
    let avatar = card.get("avatar").and_then(|v| v.as_str()).unwrap_or("");

    // 构建 sameAs 链接
    let social = card.get("social").cloned().unwrap_or_default();
    let mut same_as: Vec<String> = Vec::new();
    if let Some(social_obj) = social.as_object() {
        for (platform, handle) in social_obj {
            if let Some(handle_str) = handle.as_str() {
                let url = SOCIAL_URL_MAP
                    .iter()
                    .find(|(name, _)| *name == platform)
                    .map(|(_, template)| template.replace("{handle}", handle_str))
                    .unwrap_or_else(|| format!("https://{}.com/{}", platform, handle_str));
                same_as.push(url);
            }
        }
    }

    // 构建 knowsAbout
    let tags: Vec<String> = card
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let card_url = format!("https://card.openlink.dev/{}", code);

    let json_ld = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "Person",
        "name": display_name,
        "description": bio,
        "image": avatar,
        "sameAs": same_as,
        "knowsAbout": tags,
        "url": card_url,
    });

    serde_json::to_string_pretty(&json_ld).unwrap_or_default()
}

/// 渲染 HTML 名片页面
fn render_card_html(code: &str, card: &serde_json::Value) -> String {
    let template = include_str!("../templates/card.html");

    let display_name = card.get("display_name").and_then(|v| v.as_str()).unwrap_or("Unknown");
    let bio = card.get("bio").and_then(|v| v.as_str()).unwrap_or("");
    let avatar = card.get("avatar").and_then(|v| v.as_str()).unwrap_or("");
    let theme = card.get("theme").and_then(|v| v.as_str()).unwrap_or("dark");
    let card_url = format!("/card/{}", code);

    // 头像 fallback：取名字首字
    let avatar_fallback = display_name.chars().next().unwrap_or('?').to_string();

    // 社交链接区
    let social_section = build_social_section(code, card);

    // 项目区
    let projects_section = build_projects_section(card);

    // 标签区
    let tags_section = build_tags_section(card);

    // 替换模板占位符
    let html = template
        .replace("{{display_name}}", display_name)
        .replace("{{bio}}", bio)
        .replace("{{avatar}}", avatar)
        .replace("{{theme}}", theme)
        .replace("{{card_url}}", &card_url)
        .replace("{{avatar_fallback}}", &avatar_fallback)
        .replace("{{social_section}}", &social_section)
        .replace("{{projects_section}}", &projects_section)
        .replace("{{tags_section}}", &tags_section);

    html
}

/// 构建社交链接 HTML 区
fn build_social_section(code: &str, card: &serde_json::Value) -> String {
    let social = card.get("social").cloned().unwrap_or_default();
    let social_obj = match social.as_object() {
        Some(obj) => obj,
        None => return String::new(),
    };

    if social_obj.is_empty() {
        return String::new();
    }

    let mut links_html = String::new();
    for (platform, handle) in social_obj {
        if let Some(_handle_str) = handle.as_str() {
            let icon = get_social_icon(&platform);
            let href = format!("/card/{}/{}", code, platform);
            links_html.push_str(&format!(
                r#"<a href="{}" class="social-link" target="_blank" rel="noopener noreferrer">{}{}</a>"#,
                href, icon, platform
            ));
        }
    }

    if links_html.is_empty() {
        return String::new();
    }

    format!(
        r#"<div class="section"><div class="section-title">Social</div><div class="social-grid">{}</div></div>"#,
        links_html
    )
}

/// 构建项目 HTML 区
fn build_projects_section(card: &serde_json::Value) -> String {
    let projects = match card.get("projects").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return String::new(),
    };

    if projects.is_empty() {
        return String::new();
    }

    let badges: Vec<String> = projects
        .iter()
        .filter_map(|v| {
            v.as_str()
                .map(|s| format!(r#"<span class="project-badge">{}</span>"#, s))
        })
        .collect();

    format!(
        r#"<div class="section"><div class="section-title">Projects</div><div class="projects-list">{}</div></div>"#,
        badges.join("")
    )
}

/// 构建标签 HTML 区
fn build_tags_section(card: &serde_json::Value) -> String {
    let tags = match card.get("tags").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return String::new(),
    };

    if tags.is_empty() {
        return String::new();
    }

    let tag_spans: Vec<String> = tags
        .iter()
        .filter_map(|v| v.as_str().map(|s| format!(r#"<span class="tag">{}</span>"#, s)))
        .collect();

    format!(
        r#"<div class="section"><div class="section-title">Tags</div><div class="tags-list">{}</div></div>"#,
        tag_spans.join("")
    )
}

/// 生成 QR Code SVG
fn generate_qr_svg(data: &str) -> String {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let code = match QrCode::new(data.as_bytes()) {
        Ok(c) => c,
        Err(_) => {
            // 生成失败时返回一个简单的错误 SVG
            return r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect fill="#fff" width="100" height="100"/><text x="50" y="55" text-anchor="middle" fill="#666" font-size="12">QR Error</text></svg>"##.to_string();
        }
    };

    let svg_string = code
        .render()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#1a1a2e"))
        .light_color(svg::Color("#ffffff"))
        .build();

    svg_string
}

/// 记录名片访问日志
async fn log_card_access(
    state: &Arc<AppState>,
    link: &Link,
    headers: &HeaderMap,
    action: &str,
    response_time_ms: i64,
) -> Result<(), openlink_store::StoreError> {
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok());

    let ctx = Context::from_request(user_agent, ip);

    let log = AccessLog {
        id: uuid::Uuid::new_v4().to_string(),
        link_id: link.id.clone(),
        context: serde_json::json!({"code": link.code, "action": action}),
        matched_rule: None,
        action_taken: action.to_string(),
        response_time_ms: Some(response_time_ms),
        created_at: chrono::Utc::now(),
        code: Some(link.code.clone()),
        visitor_ip: ip.map(|s| s.to_string()),
        identity_type: Some(format!("{:?}", ctx.identity.identity_type).to_lowercase()),
        device_type: ctx.device.device_type.clone(),
    };
    state.store.log_access(&log).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_json_ld() {
        let card = serde_json::json!({
            "display_name": "小龙",
            "bio": "AI原生开源DAW开发者",
            "avatar": "https://example.com/avatar.png",
            "social": {
                "github": "youbanzhishi"
            },
            "tags": ["Rust", "音频", "开源"],
            "theme": "dark"
        });

        let json_ld = build_json_ld("xiaolong", &card);
        let parsed: serde_json::Value = serde_json::from_str(&json_ld).unwrap();

        assert_eq!(parsed["@type"], "Person");
        assert_eq!(parsed["name"], "小龙");
        assert_eq!(parsed["description"], "AI原生开源DAW开发者");
        assert_eq!(parsed["knowsAbout"][0], "Rust");
        assert!(parsed["sameAs"].as_array().unwrap().len() > 0);
    }

    #[test]
    fn test_render_card_html() {
        let card = serde_json::json!({
            "display_name": "测试用户",
            "bio": "这是一段简介",
            "avatar": "https://example.com/avatar.png",
            "social": {"github": "test"},
            "projects": ["ProjectA"],
            "tags": ["Rust"],
            "theme": "dark"
        });

        let html = render_card_html("test", &card);
        assert!(html.contains("测试用户"));
        assert!(html.contains("这是一段简介"));
        assert!(html.contains("data-theme=\"dark\""));
        assert!(html.contains("ProjectA"));
        assert!(html.contains("Rust"));
    }

    #[test]
    fn test_social_url_map() {
        let url = SOCIAL_URL_MAP
            .iter()
            .find(|(name, _)| *name == "github")
            .map(|(_, template)| template.replace("{handle}", "youbanzhishi"));
        assert_eq!(url, Some("https://github.com/youbanzhishi".to_string()));
    }

    #[test]
    fn test_generate_qr_svg() {
        let svg = generate_qr_svg("/card/test");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("rect"));
    }

    #[test]
    fn test_build_social_section() {
        let card = serde_json::json!({
            "social": {"github": "test", "twitter": "testuser"}
        });
        let section = build_social_section("test", &card);
        assert!(section.contains("github"));
        assert!(section.contains("twitter"));
    }

    #[test]
    fn test_build_projects_section() {
        let card = serde_json::json!({
            "projects": ["OpenDAW", "OpenLink"]
        });
        let section = build_projects_section(&card);
        assert!(section.contains("OpenDAW"));
        assert!(section.contains("OpenLink"));
    }

    #[test]
    fn test_build_tags_section() {
        let card = serde_json::json!({
            "tags": ["Rust", "音频"]
        });
        let section = build_tags_section(&card);
        assert!(section.contains("Rust"));
        assert!(section.contains("音频"));
    }

    #[test]
    fn test_default_theme() {
        assert_eq!(default_theme(), "dark");
    }
}
