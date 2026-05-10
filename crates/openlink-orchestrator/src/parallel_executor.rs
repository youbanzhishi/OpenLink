//! # 并行 DAG 执行器（Phase 6）
//!
//! 增强版 DAG 执行引擎，支持并行执行无依赖节点。
//! 使用拓扑层级（level）来识别可并行的节点。

use crate::dag::{Dag, DagError, DagNode, EdgeCondition, NodeId};
use crate::executor::{ExecutionResult, ExecutionStatus, NodeResult, TaskExecutor};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// 并行执行配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelConfig {
    /// 最大并行度，默认 8
    #[serde(default = "default_max_parallelism")]
    pub max_parallelism: usize,

    /// 节点执行超时（毫秒），0 = 无超时
    #[serde(default)]
    pub node_timeout_ms: u64,

    /// 是否在单个节点失败时中止整个 DAG
    #[serde(default = "default_true")]
    pub fail_fast: bool,
}

fn default_max_parallelism() -> usize {
    8
}
fn default_true() -> bool {
    true
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_parallelism: default_max_parallelism(),
            node_timeout_ms: 0,
            fail_fast: true,
        }
    }
}

/// 拓扑层级
type Level = usize;

/// 并行 DAG 执行器
pub struct ParallelDagExecutor {
    executor: Arc<dyn TaskExecutor>,
    config: ParallelConfig,
}

impl ParallelDagExecutor {
    pub fn new(executor: Arc<dyn TaskExecutor>) -> Self {
        Self::with_config(executor, ParallelConfig::default())
    }

    pub fn with_config(executor: Arc<dyn TaskExecutor>, config: ParallelConfig) -> Self {
        Self { executor, config }
    }

