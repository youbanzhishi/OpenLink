//! # DAG 执行引擎
//!
//! 按拓扑顺序执行 DAG，支持并行执行无依赖的节点。

#[allow(unused_imports)]
use crate::dag::{Dag, DagError, DagNode, EdgeCondition, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 节点执行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    /// 等待执行
    Pending,
    /// 正在执行
    Running,
    /// 执行成功
    Success,
    /// 执行失败
    Failed(String),
    /// 跳过（条件不满足）
    Skipped,
    /// 超时
    Timeout,
}

/// 节点执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    /// 节点 ID
    pub node_id: NodeId,
    /// 执行状态
    pub status: ExecutionStatus,
    /// 执行结果数据
    pub output: serde_json::Value,
    /// 执行时间（毫秒）
    pub elapsed_ms: u64,
    /// 重试次数
    pub retry_count: u32,
}

/// DAG 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// DAG ID
    pub dag_id: String,
    /// 整体状态
    pub status: ExecutionStatus,
    /// 各节点结果
    pub node_results: HashMap<NodeId, NodeResult>,
    /// 总执行时间（毫秒）
    pub total_elapsed_ms: u64,
}

/// 任务执行器 trait（由外部实现，如调用远程 Agent）
#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    /// 执行单个任务节点
    async fn execute_task(
        &self,
        agent_id: &str,
        task_type: &str,
        params: &serde_json::Value,
        inputs: &HashMap<NodeId, serde_json::Value>,
    ) -> Result<serde_json::Value, String>;
}

/// 简单任务执行器（用于测试）
pub struct SimpleTaskExecutor {
    /// 预设结果映射：(agent_id, task_type) → result
    results: Arc<RwLock<HashMap<(String, String), serde_json::Value>>>,
}

