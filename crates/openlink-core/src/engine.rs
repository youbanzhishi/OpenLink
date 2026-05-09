//! # 路由引擎 — Context → Rule匹配 → Action调度
//!
//! 路由引擎是 OpenLink 的核心调度器。
//! 它不关心具体业务逻辑，只做通用的 Context→Rule→Action 调度。
//!
//! 执行流程：
//! 1. 运行 BeforeRoute Hooks（改写 Context）
//! 2. 按优先级遍历 Route.rules，匹配 Condition
//! 3. 如果匹配到规则，使用对应 Target
//! 4. 如果没有匹配，使用 Route.default_target
//! 5. 查找 Action Handler 并执行
//! 6. 运行 AfterRoute Hooks（记录日志等）
//!
//! 设计铁律：核心层零业务逻辑 — 路由引擎不知道"短链"是什么，只知道 Context→Action

use std::sync::Arc;
use crate::primitives::{
    Context, Route, Target, ActionResult, Rule, Action, HookPhase,
};
use crate::registry::ExtensionRegistry;
use crate::error::CoreError;

/// 路由引擎 — 核心调度器
///
/// 通过 Extension Registry 查找 Action/Condition Handler，
/// 实现通用的 Context→Action 调度。
pub struct RoutingEngine {
    registry: Arc<ExtensionRegistry>,
}

impl RoutingEngine {
    /// 创建路由引擎
    pub fn new(registry: Arc<ExtensionRegistry>) -> Self {
        Self { registry }
    }

    /// 解析路由 — 核心调度方法
    ///
    /// 完整流程：Hook → Rule匹配 → Action执行 → Hook
    pub async fn resolve(
        &self,
        ctx: &mut Context,
        route: &Route,
    ) -> Result<RouteResult, CoreError> {
        let start = std::time::Instant::now();

        // 1. 运行 BeforeRoute Hooks（改写 Context）
        self.run_before_hooks(ctx).await?;

        // 2. 匹配规则：按优先级排序后依次评估
        let matched = self.match_rules(ctx, &route.rules).await?;

        // 3. 确定目标：匹配到规则用规则的 Target，否则用 default
        let (target, matched_rule_name) = match matched {
            Some((rule, rule_name)) => (rule.target.clone(), Some(rule_name)),
            None => (route.default_target.clone(), None),
        };

        // 4. 执行 Action
        let action_result = self.execute_action(ctx, &target).await?;

        let elapsed = start.elapsed();

        // 5. 运行 AfterRoute Hooks
        self.run_after_hooks(ctx).await?;

        Ok(RouteResult {
            action_result,
            matched_rule: matched_rule_name,
            action_taken: target.action.as_str().to_string(),
            response_time_ms: elapsed.as_millis() as i64,
        })
    }

    /// 匹配规则 — 按优先级排序后依次评估，命中即停
    async fn match_rules<'a>(
        &self,
        ctx: &Context,
        rules: &'a [Rule],
    ) -> Result<Option<(&'a Rule, String)>, CoreError> {
        if rules.is_empty() {
            return Ok(None);
        }

        // 按优先级排序（数值越大越优先）
        let mut sorted_indices: Vec<usize> = (0..rules.len()).collect();
        sorted_indices.sort_by(|a, b| rules[*b].priority.cmp(&rules[*a].priority));

        for idx in sorted_indices {
            let rule = &rules[idx];
            if self.evaluate_condition(ctx, &rule.condition).await? {
                let rule_name = format!(
                    "rule[{}] condition={}",
                    idx, rule.condition.condition_type
                );
                tracing::debug!(rule = %rule_name, "Rule matched");
                return Ok(Some((rule, rule_name)));
            }
        }

        Ok(None)
    }

    /// 评估条件 — 通过 Extension Registry 查找 Condition Handler
    async fn evaluate_condition(
        &self,
        ctx: &Context,
        condition: &crate::primitives::Condition,
    ) -> Result<bool, CoreError> {
        // 内置条件：always（永远匹配）
        if condition.condition_type == "always" {
            return Ok(true);
        }

        // 内置条件：identity-type
        if condition.condition_type == "identity-type" {
            let target_type = condition
                .params
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return Ok(format!("{:?}", ctx.identity.identity_type).to_lowercase() == target_type);
        }

        // 内置条件：device-type
        if condition.condition_type == "device-type" {
            let target_type = condition
                .params
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return Ok(ctx
                .device
                .device_type
                .as_deref()
                .map(|dt| dt == target_type)
                .unwrap_or(false));
        }

        // 查找注册的 Condition Handler
        if let Some(handler) = self.registry.get_condition_handler(&condition.condition_type) {
            return handler.evaluate(ctx, &condition.params).await;
        }

        // 未知条件类型：记录警告，不匹配
        tracing::warn!(
            condition_type = %condition.condition_type,
            "Unknown condition type, treating as not matched"
        );
        Ok(false)
    }

    /// 执行 Action — 通过 Extension Registry 查找 Action Handler
    async fn execute_action(
        &self,
        ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let action_name = target.action.as_str();

        // 查找 Action Handler
        let handler = self
            .registry
            .get_action_handler(action_name)
            .ok_or_else(|| {
                CoreError::ExtensionError(format!("Action handler '{}' not found", action_name))
            })?;

        tracing::debug!(action = %action_name, "Executing action");
        handler.execute(ctx, target).await
    }

    /// 运行 BeforeRoute Hooks
    async fn run_before_hooks(&self, ctx: &mut Context) -> Result<(), CoreError> {
        for hook in self.registry.get_before_hooks() {
            *ctx = hook.handle(ctx.clone()).await?;
        }
        Ok(())
    }

    /// 运行 AfterRoute Hooks
    async fn run_after_hooks(&self, ctx: &Context) -> Result<(), CoreError> {
        for hook in self.registry.get_after_hooks() {
            // AfterRoute 钩子不应改写 Context，这里只是执行副作用（如日志）
            let _ = hook.handle(ctx.clone()).await;
        }
        Ok(())
    }
}

