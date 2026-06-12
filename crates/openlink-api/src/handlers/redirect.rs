//! # 重定向处理器
//!
//! GET /:code → 302 / JSON — 核心路径，必须最快
//!
//! 这是 OpenLink 最关键的请求路径：
//! 1. 查找 Link
//! 2. 获取 Route
//! 3. 构建请求 Context（Phase 2: 增强 User-Agent 解析 + Headers 保留）
//! 4. 调用路由引擎解析
//! 5. 根据路由结果返回重定向或 JSON（Phase 2: 同一短链，浏览器跳网页，curl 返回 JSON）
//! 6. 记录访问日志（Phase 2: 增强字段）

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Redirect},
};
use openlink_core::{AccessLog, ActionResult, Context};
use std::sync::Arc;

use crate::config::KnowledgeSource;
use crate::state::AppState;

/// 知识源邀请码短链 — 直接返回入口内容
///
/// 当 /:code 匹配到知识源邀请码时，根据 User-Agent 返回对应格式：
/// - 只读智能体 → Markdown
/// - 全能智能体 → JSON（含 API 端点 + token）
/// - 浏览器 → HTML 引导页
async fn serve_knowledge_entry(
    state: &Arc<AppState>,
    source: &KnowledgeSource,
    headers: &HeaderMap,
) -> axum::response::Response {
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let base_url = &state.config.knowledge.base_url;
    let repo_path = &source.repo_path;

    // 判断访问者类型
    let is_llm = user_agent.contains("GPT")
        || user_agent.contains("Claude")
        || user_agent.contains("Gemini")
        || user_agent.contains("openai")
        || user_agent.contains("anthropic")
        || user_agent.contains("doubao")
        || user_agent.contains("qwen")
        || user_agent.contains("kimi")
        || user_agent.contains("deepseek")
        || user_agent.contains("ChatGLM")
        || user_agent.contains("Meta-Llama");

    let is_curl = user_agent.starts_with("curl/")
        || user_agent.starts_with("Wget/")
        || user_agent.starts_with("HTTPie/");

    let is_api_client = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("application/json"))
        .unwrap_or(false);

    if is_llm || is_curl || is_api_client {
        // 智能/API 客户端：返回精简 Markdown
        match crate::handlers::knowledge::build_lightweight_markdown(repo_path, &source.name, base_url, source.label()) {
            Ok(markdown) => (
                StatusCode::OK,
                [("Content-Type", "text/markdown; charset=utf-8")],
                markdown,
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read knowledge entry: {}", e.1),
            )
                .into_response(),
        }
    } else if user_agent.contains("Mozilla") || user_agent.contains("WebKit") {
        // 浏览器：返回 HTML 引导页
        let html = format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>{} — 加入知识体系</title>
<style>
  body {{ font-family:-apple-system,BlinkMacSystemFont,sans-serif; max-width:680px; margin:60px auto; padding:0 20px; color:#1a1a1a; line-height:1.7; }}
  h1 {{ color:#3b82f6; }} .card {{ background:#f8fafc; border:1px solid #e2e8f0; border-radius:12px; padding:20px 24px; margin:16px 0; }}
  code {{ background:#f1f5f9; padding:2px 6px; border-radius:4px; font-size:14px; }}
  .tip {{ color:#64748b; font-size:14px; }}
</style></head><body>
<h1>🐉 {}</h1>
<p>你已通过短链访问知识源，以下是接入方式：</p>
<div class="card">
  <h3>🤖 AI 智能体</h3>
  <p>直接访问此短链即可获取知识入口文档（Markdown 格式）。</p>
  <p class="tip">用 <code>curl</code> 或智能体 HTTP 工具访问，将自动返回 Markdown。</p>
</div>
<div class="card">
  <h3>🔧 API 接入</h3>
  <p>入口文档：<code>{base_url}/api/v1/knowledge/{source_name}/entry</code></p>
  <p>角色 RULES：<code>{base_url}/api/v1/knowledge/{source_name}/role/:name</code></p>
  <p>项目 INDEX：<code>{base_url}/api/v1/knowledge/{source_name}/project/:name</code></p>
</div>
<div class="card">
  <h3>📋 邀请码</h3>
  <p><code>{invite_codes}</code></p>
</div>
</body></html>"#,
            source.label(),
            source.label(),
            base_url = base_url,
            source_name = source.name,
            invite_codes = source.invite_codes.join(", "),
        );
        (
            StatusCode::OK,
            [("Content-Type", "text/html; charset=utf-8")],
            html,
        )
            .into_response()
    } else {
        // 默认：返回 Markdown
        match crate::handlers::knowledge::build_lightweight_markdown(repo_path, &source.name, base_url, source.label()) {
            Ok(markdown) => (
                StatusCode::OK,
                [("Content-Type", "text/markdown; charset=utf-8")],
                markdown,
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read knowledge entry: {}", e.1),
            )
                .into_response(),
        }
    }
}

/// 短链重定向 — 核心路径
///
/// GET /:code → 302 或 JSON
/// Phase 2: 同一短链，浏览器访问跳网页，curl 访问返回 JSON
pub async fn redirect(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // 1. 查找 Link
    let link = match state.store.get_link_by_code(&code).await {
        Ok(Some(link)) => link,
        Ok(None) => {
            // Fallback: 短链码可能是知识源邀请码
            // 访问 link.opendev.dev/try-openclaw 等同于 /join?code=try-openclaw
            if state.config.knowledge.enabled {
                if let Some(source) = state.config.knowledge.find_source_by_short_code(&code) {
                    tracing::info!(code = %code, source = %source.name, "Short link resolved to knowledge invite code");
                    return serve_knowledge_entry(&state, &source, &headers).await;
                }
            }
            return (StatusCode::NOT_FOUND, "Link not found").into_response();
        }
        Err(e) => {
            tracing::error!(code = %code, error = %e, "Failed to lookup link");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !link.is_active {
        return (StatusCode::GONE, "Link is inactive").into_response();
    }

    // 2. 获取 Route
    let route = match state.store.get_route_by_link_id(&link.id).await {
        Ok(Some(route)) => route,
        Ok(None) => {
            // 没有路由规则，尝试从 payload 中获取 target_url 作为简单重定向
            // 这是传统短链的最简形态
            if let Some(url) = link.payload.get("target_url").and_then(|v| v.as_str()) {
                // 记录访问日志（增强版）
                let _ = log_redirect_access(&state, &link, &headers, url, start.elapsed().as_millis() as i64).await;
                return Redirect::temporary(url).into_response();
            }
            return (StatusCode::NOT_FOUND, "No route or target_url for this link").into_response();
        }
        Err(e) => {
            tracing::error!(link_id = %link.id, error = %e, "Failed to lookup route");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // 3. 构建请求 Context（Phase 2: 增强 User-Agent 解析 + Headers 保留）
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok());

    // 构建 Headers Map（用于 header_match 条件）
    let mut headers_map = std::collections::HashMap::new();
    for (key, value) in headers.iter() {
        if let Ok(val) = value.to_str() {
            headers_map.insert(key.to_string(), val.to_string());
        }
    }

    let mut ctx = Context::from_request_with_headers(user_agent.as_deref(), ip, &headers_map);

    // 4. 调用路由引擎解析
    match state.engine.resolve(&mut ctx, &route).await {
        Ok(result) => {
            // 5. 记录访问日志（增强版）
            let _ = log_access(
                &state,
                &link,
                &ctx,
                &headers,
                &result.matched_rule,
                &result.action_taken,
                result.response_time_ms,
            )
            .await;

            // 6. 转换为 HTTP 响应
            match result.action_result {
                ActionResult::Redirect { url, status_code } => {
                    if status_code == 301 {
                        Redirect::permanent(&url).into_response()
                    } else {
                        Redirect::temporary(&url).into_response()
                    }
                }
                ActionResult::Json(val) => ([("content-type", "application/json")], val.to_string()).into_response(),
                ActionResult::Custom { content_type, body } => {
                    ([("content-type", content_type.as_str())], body).into_response()
                }
                ActionResult::WebhookTriggered { target_url, status } => (
                    [("content-type", "application/json")],
                    serde_json::json!({
                        "type": "webhook_triggered",
                        "target_url": target_url,
                        "status": status,
                    })
                    .to_string(),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            tracing::error!(code = %code, error = %e, "Routing engine error");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// 记录重定向访问日志（简化路径，无路由规则时的快速日志）
/// Phase 2: 增强版，包含 code/visitor_ip/identity_type/device_type
async fn log_redirect_access(
    state: &Arc<AppState>,
    link: &openlink_core::Link,
    headers: &HeaderMap,
    target_url: &str,
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
        context: serde_json::json!({"code": link.code, "target_url": target_url}),
        matched_rule: None,
        action_taken: "redirect".to_string(),
        response_time_ms: Some(response_time_ms),
        created_at: chrono::Utc::now(),
        code: Some(link.code.clone()),
        visitor_ip: ip.map(|s| s.to_string()),
        identity_type: Some(format!("{:?}", ctx.identity.identity_type).to_lowercase()),
        device_type: ctx.device.device_type.clone(),
    };
    state.store.log_access(&log).await
}

/// 记录访问日志（完整路由路径）
/// Phase 2: 增强版，包含 code/visitor_ip/identity_type/device_type
async fn log_access(
    state: &Arc<AppState>,
    link: &openlink_core::Link,
    ctx: &Context,
    headers: &HeaderMap,
    matched_rule: &Option<String>,
    action_taken: &str,
    response_time_ms: i64,
) -> Result<(), openlink_store::StoreError> {
    let ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok());

    let log = AccessLog {
        id: uuid::Uuid::new_v4().to_string(),
        link_id: link.id.clone(),
        context: serde_json::to_value(ctx).unwrap_or_default(),
        matched_rule: matched_rule.clone(),
        action_taken: action_taken.to_string(),
        response_time_ms: Some(response_time_ms),
        created_at: chrono::Utc::now(),
        code: Some(link.code.clone()),
        visitor_ip: ip.map(|s| s.to_string()),
        identity_type: Some(format!("{:?}", ctx.identity.identity_type).to_lowercase()),
        device_type: ctx.device.device_type.clone(),
    };
    state.store.log_access(&log).await
}

/// 分享码重定向 — 通过分享码访问文件
///
/// GET /s/:share_code → 302 重定向到文件下载 URL 或返回 JSON
pub async fn share_redirect(
    State(_state): State<Arc<AppState>>,
    Path(share_code): Path<String>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    // 在实际实现中，应该查询 share_code 对应的文件
    // 这里简化处理，返回一个 JSON 响应告知客户端调用下载 API

    tracing::info!(share_code = %share_code, "Share code accessed");

    // 查找分享记录
    // 这里应该调用 state.store.get_file_by_share_code(&share_code)
    // 简化处理，返回元信息

    let response = serde_json::json!({
        "type": "share_access",
        "share_code": share_code,
        "message": "Use /api/v1/files/download endpoint with this share_code"
    });

    (StatusCode::OK, Json(response)).into_response()
}
