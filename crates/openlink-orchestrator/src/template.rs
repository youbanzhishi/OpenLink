//! # 工作流模板
//!
//! 预设的编排模板，方便快速创建常见工作流。

use crate::dag::{Dag, DagNode, EdgeCondition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 工作流模板
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    /// 模板 ID
    pub id: String,
    /// 模板名称
    pub name: String,
    /// 模板描述
    pub description: String,
    /// 模板类别
    pub category: String,
    /// 模板参数定义
    pub parameters: Vec<TemplateParameter>,
    /// DAG 生成函数标识（用于从模板创建 DAG）
    pub dag_blueprint: DagBlueprint,
}

/// 模板参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    /// 参数名
    pub name: String,
    /// 参数类型
    pub param_type: String,
    /// 是否必填
    pub required: bool,
    /// 默认值
    pub default: Option<serde_json::Value>,
    /// 描述
    pub description: String,
}

/// DAG 蓝图（模板中的 DAG 结构定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagBlueprint {
    /// 节点模板列表
    pub nodes: Vec<NodeTemplate>,
    /// 边定义
    pub edges: Vec<EdgeTemplate>,
}

/// 节点模板
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTemplate {
    /// 节点 ID（模板中的占位符）
    pub id: String,
    /// 节点名称
    pub name: String,
    /// Agent ID（可使用参数引用如 {{agent_id}}）
    pub agent_id: String,
    /// 任务类型
    pub task_type: String,
    /// 默认参数
    #[serde(default)]
    pub default_params: serde_json::Value,
    /// 超时时间
    #[serde(default)]
    pub timeout_ms: u64,
    /// 是否允许失败继续
    #[serde(default)]
    pub continue_on_error: bool,
    /// 最大重试次数
    #[serde(default)]
    pub max_retries: u32,
}

/// 边模板
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeTemplate {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub condition: Option<EdgeCondition>,
}

