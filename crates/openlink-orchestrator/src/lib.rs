//! # OpenLink Orchestrator — 多 Agent 任务编排引擎
//!
//! Phase 6: DAG 定义与执行、工作流模板、结果聚合。
//!
//! ## 核心组件
//! - **DAG**: 有向无环图定义
//! - **DagExecutor**: 顺序 DAG 执行引擎
//! - **ParallelDagExecutor**: 并行 DAG 执行引擎
//! - **WorkflowTemplate**: 预设编排模板
//! - **ResultAggregator**: 结果聚合和回调

pub mod aggregator;
pub mod dag;
pub mod executor;
pub mod parallel_executor;
pub mod template;

pub use aggregator::{AggregationStrategy, ResultAggregator};
pub use dag::{Dag, DagEdge, DagNode, EdgeCondition, NodeId};
pub use executor::{DagExecutor, ExecutionResult, ExecutionStatus, NodeResult, SimpleTaskExecutor, TaskExecutor};
pub use parallel_executor::{ParallelConfig, ParallelDagExecutor};
pub use template::{TemplateRegistry, WorkflowTemplate};