    /// 计算拓扑层级
    ///
    /// 同一层级的节点没有相互依赖，可以并行执行。
    fn compute_levels(dag: &Dag) -> HashMap<NodeId, Level> {
        let in_degree = dag.compute_in_degree();
        let adjacency = dag.build_adjacency();

        let mut levels: HashMap<NodeId, Level> = HashMap::new();
        let mut queue: VecDeque<NodeId> = VecDeque::new();

        // 入度为 0 的节点为 level 0
        for node in &dag.nodes {
            if in_degree.get(&node.id).copied().unwrap_or(0) == 0 {
                levels.insert(node.id.clone(), 0);
                queue.push_back(node.id.clone());
            }
        }

        // BFS 计算层级
        while let Some(node_id) = queue.pop_front() {
            let current_level = levels[&node_id];

            if let Some(neighbors) = adjacency.get(&node_id) {
                for neighbor in neighbors {
                    let neighbor_level = levels.get(neighbor).copied().unwrap_or(0);
                    let new_level = current_level + 1;
                    if new_level > neighbor_level {
                        levels.insert(neighbor.clone(), new_level);
                    }

                    // 重新检查入度
                    let new_in_degree: usize = dag
                        .edges
                        .iter()
                        .filter(|e| e.to == *neighbor && !levels.contains_key(&e.from))
                        .count();

                    if new_in_degree == 0 && !levels.contains_key(neighbor) {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        levels
    }

    /// 按层级分组节点
    fn group_by_levels(dag: &Dag, levels: &HashMap<NodeId, Level>) -> Vec<Vec<NodeId>> {
        if dag.nodes.is_empty() {
            return Vec::new();
        }

        let max_level = levels.values().copied().max().unwrap_or(0);
        let mut groups: Vec<Vec<NodeId>> = vec![Vec::new(); max_level + 1];

        for node in &dag.nodes {
            if let Some(&level) = levels.get(&node.id) {
                groups[level].push(node.id.clone());
            }
        }

        groups
    }

    /// 执行 DAG（并行版本）
    pub async fn execute(&self, dag: &Dag) -> Result<ExecutionResult, DagError> {
        dag.validate()?;

        let start = std::time::Instant::now();
        let levels = Self::compute_levels(dag);
        let level_groups = Self::group_by_levels(dag, &levels);

        let mut node_results: HashMap<NodeId, NodeResult> = HashMap::new();
        let mut completed: HashSet<NodeId> = HashSet::new();
        let mut failed: HashSet<NodeId> = HashSet::new();
        let mut node_outputs: HashMap<NodeId, serde_json::Value> = HashMap::new();

        for level_nodes in &level_groups {
            // 限制并行度
            let chunks = level_nodes.chunks(self.config.max_parallelism);

            for chunk in chunks {
                let mut handles = Vec::new();

                for node_id in chunk {
                    let node = dag.get_node(node_id).unwrap().clone();

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

                    // 收集前驱输出
                    let inputs: HashMap<NodeId, serde_json::Value> = dag
                        .predecessors(node_id)
                        .iter()
                        .filter_map(|pred| {
                            node_outputs
                                .get(&pred.id)
                                .map(|v| (pred.id.clone(), v.clone()))
                        })
                        .collect();

                    let executor = self.executor.clone();
                    let node_id_clone = node_id.clone();

                    let handle = tokio::spawn(async move {
                        let node_start = std::time::Instant::now();
                        let result = executor
                            .execute_task(&node.agent_id, &node.task_type, &node.params, &inputs)
                            .await;

                        let elapsed = node_start.elapsed().as_millis() as u64;

                        match result {
                            Ok(output) => (
                                node_id_clone,
                                ExecutionStatus::Success,
                                output,
                                elapsed,
                                0u32,
                            ),
                            Err(e) => (
                                node_id_clone,
                                ExecutionStatus::Failed(e),
                                serde_json::Value::Null,
                                elapsed,
                                0,
                            ),
                        }
                    });

                    handles.push(handle);
                }

                // 等待本批次完成
                for handle in handles {
                    match handle.await {
                        Ok((node_id, status, output, elapsed_ms, retry_count)) => {
                            let is_success = matches!(status, ExecutionStatus::Success);

                            node_results.insert(
                                node_id.clone(),
                                NodeResult {
                                    node_id: node_id.clone(),
                                    status: status.clone(),
                                    output: output.clone(),
                                    elapsed_ms,
                                    retry_count,
                                },
                            );

                            if is_success {
                                node_outputs.insert(node_id.clone(), output);
                                completed.insert(node_id.clone());
                            } else {
                                failed.insert(node_id.clone());
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Task panicked");
                        }
                    }
                }

                // 快速失败检查
                if self.config.fail_fast && !failed.is_empty() {
                    // 标记剩余节点为跳过
                    for node in &dag.nodes {
                        if !node_results.contains_key(&node.id) {
                            node_results.insert(
                                node.id.clone(),
                                NodeResult {
                                    node_id: node.id.clone(),
                                    status: ExecutionStatus::Skipped,
                                    output: serde_json::Value::Null,
                                    elapsed_ms: 0,
                                    retry_count: 0,
                                },
                            );
                        }
                    }

                    let total_elapsed = start.elapsed().as_millis() as u64;
                    return Ok(ExecutionResult {
                        dag_id: dag.id.clone(),
                        status: ExecutionStatus::Failed(format!(
                            "Fast fail: {} nodes failed",
                            failed.len()
                        )),
                        node_results,
                        total_elapsed_ms: total_elapsed,
                    });
                }
            }
        }

        let total_elapsed = start.elapsed().as_millis() as u64;
        let overall_status = if failed.is_empty() {
            ExecutionStatus::Success
        } else {
            ExecutionStatus::Failed(format!(
                "Nodes failed: {:?}",
                failed.iter().collect::<Vec<_>>()
            ))
        };

        Ok(ExecutionResult {
            dag_id: dag.id.clone(),
            status: overall_status,
            node_results,
            total_elapsed_ms: total_elapsed,
        })
    }

    /// 检查节点是否可以执行
    fn can_execute(
        &self,
        dag: &Dag,
        node_id: &str,
        completed: &HashSet<NodeId>,
        failed: &HashSet<NodeId>,
    ) -> bool {
        let predecessors = dag.predecessors(node_id);
        if predecessors.is_empty() {
            return true;
        }

        let incoming_edges: Vec<_> = dag.edges.iter().filter(|e| e.to == node_id).collect();

        for edge in &incoming_edges {
            let pred_completed = completed.contains(&edge.from);
            let pred_failed = failed.contains(&edge.from);

            match &edge.condition {
                None => {
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
    use crate::executor::SimpleTaskExecutor;

    fn make_diamond_dag() -> Dag {
        let mut dag = Dag::new("diamond", "Diamond DAG");
        for (id, name) in [
            ("a", "Start"),
            ("b", "Branch B"),
            ("c", "Branch C"),
            ("d", "End"),
        ] {
            dag.add_node(DagNode {
                id: id.to_string(),
                name: name.to_string(),
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
        dag
    }

    #[tokio::test]
    async fn test_parallel_diamond_dag() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let engine = ParallelDagExecutor::new(executor);

        let dag = make_diamond_dag();
        let result = engine.execute(&dag).await.unwrap();

        assert_eq!(result.status, ExecutionStatus::Success);
        assert_eq!(result.node_results.len(), 4);
        // b and c should have succeeded
        assert_eq!(result.node_results["b"].status, ExecutionStatus::Success);
        assert_eq!(result.node_results["c"].status, ExecutionStatus::Success);
    }

    #[tokio::test]
    async fn test_parallel_serial_dag() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let engine = ParallelDagExecutor::new(executor);

        let mut dag = Dag::new("serial", "Serial Pipeline");
        dag.add_node(DagNode {
            id: "a".to_string(),
            name: "Step A".to_string(),
            agent_id: "agent-1".to_string(),
            task_type: "process".to_string(),
            params: serde_json::Value::Null,
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

        let result = engine.execute(&dag).await.unwrap();
        assert_eq!(result.status, ExecutionStatus::Success);
        assert_eq!(result.node_results.len(), 2);
    }

    #[tokio::test]
    async fn test_parallel_config() {
        let config = ParallelConfig {
            max_parallelism: 4,
            fail_fast: false,
            ..Default::default()
        };
        assert_eq!(config.max_parallelism, 4);
        assert!(!config.fail_fast);
    }

    #[tokio::test]
    async fn test_parallel_invalid_dag() {
        let executor = Arc::new(SimpleTaskExecutor::new());
        let engine = ParallelDagExecutor::new(executor);

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

    #[test]
    fn test_compute_levels() {
        let dag = make_diamond_dag();
        let levels = ParallelDagExecutor::compute_levels(&dag);

        assert_eq!(levels["a"], 0);
        assert_eq!(levels["b"], 1);
        assert_eq!(levels["c"], 1);
        assert_eq!(levels["d"], 2);
    }

    #[test]
    fn test_group_by_levels() {
        let dag = make_diamond_dag();
        let levels = ParallelDagExecutor::compute_levels(&dag);
        let groups = ParallelDagExecutor::group_by_levels(&dag, &levels);

        assert_eq!(groups.len(), 3);
        assert!(groups[0].contains(&"a".to_string()));
        assert_eq!(groups[1].len(), 2); // b and c
        assert!(groups[2].contains(&"d".to_string()));
    }
}
