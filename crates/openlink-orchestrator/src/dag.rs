//! # DAG 定义
//!
//! 有向无环图（DAG）数据结构，用于定义多 Agent 任务编排。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// 节点 ID
pub type NodeId = String;

/// DAG 节点（任务节点）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// 节点 ID
    pub id: NodeId,
    /// 节点名称
    pub name: String,
    /// 执行的 Agent ID
    pub agent_id: String,
    /// 任务类型
    pub task_type: String,
    /// 任务参数
    #[serde(default)]
    pub params: serde_json::Value,
    /// 超时时间（毫秒，0 = 无超时）
    #[serde(default)]
    pub timeout_ms: u64,
    /// 失败后是否继续（允许部分失败）
    #[serde(default)]
    pub continue_on_error: bool,
    /// 最大重试次数
    #[serde(default)]
    pub max_retries: u32,
}

/// DAG 边（依赖关系）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagEdge {
    /// 源节点
    pub from: NodeId,
    /// 目标节点
    pub to: NodeId,
    /// 条件（可选，条件满足时才执行目标节点）
    #[serde(default)]
    pub condition: Option<EdgeCondition>,
}

/// 边条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeCondition {
    /// 源节点成功时
    OnSuccess,
    /// 源节点失败时
    OnFailure,
    /// 自定义条件表达式
    Custom(String),
}

/// DAG（有向无环图）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dag {
    /// DAG ID
    pub id: String,
    /// DAG 名称
    pub name: String,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 节点列表
    pub nodes: Vec<DagNode>,
    /// 边列表
    pub edges: Vec<DagEdge>,
    /// 全局超时（毫秒）
    #[serde(default = "default_global_timeout")]
    pub global_timeout_ms: u64,
}

fn default_global_timeout() -> u64 {
    300000 // 5 分钟
}

impl Dag {
    /// 创建新 DAG
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            global_timeout_ms: default_global_timeout(),
        }
    }

    /// 添加节点
    pub fn add_node(&mut self, node: DagNode) {
        self.nodes.push(node);
    }

    /// 添加边
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.push(DagEdge {
            from: from.to_string(),
            to: to.to_string(),
            condition: None,
        });
    }

    /// 添加条件边
    pub fn add_conditional_edge(&mut self, from: &str, to: &str, condition: EdgeCondition) {
        self.edges.push(DagEdge {
            from: from.to_string(),
            to: to.to_string(),
            condition: Some(condition),
        });
    }

    /// 验证 DAG 是否有效（无环、所有引用有效）
    pub fn validate(&self) -> Result<(), DagError> {
        // 检查节点 ID 唯一性
        let node_ids: HashSet<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();
        if node_ids.len() != self.nodes.len() {
            return Err(DagError::ValidationError("Duplicate node IDs".to_string()));
        }

        // 检查边引用的节点是否存在
        for edge in &self.edges {
            if !node_ids.contains(edge.from.as_str()) {
                return Err(DagError::ValidationError(format!(
                    "Edge references non-existent source node: {}",
                    edge.from
                )));
            }
            if !node_ids.contains(edge.to.as_str()) {
                return Err(DagError::ValidationError(format!(
                    "Edge references non-existent target node: {}",
                    edge.to
                )));
            }
        }

        // 检查是否有环（拓扑排序）
        if self.has_cycle() {
            return Err(DagError::ValidationError(
                "DAG contains a cycle".to_string(),
            ));
        }

        Ok(())
    }

    /// 检测是否有环（Kahn 算法拓扑排序）
    fn has_cycle(&self) -> bool {
        let in_degree = self.compute_in_degree();
        let adjacency = self.build_adjacency();

        let mut queue: VecDeque<&str> = VecDeque::new();
        for (node_id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(node_id);
            }
        }

        let mut visited = 0;
        while let Some(node_id) = queue.pop_front() {
            visited += 1;
            if let Some(neighbors) = adjacency.get(node_id) {
                for neighbor in neighbors {
                    if let Some(deg) = in_degree.get(neighbor.as_str()) {
                        let new_deg = deg - 1;
                        if new_deg == 0 {
                            queue.push_back(neighbor.as_str());
                        }
                    }
                }
            }
        }

        visited < self.nodes.len()
    }

    /// 拓扑排序
    pub fn topological_sort(&self) -> Vec<NodeId> {
        let in_degree = self.compute_in_degree();
        let adjacency = self.build_adjacency();

        let mut queue: VecDeque<String> = VecDeque::new();
        for (node_id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(node_id.clone());
            }
        }

        let mut result = Vec::new();
        while let Some(node_id) = queue.pop_front() {
            result.push(node_id.clone());
            if let Some(neighbors) = adjacency.get(&node_id) {
                for neighbor in neighbors {
                    if let Some(deg) = in_degree.get(neighbor.as_str()) {
                        if *deg <= 1 {
                            queue.push_back(neighbor.clone());
                        }
                    }
                }
            }
        }

        result
    }

    /// 获取入口节点（无入边的节点）
    pub fn entry_nodes(&self) -> Vec<&DagNode> {
        let in_degree = self.compute_in_degree();
        self.nodes
            .iter()
            .filter(|n| in_degree.get(n.id.as_str()).copied().unwrap_or(0) == 0)
            .collect()
    }

    /// 获取出口节点（无出边的节点）
    pub fn exit_nodes(&self) -> Vec<&DagNode> {
        let has_outgoing: HashSet<&str> = self.edges.iter().map(|e| e.from.as_str()).collect();
        self.nodes
            .iter()
            .filter(|n| !has_outgoing.contains(n.id.as_str()))
            .collect()
    }

    /// 获取节点的直接前驱
    pub fn predecessors(&self, node_id: &str) -> Vec<&DagNode> {
        let pred_ids: HashSet<&str> = self
            .edges
            .iter()
            .filter(|e| e.to == node_id)
            .map(|e| e.from.as_str())
            .collect();

        self.nodes
            .iter()
            .filter(|n| pred_ids.contains(n.id.as_str()))
            .collect()
    }

    /// 获取节点的直接后继
    pub fn successors(&self, node_id: &str) -> Vec<&DagNode> {
        let succ_ids: HashSet<&str> = self
            .edges
            .iter()
            .filter(|e| e.from == node_id)
            .map(|e| e.to.as_str())
            .collect();

        self.nodes
            .iter()
            .filter(|n| succ_ids.contains(n.id.as_str()))
            .collect()
    }

    /// 获取节点
    pub fn get_node(&self, node_id: &str) -> Option<&DagNode> {
        self.nodes.iter().find(|n| n.id == node_id)
    }

    pub fn compute_in_degree(&self) -> HashMap<String, usize> {
        let mut in_degree: HashMap<String, usize> =
            self.nodes.iter().map(|n| (n.id.clone(), 0)).collect();

        for edge in &self.edges {
            *in_degree.entry(edge.to.clone()).or_insert(0) += 1;
        }

        in_degree
    }

    pub fn build_adjacency(&self) -> HashMap<String, Vec<String>> {
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &self.edges {
            adjacency
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
        }
        adjacency
    }
}

