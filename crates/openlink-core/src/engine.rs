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
//! Phase 2 增强：
//! - 支持多条件组合（AND/OR）
//! - OnError Hook 降级处理
//! - 完整的路由决策上下文记录
//!
//! 设计铁律：核心层零业务逻辑 — 路由引擎不知道"短链"是什么，只知道 Context→Action

use crate::error::CoreError;
use crate::primitives::{ActionResult, Condition, ConditionLogic, Context, Route, Rule, Target};
use crate::registry::ExtensionRegistry;
use std::sync::Arc;

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
    pub async fn resolve(&self, ctx: &mut Context, route: &Route) -> Result<RouteResult, CoreError> {
        let start = std::time::Instant::now();

        // 1. 运行 BeforeRoute Hooks（改写 Context）
        if let Err(e) = self.run_before_hooks(ctx).await {
            tracing::warn!(error = %e, "BeforeRoute hook error, continuing");
        }

        // 2. 匹配规则：按优先级排序后依次评估
        let matched = self.match_rules(ctx, &route.rules).await;

        // 3. 确定目标：匹配到规则用规则的 Target，否则用 default
        let (target, matched_rule_name) = match matched {
            Ok(Some((rule, rule_name))) => (rule.target.clone(), Some(rule_name)),
            Ok(None) => (route.default_target.clone(), None),
            Err(e) => {
                // 运行 OnError Hooks
                self.run_error_hooks(ctx, &e).await;
                return Err(e);
            }
        };

        // 4. 执行 Action
        let action_result = match self.execute_action(ctx, &target).await {
            Ok(result) => result,
            Err(e) => {
                // 运行 OnError Hooks
                self.run_error_hooks(ctx, &e).await;
                return Err(e);
            }
        };

        let elapsed = start.elapsed();

        // 5. 运行 AfterRoute Hooks
        if let Err(e) = self.run_after_hooks(ctx).await {
            tracing::warn!(error = %e, "AfterRoute hook error, continuing");
        }

        Ok(RouteResult {
            action_result,
            matched_rule: matched_rule_name,
            action_taken: target.action.as_str().to_string(),
            response_time_ms: elapsed.as_millis() as i64,
        })
    }

    /// 匹配规则 — 按优先级排序后依次评估，命中即停
    async fn match_rules<'a>(&self, ctx: &Context, rules: &'a [Rule]) -> Result<Option<(&'a Rule, String)>, CoreError> {
        if rules.is_empty() {
            return Ok(None);
        }

        // 按优先级排序（数值越大越优先）
        let mut sorted_indices: Vec<usize> = (0..rules.len()).collect();
        sorted_indices.sort_by(|a, b| rules[*b].priority.cmp(&rules[*a].priority));

        for idx in sorted_indices {
            let rule = &rules[idx];
            if self.evaluate_rule(ctx, rule).await? {
                let rule_name = format!(
                    "rule[{}] conditions={:?}",
                    idx,
                    rule.all_conditions()
                        .iter()
                        .map(|c| c.condition_type.as_str())
                        .collect::<Vec<_>>()
                );
                tracing::debug!(rule = %rule_name, "Rule matched");
                return Ok(Some((rule, rule_name)));
            }
        }

        Ok(None)
    }

    /// 评估单条规则 — 支持多条件 AND/OR 组合（Phase 2）
    async fn evaluate_rule(&self, ctx: &Context, rule: &Rule) -> Result<bool, CoreError> {
        let conditions = rule.all_conditions();
        let logic = rule.logic();

        match logic {
            ConditionLogic::And => {
                // AND: 所有条件都满足
                for cond in &conditions {
                    if !self.evaluate_condition(ctx, cond).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            ConditionLogic::Or => {
                // OR: 任一条件满足
                for cond in &conditions {
                    if self.evaluate_condition(ctx, cond).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// 评估条件 — 通过 Extension Registry 查找 Condition Handler
    async fn evaluate_condition(&self, ctx: &Context, condition: &Condition) -> Result<bool, CoreError> {
        // 内置条件：always（永远匹配）
        if condition.condition_type == "always" {
            return Ok(true);
        }

        // 内置条件：identity-type
        if condition.condition_type == "identity-type" {
            let target_type = condition.params.get("type").and_then(|v| v.as_str()).unwrap_or("");
            return Ok(format!("{:?}", ctx.identity.identity_type).to_lowercase() == target_type);
        }

        // 内置条件：device-type
        if condition.condition_type == "device-type" {
            let target_type = condition.params.get("type").and_then(|v| v.as_str()).unwrap_or("");
            return Ok(ctx
                .device
                .device_type
                .as_deref()
                .map(|dt| dt == target_type)
                .unwrap_or(false));
        }

        // 内置条件：header-match（Phase 2）
        if condition.condition_type == "header-match" {
            return self.evaluate_header_match(ctx, &condition.params);
        }

        // 内置条件：geo-match（Phase 2: 预留接口）
        if condition.condition_type == "geo-match" {
            return self.evaluate_geo_match(ctx, &condition.params);
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

    /// 评估 header-match 条件
    /// 检查 HTTP Header 是否匹配指定模式
    fn evaluate_header_match(&self, ctx: &Context, params: &serde_json::Value) -> Result<bool, CoreError> {
        let header_name = params
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("");

        if header_name.is_empty() || pattern.is_empty() {
            return Ok(false);
        }

        // 先从 headers JSON 中查找
        if ctx.headers.is_object() {
            for (key, value) in ctx.headers.as_object().unwrap() {
                if key.to_lowercase() == header_name {
                    if let Some(val_str) = value.as_str() {
                        // 支持包含匹配（最常用场景：User-Agent 包含 curl）
                        if val_str.to_lowercase().contains(&pattern.to_lowercase()) {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        // 回退：检查 user_agent_raw（便捷路径）
        if header_name == "user-agent" {
            if let Some(ref ua) = ctx.device.user_agent_raw {
                if ua.to_lowercase().contains(&pattern.to_lowercase()) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// 评估 geo-match 条件（Phase 2: 预留接口，可配置）
    fn evaluate_geo_match(&self, ctx: &Context, params: &serde_json::Value) -> Result<bool, CoreError> {
        // 支持按国家/地区/城市匹配
        if let Some(country) = params.get("country").and_then(|v| v.as_str()) {
            if let Some(ref ctx_country) = ctx.location.country {
                if ctx_country.to_lowercase() == country.to_lowercase() {
                    // 如果还指定了 region，进一步匹配
                    if let Some(region) = params.get("region").and_then(|v| v.as_str()) {
                        if let Some(ref ctx_region) = ctx.location.region {
                            return Ok(ctx_region.to_lowercase() == region.to_lowercase());
                        }
                        return Ok(false);
                    }
                    return Ok(true);
                }
            }
        }

        if let Some(city) = params.get("city").and_then(|v| v.as_str()) {
            if let Some(ref ctx_city) = ctx.location.city {
                return Ok(ctx_city.to_lowercase() == city.to_lowercase());
            }
        }

        // 无地理位置信息时不匹配
        Ok(false)
    }

    /// 执行 Action — 通过 Extension Registry 查找 Action Handler
    async fn execute_action(&self, ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        let action_name = target.action.as_str();

        // 查找 Action Handler
        let handler = self
            .registry
            .get_action_handler(action_name)
            .ok_or_else(|| CoreError::ExtensionError(format!("Action handler '{}' not found", action_name)))?;

        tracing::debug!(action = %action_name, "Executing action");
        handler.execute(ctx, target).await
    }

    /// 运行 BeforeRoute Hooks
    async fn run_before_hooks(&self, ctx: &mut Context) -> Result<(), CoreError> {
        for hook in self.registry.get_before_hooks() {
            tracing::debug!(hook = %hook.name(), "Running BeforeRoute hook");
            *ctx = hook.handle(ctx.clone()).await?;
        }
        Ok(())
    }

    /// 运行 AfterRoute Hooks
    async fn run_after_hooks(&self, ctx: &Context) -> Result<(), CoreError> {
        for hook in self.registry.get_after_hooks() {
            tracing::debug!(hook = %hook.name(), "Running AfterRoute hook");
            let _ = hook.handle(ctx.clone()).await;
        }
        Ok(())
    }

    /// 运行 OnError Hooks（Phase 2）
    async fn run_error_hooks(&self, ctx: &Context, error: &CoreError) {
        for hook in self.registry.get_error_hooks() {
            tracing::debug!(hook = %hook.name(), "Running OnError hook");
            match hook.handle(ctx.clone()).await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(hook_error = %e, "OnError hook failed");
                }
            }
        }
        tracing::warn!(error = %error, "Routing error occurred");
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
    use crate::primitives::{Action, DeviceInfo, Identity, IdentityType};

    fn make_test_ctx(identity_type: IdentityType, device_type: Option<&str>, ua: Option<&str>) -> Context {
        Context {
            identity: Identity {
                id: "test".to_string(),
                identity_type,
                agent_type: None,
            },
            device: DeviceInfo {
                device_type: device_type.map(|s| s.to_string()),
                os: None,
                browser: None,
                bandwidth: None,
                user_agent_raw: ua.map(|s| s.to_string()),
            },
            location: crate::primitives::GeoInfo::default(),
            time: chrono::Utc::now(),
            intent: serde_json::Value::Null,
            session: "test-session".to_string(),
            custom: serde_json::Value::Null,
            headers: serde_json::Value::Null,
        }
    }

    fn make_test_registry() -> Arc<ExtensionRegistry> {
        Arc::new(ExtensionRegistry::new())
    }

    #[tokio::test]
    async fn test_evaluate_identity_type() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        let ctx = make_test_ctx(IdentityType::Human, Some("desktop"), None);
        let condition = Condition {
            condition_type: "identity-type".to_string(),
            params: serde_json::json!({"type": "human"}),
        };
        assert!(engine.evaluate_condition(&ctx, &condition).await.unwrap());
    }

    #[tokio::test]
    async fn test_evaluate_device_type() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        let ctx = make_test_ctx(IdentityType::Service, Some("server"), Some("curl/7.88"));
        let condition = Condition {
            condition_type: "device-type".to_string(),
            params: serde_json::json!({"type": "server"}),
        };
        assert!(engine.evaluate_condition(&ctx, &condition).await.unwrap());
    }

    #[tokio::test]
    async fn test_evaluate_header_match() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        let ctx = make_test_ctx(IdentityType::Service, Some("server"), Some("curl/7.88.1"));
        let condition = Condition {
            condition_type: "header-match".to_string(),
            params: serde_json::json!({"header": "user-agent", "pattern": "curl"}),
        };
        assert!(engine.evaluate_condition(&ctx, &condition).await.unwrap());
    }

    #[tokio::test]
    async fn test_evaluate_header_match_no_match() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        let ctx = make_test_ctx(IdentityType::Human, Some("desktop"), Some("Mozilla/5.0"));
        let condition = Condition {
            condition_type: "header-match".to_string(),
            params: serde_json::json!({"header": "user-agent", "pattern": "curl"}),
        };
        assert!(!engine.evaluate_condition(&ctx, &condition).await.unwrap());
    }

    #[tokio::test]
    async fn test_evaluate_geo_match() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        let mut ctx = make_test_ctx(IdentityType::Human, None, None);
        ctx.location.country = Some("CN".to_string());
        ctx.location.region = Some("Beijing".to_string());

        let condition = Condition {
            condition_type: "geo-match".to_string(),
            params: serde_json::json!({"country": "CN"}),
        };
        assert!(engine.evaluate_condition(&ctx, &condition).await.unwrap());
    }

    #[tokio::test]
    async fn test_rule_and_logic() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        // AND: identity=service AND device=server → both match
        let ctx = make_test_ctx(IdentityType::Service, Some("server"), Some("curl/7.88"));
        let rule = Rule {
            condition: Condition {
                condition_type: "always".to_string(),
                params: serde_json::Value::Null,
            },
            conditions: vec![
                Condition {
                    condition_type: "identity-type".to_string(),
                    params: serde_json::json!({"type": "service"}),
                },
                Condition {
                    condition_type: "device-type".to_string(),
                    params: serde_json::json!({"type": "server"}),
                },
            ],
            condition_logic: ConditionLogic::And,
            target: Target {
                action: Action::JsonData,
                params: serde_json::json!({"message": "API response"}),
            },
            priority: 10,
        };
        assert!(engine.evaluate_rule(&ctx, &rule).await.unwrap());
    }

    #[tokio::test]
    async fn test_rule_and_logic_partial_match() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        // AND: identity=service AND device=mobile → only one matches
        let ctx = make_test_ctx(IdentityType::Service, Some("mobile"), None);
        let rule = Rule {
            condition: Condition {
                condition_type: "always".to_string(),
                params: serde_json::Value::Null,
            },
            conditions: vec![
                Condition {
                    condition_type: "identity-type".to_string(),
                    params: serde_json::json!({"type": "service"}),
                },
                Condition {
                    condition_type: "device-type".to_string(),
                    params: serde_json::json!({"type": "server"}),
                },
            ],
            condition_logic: ConditionLogic::And,
            target: Target {
                action: Action::JsonData,
                params: serde_json::json!({}),
            },
            priority: 10,
        };
        assert!(!engine.evaluate_rule(&ctx, &rule).await.unwrap());
    }

    #[tokio::test]
    async fn test_rule_or_logic() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        // OR: identity=service OR device=mobile → one matches
        let ctx = make_test_ctx(IdentityType::Service, Some("desktop"), None);
        let rule = Rule {
            condition: Condition {
                condition_type: "always".to_string(),
                params: serde_json::Value::Null,
            },
            conditions: vec![
                Condition {
                    condition_type: "identity-type".to_string(),
                    params: serde_json::json!({"type": "service"}),
                },
                Condition {
                    condition_type: "device-type".to_string(),
                    params: serde_json::json!({"type": "mobile"}),
                },
            ],
            condition_logic: ConditionLogic::Or,
            target: Target {
                action: Action::JsonData,
                params: serde_json::json!({}),
            },
            priority: 10,
        };
        assert!(engine.evaluate_rule(&ctx, &rule).await.unwrap());
    }

    #[tokio::test]
    async fn test_backward_compat_single_condition() {
        let registry = make_test_registry();
        let engine = RoutingEngine::new(registry);

        // Phase 1 风格：单个 condition 字段
        let ctx = make_test_ctx(IdentityType::Human, Some("desktop"), None);
        let rule = Rule {
            condition: Condition {
                condition_type: "identity-type".to_string(),
                params: serde_json::json!({"type": "human"}),
            },
            conditions: vec![],
            condition_logic: ConditionLogic::And,
            target: Target {
                action: Action::Redirect,
                params: serde_json::json!({"url": "https://example.com"}),
            },
            priority: 10,
        };
        assert!(engine.evaluate_rule(&ctx, &rule).await.unwrap());
    }
}