/// 模板注册表
pub struct TemplateRegistry {
    templates: HashMap<String, WorkflowTemplate>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            templates: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    /// 注册默认模板
    fn register_defaults(&mut self) {
        // 模板1: 串行流水线
        self.register(WorkflowTemplate {
            id: "serial-pipeline".to_string(),
            name: "Serial Pipeline".to_string(),
            description: "串行执行多个 Agent 任务，前一步输出作为后一步输入".to_string(),
            category: "pipeline".to_string(),
            parameters: vec![TemplateParameter {
                name: "steps".to_string(),
                param_type: "array".to_string(),
                required: true,
                default: None,
                description: "Agent 任务步骤列表".to_string(),
            }],
            dag_blueprint: DagBlueprint {
                nodes: vec![],
                edges: vec![],
            },
        });

        // 模板2: 扇出-扇入
        self.register(WorkflowTemplate {
            id: "fan-out-fan-in".to_string(),
            name: "Fan-Out Fan-In".to_string(),
            description: "并行执行多个 Agent，聚合结果".to_string(),
            category: "parallel".to_string(),
            parameters: vec![
                TemplateParameter {
                    name: "agents".to_string(),
                    param_type: "array".to_string(),
                    required: true,
                    default: None,
                    description: "并行执行的 Agent 列表".to_string(),
                },
                TemplateParameter {
                    name: "aggregator_agent".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "聚合结果的 Agent".to_string(),
                },
            ],
            dag_blueprint: DagBlueprint {
                nodes: vec![],
                edges: vec![],
            },
        });

        // 模板3: 条件分支
        self.register(WorkflowTemplate {
            id: "conditional-branch".to_string(),
            name: "Conditional Branch".to_string(),
            description: "根据条件选择不同的执行路径".to_string(),
            category: "conditional".to_string(),
            parameters: vec![
                TemplateParameter {
                    name: "condition_agent".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "条件判断 Agent".to_string(),
                },
                TemplateParameter {
                    name: "success_agent".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "条件满足时执行的 Agent".to_string(),
                },
                TemplateParameter {
                    name: "failure_agent".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "条件不满足时执行的 Agent".to_string(),
                },
            ],
            dag_blueprint: DagBlueprint {
                nodes: vec![],
                edges: vec![],
            },
        });

        // 模板4: Map-Reduce
        self.register(WorkflowTemplate {
            id: "map-reduce".to_string(),
            name: "Map-Reduce".to_string(),
            description: "将任务分片到多个 Agent 并行处理（Map），然后汇总结果（Reduce）"
                .to_string(),
            category: "parallel".to_string(),
            parameters: vec![
                TemplateParameter {
                    name: "mapper_agent".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "Map 阶段 Agent（处理每个分片）".to_string(),
                },
                TemplateParameter {
                    name: "reducer_agent".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "Reduce 阶段 Agent（汇总所有分片结果）".to_string(),
                },
                TemplateParameter {
                    name: "shard_count".to_string(),
                    param_type: "integer".to_string(),
                    required: false,
                    default: Some(serde_json::json!(3)),
                    description: "分片数量".to_string(),
                },
            ],
            dag_blueprint: DagBlueprint {
                nodes: vec![],
                edges: vec![],
            },
        });

        // 模板5: 重试回路
        self.register(WorkflowTemplate {
            id: "retry-loop".to_string(),
            name: "Retry Loop".to_string(),
            description: "执行任务，失败时重试，达到最大重试次数后走失败路径".to_string(),
            category: "resilience".to_string(),
            parameters: vec![
                TemplateParameter {
                    name: "task_agent".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "执行任务的 Agent".to_string(),
                },
                TemplateParameter {
                    name: "max_retries".to_string(),
                    param_type: "integer".to_string(),
                    required: false,
                    default: Some(serde_json::json!(3)),
                    description: "最大重试次数".to_string(),
                },
                TemplateParameter {
                    name: "fallback_agent".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    description: "所有重试失败后的降级 Agent".to_string(),
                },
            ],
            dag_blueprint: DagBlueprint {
                nodes: vec![],
                edges: vec![],
            },
        });

        // 模板6: 并行合并
        self.register(WorkflowTemplate {
            id: "parallel-merge".to_string(),
            name: "Parallel Merge".to_string(),
            description: "多 Agent 并行处理后合并所有结果（无需单独聚合 Agent）".to_string(),
            category: "parallel".to_string(),
            parameters: vec![TemplateParameter {
                name: "agents".to_string(),
                param_type: "array".to_string(),
                required: true,
                default: None,
                description: "并行执行的 Agent 列表".to_string(),
            }],
            dag_blueprint: DagBlueprint {
                nodes: vec![],
                edges: vec![],
            },
        });
    }

    /// 注册模板
    pub fn register(&mut self, template: WorkflowTemplate) {
        self.templates.insert(template.id.clone(), template);
    }

    /// 获取模板
    pub fn get(&self, id: &str) -> Option<&WorkflowTemplate> {
        self.templates.get(id)
    }

    /// 列出所有模板
    pub fn list(&self) -> Vec<&WorkflowTemplate> {
        self.templates.values().collect()
    }

