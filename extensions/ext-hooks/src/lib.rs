//! # ext-hooks — Hook 扩展（BeforeRoute/AfterRoute/OnError）
//!
//! 提供 OpenLink 内置的 Hook 处理器：
//! - `identity-inject`: BeforeRoute — 注入访问者身份信息
//! - `access-log`: AfterRoute — 访问日志记录
//! - `error-fallback`: OnError — 错误降级处理
//!
//! 设计验证：Hook 通过 Extension Registry 注册，核心不改。

use async_trait::async_trait;
use openlink_core::primitives::IdentityType;
use openlink_core::{Context, CoreError, ExtensionRegistry, HookHandler, HookPhase};
use std::sync::Arc;

// ─── Identity Inject Hook (BeforeRoute) ────────────────────

/// BeforeRoute Hook: 根据 User-Agent 注入身份信息
///
/// Phase 2: 增强版，识别 curl/wget/Agent 等请求类型，
/// 自动设置 identity_type 和 device_type。
struct IdentityInjectHook;

#[async_trait]
impl HookHandler for IdentityInjectHook {
    async fn handle(&self, mut ctx: Context) -> Result<Context, CoreError> {
        // 如果已经有非默认身份，跳过
        if ctx.identity.identity_type != IdentityType::Human {
            return Ok(ctx);
        }

        // 从 User-Agent 检测身份
        if let Some(ref ua) = ctx.device.user_agent_raw {
            let ua_lower = ua.to_lowercase();
            let new_identity_type =
                if ua_lower.contains("curl/") || ua_lower.contains("wget/") || ua_lower.contains("python-requests/") {
                    IdentityType::Service
                } else if ua_lower.contains("openai")
                    || ua_lower.contains("anthropic")
                    || ua_lower.contains("claude")
                    || ua_lower.contains("agent")
                    || ua_lower.contains("bot")
                {
                    IdentityType::Agent
                } else {
                    IdentityType::Human
                };

            ctx.identity.identity_type = new_identity_type.clone();

            // 同时更新 device_type
            if new_identity_type == IdentityType::Service && ctx.device.device_type.as_deref() != Some("server") {
                ctx.device.device_type = Some("server".to_string());
            }
        }

        tracing::debug!(
            identity_type = ?ctx.identity.identity_type,
            device_type = ?ctx.device.device_type,
            "IdentityInject hook: injected identity"
        );

        Ok(ctx)
    }

    fn name(&self) -> &str {
        "identity-inject"
    }

    fn phase(&self) -> HookPhase {
        HookPhase::BeforeRoute
    }

    fn priority(&self) -> i32 {
        100 // 高优先级：在其他 Hook 之前执行
    }
}

// ─── Access Log Hook (AfterRoute) ──────────────────────────

/// AfterRoute Hook: 访问日志记录
///
/// 记录每次路由决策的完整上下文，用于统计和审计。
struct AccessLogHook;

#[async_trait]
impl HookHandler for AccessLogHook {
    async fn handle(&self, ctx: Context) -> Result<Context, CoreError> {
        tracing::info!(
            identity_type = ?ctx.identity.identity_type,
            device_type = ?ctx.device.device_type,
            session = %ctx.session,
            "AccessLog hook: recorded access"
        );
        // 实际的日志写入由 redirect handler 中的 log_access 完成
        // 这里只做 tracing 级别的记录
        Ok(ctx)
    }

    fn name(&self) -> &str {
        "access-log"
    }

    fn phase(&self) -> HookPhase {
        HookPhase::AfterRoute
    }

    fn priority(&self) -> i32 {
        0 // 低优先级：最后执行
    }
}

// ─── Error Fallback Hook (OnError) ─────────────────────────

/// OnError Hook: 错误降级处理
///
/// 路由出错时的兜底处理，记录错误日志并可选择降级到默认重定向。
struct ErrorFallbackHook;

