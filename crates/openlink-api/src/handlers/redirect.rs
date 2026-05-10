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

use crate::state::AppState;

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
        Ok(None) => return (StatusCode::NOT_FOUND, "Link not found").into_response(),
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
                let _ = log_redirect_access(
                    &state,
                    &link,
                    &headers,
                    url,
                    start.elapsed().as_millis() as i64,
                )
                .await;
                return Redirect::temporary(url).into_response();
            }
            return (
                StatusCode::NOT_FOUND,
                "No route or target_url for this link",
            )
                .into_response();
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
                ActionResult::Json(val) => {
                    ([("content-type", "application/json")], val.to_string()).into_response()
                }
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
