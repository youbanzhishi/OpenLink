//! # ext-orchestrator — 编排引擎扩展
//!
//! 实现 Orchestrator Action，支持多 Agent 任务编排（DAG 定义+执行）。
//! 注册到 Extension Registry。
//!
//! ## 功能
//! - **Action**: `orchestrate` — 执行 DAG 编排
//! - **Action**: `orchestrate_template` — 从模板创建并执行编排
//! - **Action**: `orchestrate_parallel` — 并行执行 DAG 编排
//! - **Action**: `list_templates` — 列出可用工作流模板
//! - **Action**: `validate_dag` — 验证 DAG 定义
//! - 支持结果聚合和回调通知
//!
//! ## 用法示例
//! ```json
//! {
//!   "action": "orchestrate",
//!   "params": {
//!     "dag": {
//!       "id": "my-pipeline",
//!       "name": "My Pipeline",
//!       "nodes": [...],
//!       "edges": [...]
//!     },
//!     "aggregation": "merge"
//!   }
//! }
//! ```

use std::sync::Arc;
use async_trait::async_trait;
use openlink_core::{
    ActionHandler, ExtensionRegistry, CoreError,
    ActionResult, Context, Target,
};
use openlink_orchestrator::{
    Dag, DagExecutor, DagNode, EdgeCondition,
    ExecutionStatus, ExecutionResult, ResultAggregator, AggregationStrategy,
    TemplateRegistry, ParallelDagExecutor, ParallelConfig,
    SimpleTaskExecutor,
};
use tokio::sync::RwLock;

/// 编排 Action
pub struct OrchestrateAction {
    executor: Arc<DagExecutor>,
    parallel_executor: Arc<ParallelDagExecutor>,
    aggregator: Arc<RwLock<ResultAggregator>>,
    templates: Arc<RwLock<TemplateRegistry>>,
}

impl OrchestrateAction {
    pub fn new(executor: Arc<DagExecutor>) -> Self {
        let parallel_executor = Arc::new(ParallelDagExecutor::new(
            Arc::new(SimpleTaskExecutor::new())
        ));
        Self {
            executor,
            parallel_executor,
            aggregator: Arc::new(RwLock::new(ResultAggregator::new())),
            templates: Arc::new(RwLock::new(TemplateRegistry::new())),
        }
    }

    /// 创建带并行执行器的编排 Action
    pub fn with_parallel(
        executor: Arc<DagExecutor>,
        parallel_executor: Arc<ParallelDagExecutor>,
    ) -> Self {
        Self {
            executor,
            parallel_executor,
            aggregator: Arc::new(RwLock::new(ResultAggregator::new())),
            templates: Arc::new(RwLock::new(TemplateRegistry::new())),
        }
    }

    /// 设置结果聚合器
    pub fn with_aggregator(mut self, aggregator: ResultAggregator) -> Self {
        self.aggregator = Arc::new(RwLock::new(aggregator));
        self
    }

    /// 解析 DAG 参数
    fn parse_dag(params: &serde_json::Value) -> Result<Dag, CoreError> {
        serde_json::from_value(params.clone())
            .map_err(|e| CoreError::ExtensionError(format!("Invalid DAG definition: {}", e)))
    }

    /// 解析聚合策略
    fn parse_aggregation_strategy(params: &serde_json::Value) -> AggregationStrategy {
        match params.get("aggregation").and_then(|v| v.as_str()).unwrap_or("merge") {
            "last" => AggregationStrategy::Last,
            "collect" => AggregationStrategy::Collect,
            "merge" => AggregationStrategy::Merge,
            custom => AggregationStrategy::Custom(custom.to_string()),
        }
    }
}

