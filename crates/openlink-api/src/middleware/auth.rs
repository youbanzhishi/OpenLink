//! # Bearer Token 认证中间件
//!
//! Phase 2: 管理API需要Token认证，重定向API不需要认证。
//! 支持多Token，每个Token有权限范围（read/write/admin）。

use axum::{
    body::Body,
    extract::Request,
    http::{StatusCode, HeaderMap},
    middleware::Next,
    response::{IntoResponse, Response},
};
use crate::state::AppState;
use std::sync::Arc;

/// Token 认证中间件
///
/// 检查请求的 Authorization header 是否包含有效的 Bearer Token。
/// 如果认证未启用（auth.enabled = false），所有请求都通过。
pub async fn require_auth(
    headers: HeaderMap,
    state: Arc<AppState>,
) -> Result<(), (StatusCode, &'static str)> {
    if !state.config.auth.enabled {
        return Ok(());
    }

    let token = extract_bearer_token(&headers);
    match token {
        Some(t) => {
            if state.config.auth.validate_token(&t).is_some() {
                Ok(())
            } else {
                Err((StatusCode::UNAUTHORIZED, "Invalid or unauthorized token"))
            }
        }
        None => Err((StatusCode::UNAUTHORIZED, "Missing Authorization header")),
    }
}

/// 从 Authorization header 提取 Bearer Token
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    if auth_header.starts_with("Bearer ") {
        Some(auth_header[7..].trim().to_string())
    } else {
        None
    }
}

/// Axum middleware 层：API Token 认证
///
/// 在 router.rs 中将管理路由与公开路由分开，
/// 管理路由添加认证中间件层。
pub async fn auth_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    // 从扩展中获取 AppState
    let state = req.extensions().get::<Arc<AppState>>().cloned();

    if let Some(state) = state {
        if state.config.auth.enabled {
            let auth_header = req.headers().get("authorization")
                .and_then(|v| v.to_str().ok());

            match auth_header {
                Some(h) if h.starts_with("Bearer ") => {
                    let token = h[7..].trim();
                    if state.config.auth.validate_token(token).is_none() {
                        return (StatusCode::UNAUTHORIZED, "Invalid or unauthorized token").into_response();
                    }
                }
                _ => {
                    return (StatusCode::UNAUTHORIZED, "Missing Authorization header").into_response();
                }
            }
        }
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-secret".parse().unwrap());
        let token = extract_bearer_token(&headers);
        assert_eq!(token, Some("test-secret".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_missing() {
        let headers = HeaderMap::new();
        let token = extract_bearer_token(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_bearer_token_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Basic dXNlcjpwYXNz".parse().unwrap());
        let token = extract_bearer_token(&headers);
        assert!(token.is_none());
    }
}
