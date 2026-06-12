//! # Extension Registry — 扩展注册表
//!
//! 与 OpenDAW 的 Extension Registry 同构，四柱模型：
//! - Action API：注册新动作
//! - Condition API：注册新路由条件
//! - Hook API：注册拦截器
//! - Protocol API：注册协议适配器
//!
//! 设计铁律：新功能 = 注册扩展，架构本身永远不需要改。

use crate::error::CoreError;
use crate::primitives::{ActionResult, Context, HookPhase, Target};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

// ─── Action Handler ─────────────────────────────────────────

/// Action 执行器 trait — 所有 Action 扩展必须实现此接口
#[async_trait]
pub trait ActionHandler: Send + Sync {
    /// 执行 Action
    ///
    /// # Arguments
    /// * `ctx` - 请求上下文
    /// * `target` - 目标（包含 Action 类型和参数）
    ///
    /// # Returns
    /// Action 执行结果
    async fn execute(&self, ctx: &Context, target: &Target) -> Result<ActionResult, CoreError>;

    /// 返回此 Handler 对应的 Action 名称
    fn name(&self) -> &str;
}

// ─── Condition Handler ──────────────────────────────────────

/// 条件评估器 trait — 所有 Condition 扩展必须实现此接口
#[async_trait]
pub trait ConditionHandler: Send + Sync {
    /// 评估条件是否满足
    ///
    /// # Arguments
    /// * `ctx` - 请求上下文
    /// * `params` - 条件参数
    ///
    /// # Returns
    /// 条件是否满足
    async fn evaluate(&self, ctx: &Context, params: &serde_json::Value) -> Result<bool, CoreError>;

    /// 返回此 Handler 对应的 Condition 名称
    fn name(&self) -> &str;
}

// ─── Hook Handler ───────────────────────────────────────────

/// 钩子处理器 trait — 所有 Hook 扩展必须实现此接口
#[async_trait]
pub trait HookHandler: Send + Sync {
    /// 执行钩子逻辑
    ///
    /// # Arguments
    /// * `ctx` - 请求上下文（可被 BeforeRoute 钩子改写）
    ///
    /// # Returns
    /// 可能被改写的上下文
    async fn handle(&self, ctx: Context) -> Result<Context, CoreError>;

    /// 返回此 Handler 的名称
    fn name(&self) -> &str;

    /// 返回此 Handler 的触发阶段
    fn phase(&self) -> HookPhase;

    /// 返回此 Handler 的优先级
    fn priority(&self) -> i32;
}

// ─── Extension Registry ────────────────────────────────────

/// 扩展注册表 — 管理所有 Action / Condition / Hook 扩展
///
/// 扩展在启动时注册，运行时查询和调用。
/// 注册表本身是线程安全的，通过 Arc 共享。
pub struct ExtensionRegistry {
    /// Action 处理器注册表：action_name → handler
    action_handlers: HashMap<String, Arc<dyn ActionHandler>>,

    /// Condition 处理器注册表：condition_name → handler
    condition_handlers: HashMap<String, Arc<dyn ConditionHandler>>,

    /// Hook 处理器注册表，按阶段分组
    hooks_before: Vec<Arc<dyn HookHandler>>,
    hooks_after: Vec<Arc<dyn HookHandler>>,
    hooks_error: Vec<Arc<dyn HookHandler>>,
}

impl ExtensionRegistry {
    /// 创建空的扩展注册表
    pub fn new() -> Self {
        Self {
            action_handlers: HashMap::new(),
            condition_handlers: HashMap::new(),
            hooks_before: Vec::new(),
            hooks_after: Vec::new(),
            hooks_error: Vec::new(),
        }
    }

    // ─── Action 注册与查询 ──────────────────────────────────

    /// 注册 Action 处理器
    ///
    /// 如果同名 Action 已注册，返回错误。
    pub fn register_action(&mut self, handler: Arc<dyn ActionHandler>) -> Result<(), CoreError> {
        let name = handler.name().to_string();
        if self.action_handlers.contains_key(&name) {
            return Err(CoreError::ExtensionError(format!(
                "Action '{}' already registered",
                name
            )));
        }
        tracing::info!(action = %name, "Registered action handler");
        self.action_handlers.insert(name, handler);
        Ok(())
    }

    /// 查询 Action 处理器
    pub fn get_action_handler(&self, action_name: &str) -> Option<Arc<dyn ActionHandler>> {
        self.action_handlers.get(action_name).cloned()
    }

    /// 列出所有已注册的 Action 名称
    pub fn list_actions(&self) -> Vec<String> {
        self.action_handlers.keys().cloned().collect()
    }

    // ─── Condition 注册与查询 ───────────────────────────────

    /// 注册 Condition 处理器
    pub fn register_condition(&mut self, handler: Arc<dyn ConditionHandler>) -> Result<(), CoreError> {
        let name = handler.name().to_string();
        if self.condition_handlers.contains_key(&name) {
            return Err(CoreError::ExtensionError(format!(
                "Condition '{}' already registered",
                name
            )));
        }
        tracing::info!(condition = %name, "Registered condition handler");
        self.condition_handlers.insert(name, handler);
        Ok(())
    }