#[async_trait]
impl HookHandler for ErrorFallbackHook {
    async fn handle(&self, ctx: Context) -> Result<Context, CoreError> {
        tracing::warn!(
            session = %ctx.session,
            identity_type = ?ctx.identity.identity_type,
            "ErrorFallback hook: routing error occurred"
        );
        // 降级处理逻辑可以在这里实现
        // 例如：重定向到默认页面、返回错误页面等
        Ok(ctx)
    }

    fn name(&self) -> &str {
        "error-fallback"
    }

    fn phase(&self) -> HookPhase {
        HookPhase::OnError
    }

    fn priority(&self) -> i32 {
        100 // 高优先级
    }
}

/// 注册所有 Hook 扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    registry.register_hook(Arc::new(IdentityInjectHook))?;
    registry.register_hook(Arc::new(AccessLogHook))?;
    registry.register_hook(Arc::new(ErrorFallbackHook))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::DeviceInfo;

    #[tokio::test]
    async fn test_identity_inject_hook_curl() {
        let hook = IdentityInjectHook;
        let mut ctx = Context::from_request(Some("curl/7.88.1"), Some("127.0.0.1"));
        // 重置为默认 Human（from_request 已经设置了 Service，这里测试 hook 逻辑）
        ctx.identity.identity_type = IdentityType::Human;
        ctx.device.user_agent_raw = Some("curl/7.88.1".to_string());

        let result = hook.handle(ctx).await.unwrap();
        assert_eq!(result.identity.identity_type, IdentityType::Service);
        assert_eq!(result.device.device_type.as_deref(), Some("server"));
    }

    #[tokio::test]
    async fn test_identity_inject_hook_browser() {
        let hook = IdentityInjectHook;
        let mut ctx = Context::from_request(None, None);
        ctx.device.user_agent_raw = Some("Mozilla/5.0 (Windows NT 10.0)".to_string());

        let result = hook.handle(ctx).await.unwrap();
        assert_eq!(result.identity.identity_type, IdentityType::Human);
    }

    #[tokio::test]
    async fn test_identity_inject_hook_agent() {
        let hook = IdentityInjectHook;
        let mut ctx = Context::from_request(None, None);
        ctx.device.user_agent_raw = Some("OpenAI/1.0 Bot".to_string());

        let result = hook.handle(ctx).await.unwrap();
        assert_eq!(result.identity.identity_type, IdentityType::Agent);
    }

    #[tokio::test]
    async fn test_identity_inject_hook_skip_non_default() {
        let hook = IdentityInjectHook;
        let mut ctx = Context::from_request(None, None);
        ctx.identity.identity_type = IdentityType::Service; // 已经非默认
        ctx.device.user_agent_raw = Some("curl/7.88".to_string());

        let result = hook.handle(ctx).await.unwrap();
        // 应该跳过，保持原有 Service 类型
        assert_eq!(result.identity.identity_type, IdentityType::Service);
    }

    #[tokio::test]
    async fn test_access_log_hook() {
        let hook = AccessLogHook;
        let ctx = Context::from_request(None, None);
        let result = hook.handle(ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_error_fallback_hook() {
        let hook = ErrorFallbackHook;
        let ctx = Context::from_request(None, None);
        let result = hook.handle(ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_register_hooks() {
        let mut registry = ExtensionRegistry::new();
        assert!(register(&mut registry).is_ok());
    }

    #[tokio::test]
    async fn test_hook_priorities() {
        let mut registry = ExtensionRegistry::new();
        register(&mut registry).unwrap();

        let before_hooks = registry.get_before_hooks();
        assert!(before_hooks.iter().any(|h| h.name() == "identity-inject"));
        assert_eq!(before_hooks[0].name(), "identity-inject"); // 最高优先级在前

        let after_hooks = registry.get_after_hooks();
        assert!(after_hooks.iter().any(|h| h.name() == "access-log"));

        let error_hooks = registry.get_error_hooks();
        assert!(error_hooks.iter().any(|h| h.name() == "error-fallback"));
    }
}