    /// 按类别列出模板
    pub fn list_by_category(&self, category: &str) -> Vec<&WorkflowTemplate> {
        self.templates
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// 从模板创建 DAG（串行流水线）
    pub fn create_serial_pipeline(
        dag_id: &str,
        steps: Vec<(&str, &str, &str)>, // (agent_id, task_type, node_name)
    ) -> Dag {
        let mut dag = Dag::new(dag_id, "Serial Pipeline");

        for (idx, (agent_id, task_type, name)) in steps.iter().enumerate() {
            let node_id = format!("step_{}", idx);
            dag.add_node(DagNode {
                id: node_id.clone(),
                name: name.to_string(),
                agent_id: agent_id.to_string(),
                task_type: task_type.to_string(),
                params: serde_json::Value::Null,
                timeout_ms: 0,
                continue_on_error: false,
                max_retries: 0,
            });

            if idx > 0 {
                dag.add_edge(&format!("step_{}", idx - 1), &node_id);
            }
        }

        dag
    }

    /// 从模板创建扇出-扇入 DAG
    pub fn create_fan_out_fan_in(
        dag_id: &str,
        fan_agents: Vec<(&str, &str)>, // (agent_id, task_type)
        aggregator_agent: &str,
        aggregator_task: &str,
    ) -> Dag {
        let mut dag = Dag::new(dag_id, "Fan-Out Fan-In");

        // 入口节点
        dag.add_node(DagNode {
            id: "start".to_string(),
            name: "Start".to_string(),
            agent_id: "system".to_string(),
            task_type: "noop".to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });

        // 扇出节点
        for (idx, (agent_id, task_type)) in fan_agents.iter().enumerate() {
            let node_id = format!("fan_{}", idx);
            dag.add_node(DagNode {
                id: node_id.clone(),
                name: format!("Fan {}", idx),
                agent_id: agent_id.to_string(),
                task_type: task_type.to_string(),
                params: serde_json::Value::Null,
                timeout_ms: 0,
                continue_on_error: true,
                max_retries: 1,
            });
            dag.add_edge("start", &node_id);
        }

        // 聚合节点
        dag.add_node(DagNode {
            id: "aggregate".to_string(),
            name: "Aggregate".to_string(),
            agent_id: aggregator_agent.to_string(),
            task_type: aggregator_task.to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });

        // 扇出节点 → 聚合
        for idx in 0..fan_agents.len() {
            dag.add_edge(&format!("fan_{}", idx), "aggregate");
        }

        dag
    }

    /// 从模板创建条件分支 DAG
    pub fn create_conditional_branch(
        dag_id: &str,
        condition_agent: &str,
        condition_task: &str,
        success_agent: &str,
        success_task: &str,
        failure_agent: &str,
        failure_task: &str,
    ) -> Dag {
        let mut dag = Dag::new(dag_id, "Conditional Branch");

        dag.add_node(DagNode {
            id: "condition".to_string(),
            name: "Condition Check".to_string(),
            agent_id: condition_agent.to_string(),
            task_type: condition_task.to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });

        dag.add_node(DagNode {
            id: "success_branch".to_string(),
            name: "Success Branch".to_string(),
            agent_id: success_agent.to_string(),
            task_type: success_task.to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });

        dag.add_node(DagNode {
            id: "failure_branch".to_string(),
            name: "Failure Branch".to_string(),
            agent_id: failure_agent.to_string(),
            task_type: failure_task.to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });

        dag.add_conditional_edge("condition", "success_branch", EdgeCondition::OnSuccess);
        dag.add_conditional_edge("condition", "failure_branch", EdgeCondition::OnFailure);

        dag
    }

    /// 创建 Map-Reduce DAG
    ///
    /// 将任务分片到多个 Mapper 并行处理，然后 Reducer 汇总结果。
    pub fn create_map_reduce(
        dag_id: &str,
        mapper_agent: &str,
        reducer_agent: &str,
        shard_count: usize,
        mapper_task: &str,
        reducer_task: &str,
    ) -> Dag {
        let mut dag = Dag::new(dag_id, "Map-Reduce");

        // Mapper 节点
        for i in 0..shard_count {
            dag.add_node(DagNode {
                id: format!("map_{}", i),
                name: format!("Map Shard {}", i),
                agent_id: mapper_agent.to_string(),
                task_type: mapper_task.to_string(),
                params: serde_json::json!({"shard": i, "total_shards": shard_count}),
                timeout_ms: 0,
                continue_on_error: false,
                max_retries: 1,
            });
        }

        // Reducer 节点
        dag.add_node(DagNode {
            id: "reduce".to_string(),
            name: "Reduce".to_string(),
            agent_id: reducer_agent.to_string(),
            task_type: reducer_task.to_string(),
            params: serde_json::json!({"shard_count": shard_count}),
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });

        // 每个 Mapper → Reducer
        for i in 0..shard_count {
            dag.add_edge(&format!("map_{}", i), "reduce");
        }

        dag
    }

    /// 创建并行合并 DAG
    ///
    /// 多 Agent 并行执行，无需单独聚合节点。
    /// 结果由编排器自动合并。
    pub fn create_parallel_merge(dag_id: &str, agents: Vec<(&str, &str)>) -> Dag {
        let mut dag = Dag::new(dag_id, "Parallel Merge");

        // 起始节点
        dag.add_node(DagNode {
            id: "start".to_string(),
            name: "Start".to_string(),
            agent_id: "system".to_string(),
            task_type: "noop".to_string(),
            params: serde_json::Value::Null,
            timeout_ms: 0,
            continue_on_error: false,
            max_retries: 0,
        });

        for (i, (agent_id, task_type)) in agents.iter().enumerate() {
            dag.add_node(DagNode {
                id: format!("task_{}", i),
                name: format!("Task {}", i),
                agent_id: agent_id.to_string(),
                task_type: task_type.to_string(),
                params: serde_json::Value::Null,
                timeout_ms: 0,
                continue_on_error: true,
                max_retries: 0,
            });
            dag.add_edge("start", &format!("task_{}", i));
        }

        dag
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_defaults() {
        let registry = TemplateRegistry::new();
        assert!(registry.get("serial-pipeline").is_some());
        assert!(registry.get("fan-out-fan-in").is_some());
        assert!(registry.get("conditional-branch").is_some());
    }

    #[test]
    fn test_list_by_category() {
        let registry = TemplateRegistry::new();
        let pipeline = registry.list_by_category("pipeline");
        assert!(!pipeline.is_empty());
    }

    #[test]
    fn test_create_serial_pipeline() {
        let dag = TemplateRegistry::create_serial_pipeline(
            "test-serial",
            vec![
                ("agent-1", "extract", "Extract Data"),
                ("agent-2", "transform", "Transform Data"),
                ("agent-3", "load", "Load Data"),
            ],
        );

        assert!(dag.validate().is_ok());
        assert_eq!(dag.nodes.len(), 3);
        assert_eq!(dag.edges.len(), 2);
        assert_eq!(dag.entry_nodes().len(), 1);
        assert_eq!(dag.exit_nodes().len(), 1);
    }

    #[test]
    fn test_create_fan_out_fan_in() {
        let dag = TemplateRegistry::create_fan_out_fan_in(
            "test-fanout",
            vec![
                ("agent-1", "analyze-text"),
                ("agent-2", "analyze-image"),
                ("agent-3", "analyze-audio"),
            ],
            "agent-aggregator",
            "aggregate-results",
        );

        assert!(dag.validate().is_ok());
        assert_eq!(dag.nodes.len(), 5); // start + 3 fan + aggregate
        assert_eq!(dag.entry_nodes().len(), 1);
        assert_eq!(dag.exit_nodes().len(), 1);
    }

    #[test]
    fn test_create_conditional_branch() {
        let dag = TemplateRegistry::create_conditional_branch(
            "test-cond",
            "agent-checker",
            "check",
            "agent-success",
            "success",
            "agent-failure",
            "failure",
        );

        assert!(dag.validate().is_ok());
        assert_eq!(dag.nodes.len(), 3);
        assert_eq!(dag.edges.len(), 2);
    }

    #[test]
    fn test_create_map_reduce() {
        let dag = TemplateRegistry::create_map_reduce(
            "test-mr",
            "agent-mapper",
            "agent-reducer",
            4,
            "map",
            "reduce",
        );

        assert!(dag.validate().is_ok());
        assert_eq!(dag.nodes.len(), 5); // 4 mappers + 1 reducer
        assert_eq!(dag.edges.len(), 4); // each mapper -> reducer
        assert_eq!(dag.entry_nodes().len(), 4); // all mappers are entry nodes
        assert_eq!(dag.exit_nodes().len(), 1); // only reducer
    }

    #[test]
    fn test_create_parallel_merge() {
        let dag = TemplateRegistry::create_parallel_merge(
            "test-merge",
            vec![("agent-1", "analyze-text"), ("agent-2", "analyze-image")],
        );

        assert!(dag.validate().is_ok());
        assert_eq!(dag.nodes.len(), 3); // start + 2 tasks
        assert_eq!(dag.entry_nodes().len(), 1); // start node
        assert_eq!(dag.exit_nodes().len(), 2); // both tasks are exit nodes
    }

    #[test]
    fn test_registry_new_templates() {
        let registry = TemplateRegistry::new();
        assert!(registry.get("map-reduce").is_some());
        assert!(registry.get("retry-loop").is_some());
        assert!(registry.get("parallel-merge").is_some());
    }
}