/// DAG 错误
#[derive(Debug, thiserror::Error)]
pub enum DagError {
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Cycle detected: {0}")]
    CycleDetected(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dag() {
        let mut dag = Dag::new("test", "Test DAG");
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

        assert!(dag.validate().is_ok());
        assert_eq!(dag.entry_nodes().len(), 1);
        assert_eq!(dag.entry_nodes()[0].id, "a");
        assert_eq!(dag.exit_nodes().len(), 1);
        assert_eq!(dag.exit_nodes()[0].id, "b");
    }

    #[test]
    fn test_dag_with_cycle() {
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

        assert!(dag.validate().is_err());
    }

    #[test]
    fn test_diamond_dag() {
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

        assert!(dag.validate().is_ok());
        assert_eq!(dag.entry_nodes().len(), 1);
        assert_eq!(dag.exit_nodes().len(), 1);
        assert_eq!(dag.predecessors("d").len(), 2);
        assert_eq!(dag.successors("a").len(), 2);
    }

    #[test]
    fn test_topological_sort() {
        let mut dag = Dag::new("topo", "Topo Sort");
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
        dag.add_edge("b", "c");
        dag.add_edge("c", "d");

        let sorted = dag.topological_sort();
        assert_eq!(sorted.len(), 4);
        assert_eq!(sorted[0], "a");
        // a must come before b, b before c, c before d
        let pos_a = sorted.iter().position(|x| x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| x == "c").unwrap();
        let pos_d = sorted.iter().position(|x| x == "d").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_conditional_edge() {
        let mut dag = Dag::new("cond", "Conditional DAG");
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
            id: "b_success".to_string(),
            name: "B Success".to_string(),
            agent_id: "agent-2".to_string(),
            task_type: "process".to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });
        dag.add_node(DagNode {
            id: "b_failure".to_string(),
            name: "B Failure".to_string(),
            agent_id: "agent-3".to_string(),
            task_type: "fallback".to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });
        dag.add_conditional_edge("a", "b_success", EdgeCondition::OnSuccess);
        dag.add_conditional_edge("a", "b_failure", EdgeCondition::OnFailure);

        assert!(dag.validate().is_ok());
        assert_eq!(dag.successors("a").len(), 2);
    }
}
