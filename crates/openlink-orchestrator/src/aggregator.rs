//! # 结果聚合和回调
//!
//! Phase 6: 聚合多 Agent 的执行结果，支持回调通知。

use crate::executor::{ExecutionResult, NodeResult, ExecutionStatus};
use crate::dag::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 聚合策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    /// 合并所有结果到一个 JSON 对象
    Merge,
    /// 仅取最后一个（出口节点）的结果
    Last,
    /// 收集所有输出为数组
    Collect,
    /// 自定义聚合（由回调函数处理）
    Custom(String),
}

/// 聚合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResult {
    /// DAG ID
    pub dag_id: String,
    /// 聚合后的结果
    pub result: serde_json::Value,
    /// 聚合策略
    pub strategy: AggregationStrategy,
    /// 参与聚合的节点数
    pub node_count: usize,
    /// 成功节点数
    pub success_count: usize,
    /// 失败节点数
    pub failure_count: usize,
    /// 跳过节点数
    pub skipped_count: usize,
}

/// 回调配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackConfig {
    /// 回调 URL
    pub url: String,
    /// 回调方法
    #[serde(default = "default_method")]
    pub method: String,
    /// 自定义 Headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 是否在失败时也回调
    #[serde(default)]
    pub callback_on_failure: bool,
}

fn default_method() -> String {
    "POST".to_string()
}

/// 回调事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackEvent {
    /// 事件类型
    pub event_type: CallbackEventType,
    /// DAG ID
    pub dag_id: String,
    /// 节点 ID（如果是节点级别事件）
    pub node_id: Option<String>,
    /// 事件数据
    pub data: serde_json::Value,
    /// 时间戳
    pub timestamp: i64,
}

/// 回调事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CallbackEventType {
    /// DAG 执行开始
    DagStarted,
    /// 节点执行完成
    NodeCompleted,
    /// 节点执行失败
    NodeFailed,
    /// DAG 执行完成
    DagCompleted,
    /// DAG 执行失败
    DagFailed,
}

/// 结果聚合器
pub struct ResultAggregator {
    /// 回调配置列表
    callbacks: Vec<CallbackConfig>,
}

impl ResultAggregator {
    /// 创建聚合器
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    /// 添加回调
    pub fn add_callback(&mut self, config: CallbackConfig) {
        self.callbacks.push(config);
    }

    /// 聚合执行结果
    pub fn aggregate(
        &self,
        execution_result: &ExecutionResult,
        strategy: &AggregationStrategy,
    ) -> AggregatedResult {
        let (success_count, failure_count, skipped_count) =
            self.count_results(&execution_result.node_results);

        let result = match strategy {
            AggregationStrategy::Merge => self.merge_results(&execution_result.node_results),
            AggregationStrategy::Last => self.last_result(&execution_result.node_results),
            AggregationStrategy::Collect => self.collect_results(&execution_result.node_results),
            AggregationStrategy::Custom(name) => {
                serde_json::json!({
                    "custom_aggregator": name,
                    "dag_id": execution_result.dag_id,
                    "success_count": success_count,
                    "failure_count": failure_count,
                })
            }
        };

        AggregatedResult {
            dag_id: execution_result.dag_id.clone(),
            result,
            strategy: strategy.clone(),
            node_count: execution_result.node_results.len(),
            success_count,
            failure_count,
            skipped_count,
        }
    }

    /// 触发回调事件
    pub async fn trigger_callback(&self, event: &CallbackEvent) {
        for callback in &self.callbacks {
            // 检查是否应该在失败时回调
            if !callback.callback_on_failure
                && (event.event_type == CallbackEventType::NodeFailed
                    || event.event_type == CallbackEventType::DagFailed)
            {
                continue;
            }

            tracing::info!(
                url = %callback.url,
                event_type = ?event.event_type,
                dag_id = %event.dag_id,
                "Triggering callback"
            );

            // 尝试发送 HTTP 回调
            #[cfg(feature = "http-callback")]
            {
                match self.send_http_callback(callback, event).await {
                    Ok(()) => {
                        tracing::info!(
                            url = %callback.url,
                            "Callback delivered successfully"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            url = %callback.url,
                            error = %e,
                            "Callback delivery failed"
                        );
                    }
                }
            }
        }
    }

    /// 发送 HTTP 回调
    #[cfg(feature = "http-callback")]
    async fn send_http_callback(
        &self,
        callback: &CallbackConfig,
        event: &CallbackEvent,
    ) -> Result<(), String> {
        let client = reqwest::Client::new();
        let body = serde_json::to_string(event)
            .map_err(|e| format!("Serialization error: {}", e))?;

        let request = match callback.method.to_uppercase().as_str() {
            "POST" => client.post(&callback.url),
            "PUT" => client.put(&callback.url),
            _ => client.post(&callback.url),
        };

        let mut request = request
            .header("Content-Type", "application/json")
            .body(body);

        for (key, value) in &callback.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Callback returned status: {}", response.status()));
        }