#[async_trait]
impl ActionHandler for OrchestrateAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        // 检查是否使用模板
        if let Some(template_id) = target.params.get("template_id").and_then(|v| v.as_str()) {
            return self.execute_template(template_id, &target.params).await;
        }

        // 检查是否使用并行执行
        let parallel = target.params.get("parallel")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 直接执行 DAG
        let dag_value = target.params.get("dag")
            .ok_or_else(|| CoreError::InvalidInput("'dag' parameter is required".to_string()))?;

        let dag = Self::parse_dag(dag_value)?;

        let result = if parallel {
            self.parallel_executor.execute(&dag).await
                .map_err(|e| CoreError::ExtensionError(format!("Parallel DAG execution failed: {}", e)))?
        } else {
            self.executor.execute(&dag).await
                .map_err(|e| CoreError::ExtensionError(format!("DAG execution failed: {}", e)))?
        };

        // 聚合结果
        let strategy = Self::parse_aggregation_strategy(&target.params);
        let aggregator = self.aggregator.read().await;
        let aggregated = aggregator.aggregate(&result, &strategy);

        tracing::info!(
            dag_id = %result.dag_id,
            status = ?result.status,
            total_elapsed_ms = result.total_elapsed_ms,
            parallel = parallel,
            "DAG execution completed"
        );

        // 合并执行结果和聚合结果
        Ok(ActionResult::Json(serde_json::json!({
            "execution": {
                "dag_id": result.dag_id,
                "status": format!("{:?}", result.status).to_lowercase(),
                "total_elapsed_ms": result.total_elapsed_ms,
                "node_results": result.node_results,
            },
            "aggregation": {
                "strategy": format!("{:?}", aggregated.strategy).to_lowercase(),
                "result": aggregated.result,
                "node_count": aggregated.node_count,
                "success_count": aggregated.success_count,
                "failure_count": aggregated.failure_count,
            },
        })))
    }

    fn name(&self) -> &'static str {
        "orchestrate"
    }
}

impl OrchestrateAction {
    /// 从模板执行
    async fn execute_template(
        &self,
        template_id: &str,
        params: &serde_json::Value,
    ) -> Result<ActionResult, CoreError> {
        let templates = self.templates.read().await;
        let template = templates.get(template_id)
            .ok_or_else(|| CoreError::InvalidInput(
                format!("Template '{}' not found", template_id)
            ))?;

        tracing::info!(
            template_id = %template_id,
            template_name = %template.name,
            "Executing from template"
        );

        // 模板执行逻辑（简化版：返回模板信息）
        Ok(ActionResult::Json(serde_json::json!({
            "status": "template_resolved",
            "template_id": template_id,
            "template_name": template.name,
            "message": "Template resolved. Use the template parameters to construct a DAG.",
        })))
    }
}

/// 模板列表 Action（Phase 6 新增）
pub struct ListTemplatesAction {
    templates: Arc<RwLock<TemplateRegistry>>,
}

impl ListTemplatesAction {
    pub fn new(templates: Arc<RwLock<TemplateRegistry>>) -> Self {
        Self { templates }
    }
}

#[async_trait]
impl ActionHandler for ListTemplatesAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let templates = self.templates.read().await;

        let category = target.params.get("category")
            .and_then(|v| v.as_str());

        let template_list: Vec<serde_json::Value> = if let Some(cat) = category {
            templates.list_by_category(cat)
        } else {
            templates.list()
        }.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "description": t.description,
                "category": t.category,
                "parameters": t.parameters.iter().map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "type": p.param_type,
                        "required": p.required,
                        "description": p.description,
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect();

        Ok(ActionResult::Json(serde_json::json!({
            "templates": template_list,
            "count": template_list.len(),
        })))
    }

    fn name(&self) -> &'static str {
        "list_templates"
    }
}

/// DAG 验证 Action（Phase 6 新增）
pub struct ValidateDagAction;

impl ValidateDagAction {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ActionHandler for ValidateDagAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let dag_value = target.params.get("dag")
            .ok_or_else(|| CoreError::InvalidInput("'dag' parameter is required".to_string()))?;

        let dag = OrchestrateAction::parse_dag(dag_value)?;

        match dag.validate() {
            Ok(()) => Ok(ActionResult::Json(serde_json::json!({
                "valid": true,
                "dag_id": dag.id,
                "node_count": dag.nodes.len(),
                "edge_count": dag.edges.len(),
                "entry_nodes": dag.entry_nodes().iter().map(|n| &n.id).collect::<Vec<_>>(),
                "exit_nodes": dag.exit_nodes().iter().map(|n| &n.id).collect::<Vec<_>>(),
            }))),
            Err(e) => Ok(ActionResult::Json(serde_json::json!({
                "valid": false,
                "dag_id": dag.id,
                "error": format!("{}", e),
            }))),
        }
    }

    fn name(&self) -> &'static str {
        "validate_dag"
    }
}

/// 注册编排扩展到 Extension Registry
pub fn register(
    registry: &mut ExtensionRegistry,
    executor: Arc<DagExecutor>,
) -> Result<(), CoreError> {
    let templates = Arc::new(RwLock::new(TemplateRegistry::new()));

    let action = OrchestrateAction::new(executor);
    registry.register_action(Arc::new(action))?;

    registry.register_action(Arc::new(ListTemplatesAction::new(templates.clone())))?;
    registry.register_action(Arc::new(ValidateDagAction::new()))?;

    Ok(())
}

