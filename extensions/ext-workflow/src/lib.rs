//! # ext-workflow — 多步编排 Action 扩展
//!
//! 实现 Workflow Action，支持链接多个子 Action 依次执行。
//! 作为 Extension 注册到 Registry，核心不改。
//!
//! ## Workflow 参数格式
//! ```json
//! {
//!   "steps": [
//!     {
//!       "action": "webhook",
//!       "params": {"url": "https://hook.example.com/notify"},
//!       "continue_on_error": false
//!     },
//!     {
//!       "action": "redirect",
//!       "params": {"url": "https://final.example.com"}
//!     }
//!   ],
//!   "error_step": {
//!     "action": "json_data",
//!     "params": {"data": {"error": "workflow failed"}}
//!   }
//! }
//! ```

use std::sync::Arc;
use async_trait::async_trait;
use openlink_core::{
    ActionHandler, ExtensionRegistry, Context, CoreError,
    ActionResult, Target,
};
use serde::{Deserialize, Serialize};

// ─── Workflow Action ────────────────────────────────────────

/// 工作流步骤定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Action 类型
    pub action: String,
    /// Action 参数
    #[serde(default)]
    pub params: serde_json::Value,
    /// 失败后是否继续
    #[serde(default)]
    pub continue_on_error: bool,
}

/// 工作流定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// 执行步骤
    pub steps: Vec<WorkflowStep>,
    /// 错误处理步骤（可选）
    #[serde(default)]
    pub error_step: Option<WorkflowStep>,
    /// 超时时间（毫秒）
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    30000 // 30 秒默认超时
}

/// 工作流 Action 处理器
pub struct WorkflowAction {
    /// Action 注册表引用（用于执行子 Action）
    registry: Arc<openlink_core::ExtensionRegistry>,
}

impl WorkflowAction {
    pub fn new(registry: Arc<openlink_core::ExtensionRegistry>) -> Self {
        Self { registry }
    }

    /// 解析工作流定义
    fn parse_workflow(params: &serde_json::Value) -> Result<WorkflowDefinition, CoreError> {
        serde_json::from_value(params.clone())
            .map_err(|e| CoreError::ExtensionError(format!("Invalid workflow definition: {}", e)))
    }

    /// 执行单个步骤
    async fn execute_step(
        &self,
        ctx: &Context,
        step: &WorkflowStep,
    ) -> Result<ActionResult, CoreError> {
        let target = Target {
            action: openlink_core::Action::Custom(step.action.clone()),
            params: step.params.clone(),
        };

        // 查找 Action Handler
        let handler = self.registry.get_action_handler(&step.action)
            .ok_or_else(|| CoreError::ExtensionError(format!(
                "Action handler not found: {}", step.action
            )))?;

        handler.execute(ctx, &target).await
    }

    /// 执行工作流
    async fn execute_workflow(
        &self,
        ctx: &Context,
        definition: &WorkflowDefinition,
    ) -> Result<ActionResult, CoreError> {
        let start = std::time::Instant::now();

        for (idx, step) in definition.steps.iter().enumerate() {
            // 检查超时
            if start.elapsed().as_millis() as u64 > definition.timeout_ms {
                return Err(CoreError::ExtensionError("Workflow timeout".to_string()));
            }

            tracing::info!(step = idx, action = %step.action, "Executing workflow step");

            match self.execute_step(ctx, step).await {
                Ok(_result) => {
                    tracing::debug!(step = idx, "Step completed successfully");
                    // 继续执行下一步
                }
                Err(e) => {
                    tracing::warn!(step = idx, error = %e, "Step failed");
                    if step.continue_on_error {
                        tracing::info!(step = idx, "Continuing despite error");
                        continue;
                    }

                    // 执行错误处理步骤
                    if let Some(error_step) = &definition.error_step {
                        tracing::info!("Executing error step");
                        return self.execute_step(ctx, error_step).await;
                    }

                    return Err(e);
                }
            }
        }

        // 返回最后一个结果的摘要
        Ok(ActionResult::Json(serde_json::json!({
            "type": "workflow_complete",
            "steps_executed": definition.steps.len(),
            "elapsed_ms": start.elapsed().as_millis()
        })))
    }
}

#[async_trait]
impl ActionHandler for WorkflowAction {
    async fn execute(
        &self,
        ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let definition = Self::parse_workflow(&target.params)?;
        self.execute_workflow(ctx, &definition).await
    }

    fn name(&self) -> &str {
        "workflow"
    }
}

/// 注册工作流扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    let workflow = WorkflowAction::new(Arc::new(registry.clone_inner()));
    registry.register_action(Arc::new(workflow))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::ExtensionRegistry;

    #[test]
    fn test_parse_workflow_basic() {
        let params = serde_json::json!({
            "steps": [
                {"action": "webhook", "params": {"url": "https://hook.example.com"}},
                {"action": "redirect", "params": {"url": "https://example.com"}}
            ]
        });

        let workflow = WorkflowAction::parse_workflow(&params).unwrap();
        assert_eq!(workflow.steps.len(), 2);
        assert_eq!(workflow.steps[0].action, "webhook");
        assert_eq!(workflow.steps[1].action, "redirect");
    }

    #[test]
    fn test_parse_workflow_with_error_step() {
        let params = serde_json::json!({
            "steps": [
                {"action": "webhook", "params": {"url": "https://hook.example.com"}}
            ],
            "error_step": {
                "action": "json_data",
                "params": {"data": {"error": "failed"}}
            }
        });

        let workflow = WorkflowAction::parse_workflow(&params).unwrap();
        assert_eq!(workflow.steps.len(), 1);
        assert!(workflow.error_step.is_some());
        assert_eq!(workflow.error_step.unwrap().action, "json_data");
    }

    #[test]
    fn test_parse_workflow_with_timeout() {
        let params = serde_json::json!({
            "steps": [],
            "timeout_ms": 5000
        });

        let workflow = WorkflowAction::parse_workflow(&params).unwrap();
        assert_eq!(workflow.timeout_ms, 5000);
    }

    #[test]
    fn test_parse_workflow_invalid() {
        let params = serde_json::json!({
            "not_steps": "invalid"
        });

        assert!(WorkflowAction::parse_workflow(&params).is_err());
    }

    #[test]
    fn test_workflow_step_serialization() {
        let step = WorkflowStep {
            action: "webhook".to_string(),
            params: serde_json::json!({"url": "https://example.com"}),
            continue_on_error: true,
        };

        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("\"action\":\"webhook\""));
        assert!(json.contains("\"continue_on_error\":true"));
    }

    #[test]
    fn test_workflow_definition_serialization() {
        let workflow = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    action: "redirect".to_string(),
                    params: serde_json::json!({"url": "https://example.com"}),
                    continue_on_error: false,
                },
            ],
            error_step: None,
            timeout_ms: 10000,
        };

        let json = serde_json::to_string(&workflow).unwrap();
        let parsed: WorkflowDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.steps.len(), 1);
        assert_eq!(parsed.timeout_ms, 10000);
    }
}