/// 路由结果 — 引擎的输出
pub struct RouteResult {
    /// Action 执行结果
    pub action_result: ActionResult,
    /// 命中的规则名称
    pub matched_rule: Option<String>,
    /// 执行的动作名称
    pub action_taken: String,
    /// 响应时间（毫秒）
    pub response_time_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::*;
    use crate::registry::ActionHandler;
    use async_trait::async_trait;
    use chrono::Utc;

    /// 测试用 Redirect Action Handler
    struct MockRedirectHandler;

    #[async_trait]
    impl ActionHandler for MockRedirectHandler {
        async fn execute(&self, _ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
            let url = target
                .params
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("https://example.com")
                .to_string();
            let status_code = target
                .params
                .get("status_code")
                .and_then(|v| v.as_u64())
                .unwrap_or(302) as u16;
            Ok(ActionResult::Redirect { url, status_code })
        }

        fn name(&self) -> &str {
            "redirect"
        }
    }

    fn make_simple_route(target_url: &str) -> Route {
        Route {
            id: "route-1".to_string(),
            link_id: "link-1".to_string(),
            rules: vec![],
            default_target: Target {
                action: Action::Redirect,
                params: serde_json::json!({
                    "url": target_url,
                    "status_code": 302,
                }),
            },
            version: 1,
            created_at: Utc::now(),
        }
    }

    fn make_context() -> Context {
        Context::from_request(None, None)
    }

    #[tokio::test]
    async fn test_simple_redirect_route() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register_action(Arc::new(MockRedirectHandler))
            .unwrap();

        let engine = RoutingEngine::new(Arc::new(registry));
        let route = make_simple_route("https://example.com/long-url");
        let mut ctx = make_context();

        let result = engine.resolve(&mut ctx, &route).await.unwrap();
        match result.action_result {
            ActionResult::Redirect { url, status_code } => {
                assert_eq!(url, "https://example.com/long-url");
                assert_eq!(status_code, 302);
            }
            _ => panic!("Expected Redirect result"),
        }
        assert_eq!(result.action_taken, "redirect");
        assert!(result.matched_rule.is_none()); // No rules, just default
    }

    #[tokio::test]
    async fn test_route_with_matching_condition() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register_action(Arc::new(MockRedirectHandler))
            .unwrap();

        let engine = RoutingEngine::new(Arc::new(registry));
        let route = Route {
            id: "route-1".to_string(),
            link_id: "link-1".to_string(),
            rules: vec![Rule {
                condition: Condition {
                    condition_type: "identity-type".to_string(),
                    params: serde_json::json!({"type": "human"}),
                },
                target: Target {
                    action: Action::Redirect,
                    params: serde_json::json!({
                        "url": "https://human.example.com",
                        "status_code": 302,
                    }),
                },
                priority: 10,
            }],
            default_target: Target {
                action: Action::Redirect,
                params: serde_json::json!({
                    "url": "https://default.example.com",
                    "status_code": 302,
                }),
            },
            version: 1,
            created_at: Utc::now(),
        };

        let mut ctx = make_context(); // Default is Human
        let result = engine.resolve(&mut ctx, &route).await.unwrap();
        match result.action_result {
            ActionResult::Redirect { url, .. } => {
                assert_eq!(url, "https://human.example.com");
            }
            _ => panic!("Expected Redirect result"),
        }
        assert!(result.matched_rule.is_some());
    }

    #[tokio::test]
    async fn test_route_falls_to_default() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register_action(Arc::new(MockRedirectHandler))
            .unwrap();

        let engine = RoutingEngine::new(Arc::new(registry));
        let route = Route {
            id: "route-1".to_string(),
            link_id: "link-1".to_string(),
            rules: vec![Rule {
                condition: Condition {
                    condition_type: "identity-type".to_string(),
                    params: serde_json::json!({"type": "agent"}),
                },
                target: Target {
                    action: Action::Redirect,
                    params: serde_json::json!({
                        "url": "https://agent.example.com",
                        "status_code": 302,
                    }),
                },
                priority: 10,
            }],
            default_target: Target {
                action: Action::Redirect,
                params: serde_json::json!({
                    "url": "https://default.example.com",
                    "status_code": 302,
                }),
            },
            version: 1,
            created_at: Utc::now(),
        };

        let mut ctx = make_context(); // Default is Human, won't match "agent"
        let result = engine.resolve(&mut ctx, &route).await.unwrap();
        match result.action_result {
            ActionResult::Redirect { url, .. } => {
                assert_eq!(url, "https://default.example.com");
            }
            _ => panic!("Expected Redirect result"),
        }
        assert!(result.matched_rule.is_none());
    }

    #[tokio::test]
    async fn test_unknown_action_handler() {
        let registry = ExtensionRegistry::new(); // No handlers registered
        let engine = RoutingEngine::new(Arc::new(registry));
        let route = Route {
            id: "route-1".to_string(),
            link_id: "link-1".to_string(),
            rules: vec![],
            default_target: Target {
                action: Action::Custom("nonexistent".to_string()),
                params: serde_json::json!({}),
            },
            version: 1,
            created_at: Utc::now(),
        };

        let mut ctx = make_context();
        let result = engine.resolve(&mut ctx, &route).await;
        assert!(result.is_err());
    }
}