/// 注册编排扩展（带并行执行器）
pub fn register_with_parallel(
    registry: &mut ExtensionRegistry,
    executor: Arc<DagExecutor>,
    parallel_executor: Arc<ParallelDagExecutor>,
) -> Result<(), CoreError> {
    let templates = Arc::new(RwLock::new(TemplateRegistry::new()));

    let action = OrchestrateAction::with_parallel(executor, parallel_executor);
    registry.register_action(Arc::new(action))?;

    registry.register_action(Arc::new(ListTemplatesAction::new(templates.clone())))?;
    registry.register_action(Arc::new(ValidateDagAction::new()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::Action;

    fn make_simple_dag_json() -> serde_json::Value {
        serde_json::json!({
            "id": "test-dag",
            "name": "Test DAG",
            "nodes": [
                {
                    "id": "step_0",
                    "name": "Step A",
                    "agent_id": "agent-1",
                    "task_type": "process",
                    "params": null,
                    "timeout_ms": 0,
                    "continue_on_error": false,
                    "max_retries": 0
                },
                {
                    "id": "step_1",
                    "name": "Step B",
                    "agent_id": "agent-2",
                    "task_type": "process",
                    "params": null,
                    "timeout_ms": 0,
                    "continue_on_error": false,
                    "max_retries": 0
                }
            ],
            "edges": [
                { "from": "step_0", "to": "step_1" }
            ],
            "global_timeout_ms": 300000
        })
    }

    #[tokio::test]
    async fn test_orchestrate_action() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let dag_executor = Arc::new(DagExecutor::new(executor));
        let action = OrchestrateAction::new(dag_executor);

        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("orchestrate".to_string()),
            params: serde_json::json!({
                "dag": make_simple_dag_json(),
                "aggregation": "merge"
            }),
        };

        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["execution"]["dag_id"], "test-dag");
                assert_eq!(val["execution"]["status"], "success");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_orchestrate_parallel() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let dag_executor = Arc::new(DagExecutor::new(executor));
        let action = OrchestrateAction::new(dag_executor);

        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("orchestrate".to_string()),
            params: serde_json::json!({
                "dag": make_simple_dag_json(),
                "aggregation": "collect",
                "parallel": true
            }),
        };

        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["execution"]["dag_id"], "test-dag");
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_orchestrate_missing_dag() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let dag_executor = Arc::new(DagExecutor::new(executor));
        let action = OrchestrateAction::new(dag_executor);

        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("orchestrate".to_string()),
            params: serde_json::json!({}),
        };

        let result = action.execute(&ctx, &target).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_dag() {
        let dag_json = make_simple_dag_json();
        let dag = OrchestrateAction::parse_dag(&dag_json).unwrap();
        assert_eq!(dag.id, "test-dag");
        assert_eq!(dag.nodes.len(), 2);
        assert_eq!(dag.edges.len(), 1);
    }

    #[test]
    fn test_aggregation_strategy_parsing() {
        let params = serde_json::json!({"aggregation": "collect"});
        let strategy = OrchestrateAction::parse_aggregation_strategy(&params);
        assert_eq!(strategy, AggregationStrategy::Collect);

        let params = serde_json::json!({"aggregation": "last"});
        let strategy = OrchestrateAction::parse_aggregation_strategy(&params);
        assert_eq!(strategy, AggregationStrategy::Last);
    }

    #[tokio::test]
    async fn test_validate_dag_action() {
        let action = ValidateDagAction::new();
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("validate_dag".to_string()),
            params: serde_json::json!({
                "dag": make_simple_dag_json()
            }),
        };

        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["valid"], true);
                assert_eq!(val["node_count"], 2);
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_list_templates_action() {
        let templates = Arc::new(RwLock::new(TemplateRegistry::new()));
        let action = ListTemplatesAction::new(templates);

        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("list_templates".to_string()),
            params: serde_json::json!({}),
        };

        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert!(val["count"].as_u64().unwrap() >= 3);
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_list_templates_by_category() {
        let templates = Arc::new(RwLock::new(TemplateRegistry::new()));
        let action = ListTemplatesAction::new(templates);

        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("list_templates".to_string()),
            params: serde_json::json!({"category": "parallel"}),
        };

        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                let count = val["count"].as_u64().unwrap();
                assert!(count >= 2); // fan-out-fan-in, map-reduce, parallel-merge
            }
            _ => panic!("Expected Json result"),
        }
    }
}