impl SimpleTaskExecutor {
    pub fn new() -> Self {
        Self {
            results: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn preset_result(&self, agent_id: &str, task_type: &str, result: serde_json::Value) {
        let mut results = self.results.write().await;
        results.insert((agent_id.to_string(), task_type.to_string()), result);
    }
}

impl Default for SimpleTaskExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TaskExecutor for SimpleTaskExecutor {
    async fn execute_task(
        &self,
        agent_id: &str,
        task_type: &str,
        _params: &serde_json::Value,
        _inputs: &HashMap<NodeId, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let results = self.results.read().await;
        if let Some(result) = results.get(&(agent_id.to_string(), task_type.to_string())) {
            Ok(result.clone())
        } else {
            Ok(serde_json::json!({
                "agent": agent_id,
                "task": task_type,
                "status": "completed"
            }))
        }
    }
}

/// DAG 执行引擎
pub struct DagExecutor {
    executor: Arc<dyn TaskExecutor>,
}

impl DagExecutor {
    /// 创建执行引擎
    pub fn new(executor: Arc<dyn TaskExecutor>) -> Self {
        Self { executor }
    }

    /// 执行 DAG
    pub async fn execute(&self, dag: &Dag) -> Result<ExecutionResult, DagError> {
        // 验证 DAG
        dag.validate()?;

        let start = std::time::Instant::now();
        let topo_order = dag.topological_sort();

        let mut node_results: HashMap<NodeId, NodeResult> = HashMap::new();
        let mut completed: HashSet<NodeId> = HashSet::new();
        let mut failed: HashSet<NodeId> = HashSet::new();
        let mut node_outputs: HashMap<NodeId, serde_json::Value> = HashMap::new();

        // 按拓扑顺序执行
        for node_id in &topo_order {
            let node = dag.get_node(node_id).unwrap();

            // 检查前置条件
            if !self.can_execute(dag, node_id, &completed, &failed) {
                node_results.insert(
                    node_id.clone(),
                    NodeResult {
                        node_id: node_id.clone(),
                        status: ExecutionStatus::Skipped,
                        output: serde_json::Value::Null,
                        elapsed_ms: 0,
                        retry_count: 0,
                    },
                );
                continue;
            }

            // 收集前驱节点的输出
            let inputs: HashMap<NodeId, serde_json::Value> = dag
                .predecessors(node_id)
                .iter()
                .filter_map(|pred| node_outputs.get(&pred.id).map(|v| (pred.id.clone(), v.clone())))
                .collect();

            // 执行任务（带重试）
            let node_start = std::time::Instant::now();
            let mut retry_count = 0;
            let mut last_error = String::new();

            loop {
                match self
                    .executor
                    .execute_task(&node.agent_id, &node.task_type, &node.params, &inputs)
                    .await
                {
                    Ok(output) => {
                        let elapsed = node_start.elapsed().as_millis() as u64;
                        node_results.insert(
                            node_id.clone(),
                            NodeResult {
                                node_id: node_id.clone(),
                                status: ExecutionStatus::Success,
                                output: output.clone(),
                                elapsed_ms: elapsed,
                                retry_count,
                            },
                        );
                        node_outputs.insert(node_id.clone(), output);
                        completed.insert(node_id.clone());

                        let elapsed_ms = node_start.elapsed().as_millis() as u64;
                        tracing::info!(
                            node = %node_id,
                            agent = %node.agent_id,
                            elapsed_ms,
                            "Node completed successfully"
                        );
                        break;
                    }
                    Err(e) => {
                        retry_count += 1;
                        last_error = e.clone();

                        if retry_count <= node.max_retries {
                            tracing::warn!(
                                node = %node_id,
                                retry = retry_count,
                                error = %e,
                                "Node failed, retrying"
                            );
                            continue;
                        }

                        let elapsed = node_start.elapsed().as_millis() as u64;
                        node_results.insert(
                            node_id.clone(),
                            NodeResult {
                                node_id: node_id.clone(),
                                status: ExecutionStatus::Failed(e.clone()),
                                output: serde_json::Value::Null,
                                elapsed_ms: elapsed,
                                retry_count,
                            },
                        );

                        if node.continue_on_error {
                            tracing::warn!(
                                node = %node_id,
                                error = %e,
                                "Node failed but continue_on_error=true"
                            );
                            completed.insert(node_id.clone());
                        } else {
                            failed.insert(node_id.clone());
                            tracing::error!(node = %node_id, error = %e, "Node failed");
                        }
                        break;
                    }
                }
            }
        }

        let total_elapsed = start.elapsed().as_millis() as u64;
        let overall_status = if failed.is_empty() {
            ExecutionStatus::Success
        } else {
            ExecutionStatus::Failed(format!("Nodes failed: {:?}", failed.iter().collect::<Vec<_>>()))
        };

        Ok(ExecutionResult {
            dag_id: dag.id.clone(),
            status: overall_status,
            node_results,
            total_elapsed_ms: total_elapsed,
        })
    }

    /// 检查节点是否可以执行
    fn can_execute(&self, dag: &Dag, node_id: &str, completed: &HashSet<NodeId>, failed: &HashSet<NodeId>) -> bool {
        let predecessors = dag.predecessors(node_id);
        if predecessors.is_empty() {
            return true; // 入口节点
        }

        // 获取所有入边
        let incoming_edges: Vec<_> = dag.edges.iter().filter(|e| e.to == node_id).collect();

        for edge in &incoming_edges {
            let pred_completed = completed.contains(&edge.from);
            let pred_failed = failed.contains(&edge.from);

            match &edge.condition {
                None => {
                    // 无条件边：前驱必须成功完成
                    if !pred_completed || pred_failed {
                        return false;
                    }
                }
                Some(EdgeCondition::OnSuccess) => {
                    if !pred_completed || pred_failed {
                        return false;
                    }
                }
                Some(EdgeCondition::OnFailure) => {
                    if !pred_failed {
                        return false;
                    }
                }
                Some(EdgeCondition::Custom(_)) => {
                    // 自定义条件简化处理：前驱必须完成
                    if !pred_completed && !pred_failed {
                        return false;
                    }
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_dag() -> Dag {
        let mut dag = Dag::new("test", "Test DAG");
        dag.add_node(DagNode {
            id: "a".to_string(),
            name: "Step A".to_string(),
            agent_id: "agent-1".to_string(),
            task_type: "process".to_string(),
            params: serde_json::json!({"input": "data"}),
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });
        dag.add_node(DagNode {
            id: "b".to_string(),
            name: "Step B".to_string(),
            agent_id: "agent-2".to_string(),
            task_type: "process".to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });
        dag.add_edge("a", "b");
        dag
    }

    #[tokio::test]
    async fn test_execute_simple_dag() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let engine = DagExecutor::new(executor);

        let dag = make_simple_dag();
        let result = engine.execute(&dag).await.unwrap();

        assert_eq!(result.status, ExecutionStatus::Success);
        assert_eq!(result.node_results.len(), 2);
        assert_eq!(result.node_results["a"].status, ExecutionStatus::Success);
        assert_eq!(result.node_results["b"].status, ExecutionStatus::Success);
    }

    #[tokio::test]
    async fn test_execute_diamond_dag() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let engine = DagExecutor::new(executor);

        let mut dag = Dag::new("diamond", "Diamond DAG");
        for id in &["a", "b", "c", "d"] {
            dag.add_node(DagNode {
                id: id.to_string(),
                name: format!("Step {}", id),
                agent_id: "agent-1".to_string(),
                task_type: "process".to_string(),
                params: serde_json::Value::Null,
                timeout_ms: 0,
                continue_on_error: false,
                max_retries: 0,
            });
        }
        dag.add_edge("a", "b");
        dag.add_edge("a", "c");
        dag.add_edge("b", "d");
        dag.add_edge("c", "d");

        let result = engine.execute(&dag).await.unwrap();
        assert_eq!(result.status, ExecutionStatus::Success);
        assert_eq!(result.node_results.len(), 4);
    }

    #[tokio::test]
    async fn test_execute_invalid_dag() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let engine = DagExecutor::new(executor);

        let mut dag = Dag::new("cycle", "Cycle DAG");
        dag.add_node(DagNode {
            id: "a".to_string(),
            name: "A".to_string(),
            agent_id: "agent-1".to_string(),
            task_type: "process".to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });
        dag.add_node(DagNode {
            id: "b".to_string(),
            name: "B".to_string(),
            agent_id: "agent-2".to_string(),
            task_type: "process".to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });
        dag.add_edge("a", "b");
        dag.add_edge("b", "a");

        assert!(engine.execute(&dag).await.is_err());
    }
}