        Ok(())
    }

    /// 创建回调事件
    pub fn create_event(
        &self,
        event_type: CallbackEventType,
        dag_id: &str,
        node_id: Option<&str>,
        data: serde_json::Value,
    ) -> CallbackEvent {
        CallbackEvent {
            event_type,
            dag_id: dag_id.to_string(),
            node_id: node_id.map(|s| s.to_string()),
            data,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    fn merge_results(&self, results: &HashMap<NodeId, NodeResult>) -> serde_json::Value {
        let mut merged = serde_json::Map::new();
        for (node_id, node_result) in results {
            if node_result.status == ExecutionStatus::Success {
                merged.insert(
                    node_id.clone(),
                    node_result.output.clone(),
                );
            }
        }
        serde_json::Value::Object(merged)
    }

    fn last_result(&self, results: &HashMap<NodeId, NodeResult>) -> serde_json::Value {
        // 找到最后一个成功的节点结果
        results
            .values()
            .filter(|r| r.status == ExecutionStatus::Success)
            .last()
            .map(|r| r.output.clone())
            .unwrap_or(serde_json::Value::Null)
    }

    fn collect_results(&self, results: &HashMap<NodeId, NodeResult>) -> serde_json::Value {
        let collected: Vec<serde_json::Value> = results
            .values()
            .filter(|r| r.status == ExecutionStatus::Success)
            .map(|r| {
                serde_json::json!({
                    "node_id": r.node_id,
                    "output": r.output,
                })
            })
            .collect();
        serde_json::Value::Array(collected)
    }

    fn count_results(
        &self,
        results: &HashMap<NodeId, NodeResult>,
    ) -> (usize, usize, usize) {
        let mut success = 0;
        let mut failure = 0;
        let mut skipped = 0;

        for node_result in results.values() {
            match &node_result.status {
                ExecutionStatus::Success => success += 1,
                ExecutionStatus::Failed(_) => failure += 1,
                ExecutionStatus::Skipped => skipped += 1,
                _ => {}
            }
        }

        (success, failure, skipped)
    }
}

impl Default for ResultAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_execution_result() -> ExecutionResult {
        let mut node_results = HashMap::new();
        node_results.insert(
            "a".to_string(),
            NodeResult {
                node_id: "a".to_string(),
                status: ExecutionStatus::Success,
                output: serde_json::json!({"data": "from_a"}),
                elapsed_ms: 100,
                retry_count: 0,
            },
        );
        node_results.insert(
            "b".to_string(),
            NodeResult {
                node_id: "b".to_string(),
                status: ExecutionStatus::Success,
                output: serde_json::json!({"data": "from_b"}),
                elapsed_ms: 200,
                retry_count: 0,
            },
        );

        ExecutionResult {
            dag_id: "test-dag".to_string(),
            status: ExecutionStatus::Success,
            node_results,
            total_elapsed_ms: 300,
        }
    }

    #[test]
    fn test_merge_strategy() {
        let aggregator = ResultAggregator::new();
        let result = make_execution_result();

        let aggregated = aggregator.aggregate(&result, &AggregationStrategy::Merge);
        assert_eq!(aggregated.success_count, 2);
        assert_eq!(aggregated.failure_count, 0);
        assert!(aggregated.result.is_object());
    }

    #[test]
    fn test_last_strategy() {
        let aggregator = ResultAggregator::new();
        let result = make_execution_result();

        let aggregated = aggregator.aggregate(&result, &AggregationStrategy::Last);
        assert!(aggregated.result.is_object());
    }

    #[test]
    fn test_collect_strategy() {
        let aggregator = ResultAggregator::new();
        let result = make_execution_result();

        let aggregated = aggregator.aggregate(&result, &AggregationStrategy::Collect);
        assert!(aggregated.result.is_array());
        assert_eq!(aggregated.result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_custom_strategy() {
        let aggregator = ResultAggregator::new();
        let result = make_execution_result();

        let aggregated = aggregator.aggregate(
            &result,
            &AggregationStrategy::Custom("my-aggregator".to_string()),
        );
        assert_eq!(aggregated.strategy, AggregationStrategy::Custom("my-aggregator".to_string()));
    }

    #[test]
    fn test_callback_event_creation() {
        let aggregator = ResultAggregator::new();
        let event = aggregator.create_event(
            CallbackEventType::DagCompleted,
            "dag-1",
            None,
            serde_json::json!({"status": "success"}),
        );

        assert_eq!(event.dag_id, "dag-1");
        assert_eq!(event.event_type, CallbackEventType::DagCompleted);
        assert!(event.node_id.is_none());
    }
}
