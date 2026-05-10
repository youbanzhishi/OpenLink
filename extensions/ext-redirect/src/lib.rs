//! # ext-redirect — 重定向 Action 扩展
//!
//! OpenLink 的第一个注册扩展，实现 302/301 重定向。
//! 验证 Extension Registry 的设计：新功能 = 注册扩展。
//!
//! 使用方式：
//! ```rust,no_run
//! use ext_redirect::register;
//! use openlink_core::ExtensionRegistry;
//!
//! let mut registry = ExtensionRegistry::new();
//! register(&mut registry).unwrap();
//! ```

use async_trait::async_trait;
use openlink_core::{ActionHandler, ActionResult, Context, CoreError, ExtensionRegistry, Target};
use std::sync::Arc;

/// 重定向 Action Handler
///
/// 实现 302/301 HTTP 重定向，这是传统短链的核心功能。
struct RedirectHandler;

#[async_trait]
impl ActionHandler for RedirectHandler {
    async fn execute(&self, _ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        // 从 target.params 中提取 URL 和状态码
        let url = target
            .params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CoreError::InvalidInput("Redirect action requires 'url' parameter".to_string())
            })?
            .to_string();

        let status_code = target
            .params
            .get("status_code")
            .and_then(|v| v.as_u64())
            .unwrap_or(302) as u16;

        // 状态码校验
        if status_code != 301 && status_code != 302 {
            return Err(CoreError::InvalidInput(
                "Redirect status code must be 301 or 302".to_string(),
            ));
        }

        tracing::debug!(url = %url, status_code = status_code, "Executing redirect");

        Ok(ActionResult::Redirect { url, status_code })
    }

    fn name(&self) -> &str {
        "redirect"
    }
}

/// 注册重定向扩展到 Extension Registry
///
/// 这是启动时调用的入口函数，将 redirect Action Handler 注册到 Registry。
/// 第一个注册的扩展，验证 Extension Registry 的设计。
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    registry.register_action(Arc::new(RedirectHandler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::{Action, Target};

    #[tokio::test]
    async fn test_redirect_handler_302() {
        let handler = RedirectHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Redirect,
            params: serde_json::json!({
                "url": "https://example.com/long-url",
                "status_code": 302,
            }),
        };

        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Redirect { url, status_code } => {
                assert_eq!(url, "https://example.com/long-url");
                assert_eq!(status_code, 302);
            }
            _ => panic!("Expected Redirect result"),
        }
    }

    #[tokio::test]
    async fn test_redirect_handler_301() {
        let handler = RedirectHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Redirect,
            params: serde_json::json!({
                "url": "https://example.com/permanent",
                "status_code": 301,
            }),
        };

        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Redirect { url, status_code } => {
                assert_eq!(url, "https://example.com/permanent");
                assert_eq!(status_code, 301);
            }
            _ => panic!("Expected Redirect result"),
        }
    }

    #[tokio::test]
    async fn test_redirect_handler_default_status() {
        let handler = RedirectHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Redirect,
            params: serde_json::json!({
                "url": "https://example.com/default",
            }),
        };

        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Redirect { status_code, .. } => {
                assert_eq!(status_code, 302); // 默认 302
            }
            _ => panic!("Expected Redirect result"),
        }
    }

    #[tokio::test]
    async fn test_redirect_handler_missing_url() {
        let handler = RedirectHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Redirect,
            params: serde_json::json!({}),
        };

        let result = handler.execute(&ctx, &target).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_redirect_handler_invalid_status() {
        let handler = RedirectHandler;
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Redirect,
            params: serde_json::json!({
                "url": "https://example.com",
                "status_code": 200,
            }),
        };

        let result = handler.execute(&ctx, &target).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_register_with_registry() {
        let mut registry = ExtensionRegistry::new();
        assert!(register(&mut registry).is_ok());
        assert!(registry.get_action_handler("redirect").is_some());
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let mut registry = ExtensionRegistry::new();
        assert!(register(&mut registry).is_ok());
        assert!(register(&mut registry).is_err()); // 重复注册
    }
}
