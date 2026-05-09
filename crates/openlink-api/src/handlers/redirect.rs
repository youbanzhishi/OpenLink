//! # 重定向处理器
//!
//! GET /:code → 302 — 核心路径，必须最快
//!
//! 这是 OpenLink 最关键的请求路径：
//! 1. 查找 Link
//! 2. 获取 Route
//! 3. 构建请求 Context
//! 4. 调用路由引擎解析
//! 5. 返回重定向响应
//! 6. 记录访问日志（可观测内置）

use axum::{
    extract::{State, Path, ConnectInfo},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Redirect},
};
use std::sync::Arc;
use openlink_core::{Context, AccessLog, ActionResult};
use openlink_store::Store;
use crate::state::AppState;

/// 短链重定向 — 核心路径
///
/// GET /:code → 302
/// 这是最频繁的请求路径，优化为最快响应。
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
                // 记录访问日志
                let _ = log_redirect_access(&state, &link, url, start.elapsed().as_millis() as i64).await;
                return Redirect::temporary(url).into_response();
            }
            return (StatusCode::NOT_FOUND, "No route or target_url for this link").into_response();
        }
        Err(e) => {
            tracing::error!(link_id = %link.id, error = %e, "Failed to lookup route");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // 3. 构建请求 Context
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok());
    let mut ctx = Context::from_request(user_agent.as_deref(), ip);

    // 4. 调用路由引擎解析
    match state.engine.resolve(&mut ctx, &route).await {
        Ok(result) => {
            // 5. 记录访问日志（可观测内置）
            let _ = log_access(&state, &link, &ctx, &result.matched_rule, &result.action_taken, result.response_time_ms).await;

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
            }
        }
        Err(e) => {
            tracing::error!(code = %code, error = %e, "Routing engine error");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// 记录重定向访问日志（简化路径，无路由规则时的快速日志）
async fn log_redirect_access(
    state: &Arc<AppState>,
    link: &openlink_core::Link,
    target_url: &str,
    response_time_ms: i64,
) -> Result<(), openlink_store::StoreError> {
    let log = AccessLog {
        id: uuid::Uuid::new_v4().to_string(),
        link_id: link.id.clone(),
        context: serde_json::json!({"code": link.code}),
        matched_rule: None,
        action_taken: "redirect".to_string(),
        response_time_ms: Some(response_time_ms),
        created_at: chrono::Utc::now(),
    };
    state.store.log_access(&log).await
}

/// 记录访问日志（完整路由路径）
async fn log_access(
    state: &Arc<AppState>,
    link: &openlink_core::Link,
    ctx: &Context,
    matched_rule: &Option<String>,
    action_taken: &str,
    response_time_ms: i64,
) -> Result<(), openlink_store::StoreError> {
    let log = AccessLog {
        id: uuid::Uuid::new_v4().to_string(),
        link_id: link.id.clone(),
        context: serde_json::to_value(ctx).unwrap_or_default(),
        matched_rule: matched_rule.clone(),
        action_taken: action_taken.to_string(),
        response_time_ms: Some(response_time_ms),
        created_at: chrono::Utc::now(),
    };
    state.store.log_access(&log).await
}