    /// 查询 Condition 处理器
    pub fn get_condition_handler(&self, condition_name: &str) -> Option<Arc<dyn ConditionHandler>> {
        self.condition_handlers.get(condition_name).cloned()
    }

    // ─── Hook 注册与查询 ────────────────────────────────────

    /// 注册 Hook 处理器
    pub fn register_hook(&mut self, handler: Arc<dyn HookHandler>) -> Result<(), CoreError> {
        let name = handler.name().to_string();
        tracing::info!(hook = %name, phase = ?handler.phase(), priority = handler.priority(), "Registered hook handler");
        match handler.phase() {
            HookPhase::BeforeRoute => self.hooks_before.push(handler),
            HookPhase::AfterRoute => self.hooks_after.push(handler),
            HookPhase::OnError => self.hooks_error.push(handler),
        }
        Ok(())
    }

    /// 获取 BeforeRoute 钩子列表（已按优先级排序）
    pub fn get_before_hooks(&self) -> Vec<Arc<dyn HookHandler>> {
        let mut hooks = self.hooks_before.clone();
        hooks.sort_by_key(|b| std::cmp::Reverse(b.priority()));
        hooks
    }

    /// 获取 AfterRoute 钩子列表（已按优先级排序）
    pub fn get_after_hooks(&self) -> Vec<Arc<dyn HookHandler>> {
        let mut hooks = self.hooks_after.clone();
        hooks.sort_by_key(|b| std::cmp::Reverse(b.priority()));
        hooks
    }

    /// 获取 OnError 钩子列表（已按优先级排序）
    pub fn get_error_hooks(&self) -> Vec<Arc<dyn HookHandler>> {
        let mut hooks = self.hooks_error.clone();
        hooks.sort_by_key(|b| std::cmp::Reverse(b.priority()));
        hooks
    }

    // ─── Clone (Phase 3) ─────────────────────────────────────

    /// 获取克隆引用（用于子扩展如 Workflow）
    pub fn clone_inner(&self) -> Self {
        Self {
            action_handlers: self.action_handlers.clone(),
            condition_handlers: self.condition_handlers.clone(),
            hooks_before: self.hooks_before.clone(),
            hooks_after: self.hooks_after.clone(),
            hooks_error: self.hooks_error.clone(),
        }
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{Action, Identity, IdentityType};

    /// 测试用 Action Handler
    struct TestActionHandler {
        name: String,
    }

    #[async_trait]
    impl ActionHandler for TestActionHandler {
        async fn execute(&self, _ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
            Ok(ActionResult::Json(serde_json::json!({
                "action": self.name,
                "params": target.params,
            })))
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    /// 测试用 Condition Handler
    struct TestConditionHandler;

    #[async_trait]
    impl ConditionHandler for TestConditionHandler {
        async fn evaluate(&self, ctx: &Context, params: &serde_json::Value) -> Result<bool, CoreError> {
            let target_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ctx.identity.identity_type == IdentityType::Human && target_type == "human")
        }

        fn name(&self) -> &str {
            "test-condition"
        }
    }

    #[tokio::test]
    async fn test_register_action() {
        let mut registry = ExtensionRegistry::new();
        let handler = Arc::new(TestActionHandler {
            name: "test-action".to_string(),
        });
        assert!(registry.register_action(handler).is_ok());
        assert!(registry.get_action_handler("test-action").is_some());
    }

    #[tokio::test]
    async fn test_duplicate_action_registration() {
        let mut registry = ExtensionRegistry::new();
        let handler1 = Arc::new(TestActionHandler {
            name: "test-action".to_string(),
        });
        let handler2 = Arc::new(TestActionHandler {
            name: "test-action".to_string(),
        });
        assert!(registry.register_action(handler1).is_ok());
        assert!(registry.register_action(handler2).is_err());
    }

    #[tokio::test]
    async fn test_list_actions() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register_action(Arc::new(TestActionHandler {
                name: "action-a".to_string(),
            }))
            .unwrap();
        registry
            .register_action(Arc::new(TestActionHandler {
                name: "action-b".to_string(),
            }))
            .unwrap();
        let actions = registry.list_actions();
        assert_eq!(actions.len(), 2);
    }

    #[tokio::test]
    async fn test_execute_action() {
        let mut registry = ExtensionRegistry::new();
        let handler = Arc::new(TestActionHandler {
            name: "test-action".to_string(),
        });
        registry.register_action(handler).unwrap();

        let handler = registry.get_action_handler("test-action").unwrap();
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("test-action".to_string()),
            params: serde_json::json!({"key": "value"}),
        };
        let result = handler.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["action"], "test-action");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_condition_handler() {
        let mut registry = ExtensionRegistry::new();
        registry.register_condition(Arc::new(TestConditionHandler)).unwrap();

        let handler = registry.get_condition_handler("test-condition").unwrap();
        let ctx = Context::from_request(None, None); // Default is Human
        let result = handler
            .evaluate(&ctx, &serde_json::json!({"type": "human"}))
            .await
            .unwrap();
        assert!(result);
    }
}
