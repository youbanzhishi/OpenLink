//! # A2A 消息总线（Phase 6）
//!
//! Agent 间通信的消息总线，支持：
//! - 点对点消息
//! - 广播消息
//! - 按能力订阅
//! - 消息过滤和路由

use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing;

/// 消息总线配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBusConfig {
    /// 每个订阅者的消息缓冲区大小
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// 消息过期时间（秒），0 = 不过期
    #[serde(default)]
    pub message_ttl_secs: u64,
    /// 最大消息大小（字节）
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
}

fn default_buffer_size() -> usize { 256 }
fn default_max_message_size() -> usize { 1024 * 1024 } // 1MB

impl Default for MessageBusConfig {
    fn default() -> Self {
        Self {
            buffer_size: default_buffer_size(),
            message_ttl_secs: 0,
            max_message_size: default_max_message_size(),
        }
    }
}

/// 订阅 ID
pub type SubscriptionId = String;

/// 订阅信息
#[derive(Debug, Clone)]
struct Subscription {
    /// 订阅者 Agent ID
    agent_id: AgentId,
    /// 订阅的消息类型（None = 所有类型）
    msg_type_filter: Option<A2AMessageType>,
    /// 消息发送端
    sender: mpsc::Sender<A2AMessage>,
}

/// A2A 消息总线
///
/// 支持点对点和广播通信的消息传递系统。
pub struct MessageBus {
    config: MessageBusConfig,
    /// 按消息类型分组的订阅列表
    subscriptions: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    /// 统计
    stats: Arc<RwLock<MessageBusStats>>,
}

/// 消息总线统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageBusStats {
    /// 发送的消息总数
    pub messages_sent: u64,
    /// 广播消息数
    pub broadcasts: u64,
    /// 点对点消息数
    pub direct_messages: u64,
    /// 投递失败数
    pub delivery_failures: u64,
    /// 当前订阅数
    pub active_subscriptions: usize,
}

impl MessageBus {
    /// 创建消息总线
    pub fn new(config: MessageBusConfig) -> Self {
        Self {
            config,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(MessageBusStats::default())),
        }
    }

    /// 订阅特定类型的消息
    ///
    /// 返回消息接收端和订阅 ID。
    pub async fn subscribe(
        &self,
        agent_id: AgentId,
        msg_type: Option<A2AMessageType>,
    ) -> (SubscriptionId, mpsc::Receiver<A2AMessage>) {
        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        let sub_id = uuid::Uuid::new_v4().to_string();

        let subscription = Subscription {
            agent_id: agent_id.clone(),
            msg_type_filter: msg_type.clone(),
            sender: tx,
        };

        let key = match &msg_type {
            Some(mt) => format!("{:?}", mt),
            None => "__all__".to_string(),
        };

        {
            let mut subs = self.subscriptions.write().await;
            subs.entry(key).or_default().push(subscription);
        }

        {
            let mut stats = self.stats.write().await;
            stats.active_subscriptions += 1;
        }

        tracing::info!(
            agent_id = %agent_id,
            subscription_id = %sub_id,
            msg_type = ?msg_type,
            "Agent subscribed to message bus"
        );

        (sub_id, rx)
    }

    /// 发送点对点消息
    pub async fn send_direct(&self, message: A2AMessage) -> Result<(), MessageBusError> {
        // 检查消息大小
        let msg_size = serde_json::to_string(&message)
            .map_err(|e| MessageBusError::SerializationError(e.to_string()))?
            .len();

        if msg_size > self.config.max_message_size {
            return Err(MessageBusError::MessageTooLarge {
                size: msg_size,
                limit: self.config.max_message_size,
            });
        }

        let mut delivered = false;

        // 查找目标 Agent 的所有订阅
        let subs = self.subscriptions.read().await;
        for (_, subscription_list) in subs.iter() {
            for sub in subscription_list.iter() {
                if sub.agent_id == message.to {
                    // 检查消息类型过滤器
                    if let Some(filter) = &sub.msg_type_filter {
                        if *filter != message.msg_type {
                            continue;
                        }
                    }

                    match sub.sender.try_send(message.clone()) {
                        Ok(()) => delivered = true,
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            tracing::warn!(
                                to = %message.to,
                                "Message bus channel full, dropping message"
                            );
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            // 频道已关闭，稍后清理
                        }
                    }
                }
            }
        }

        {
            let mut stats = self.stats.write().await;
            stats.messages_sent += 1;
            stats.direct_messages += 1;
            if !delivered {
                stats.delivery_failures += 1;
            }
        }

        if delivered {
            Ok(())
        } else {
            Err(MessageBusError::AgentNotFound(message.to.clone()))
        }
    }

    /// 广播消息给所有订阅了该类型的 Agent
    pub async fn broadcast(&self, message: A2AMessage) -> BroadcastResult {
        let mut delivered = 0;
        let mut failed = 0;
        let msg_type_key = format!("{:?}", message.msg_type);

        let subs = self.subscriptions.read().await;

        // 查找特定类型的订阅者
        if let Some(subscription_list) = subs.get(&msg_type_key) {
            for sub in subscription_list {
                // 不发送给自己
                if sub.agent_id == message.from {
                    continue;
                }

                match sub.sender.try_send(message.clone()) {
                    Ok(()) => delivered += 1,
                    Err(_) => failed += 1,
                }
            }
        }

        // 查找全局订阅者
        if let Some(subscription_list) = subs.get("__all__") {
            for sub in subscription_list {
                if sub.agent_id == message.from {
                    continue;
                }

                match sub.sender.try_send(message.clone()) {
                    Ok(()) => delivered += 1,
                    Err(_) => failed += 1,
                }
            }
        }

        {
            let mut stats = self.stats.write().await;
            stats.messages_sent += 1;
            stats.broadcasts += 1;
            stats.delivery_failures += failed as u64;
        }

        BroadcastResult {
            delivered,
            failed,
        }
    }

    /// 获取消息总线统计
    pub async fn stats(&self) -> MessageBusStats {
        let stats = self.stats.read().await;
        let mut result = stats.clone();
        // 更新活跃订阅数
        let subs = self.subscriptions.read().await;
        result.active_subscriptions = subs.values().map(|v| v.len()).sum();
        result
    }
}

/// 广播结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastResult {
    /// 成功投递数
    pub delivered: usize,
    /// 失败投递数
    pub failed: usize,
}

/// 消息总线错误
#[derive(Debug, thiserror::Error)]
pub enum MessageBusError {
    /// Agent 未找到
    #[error("Agent not found: {0}")]
    AgentNotFound(AgentId),

    /// 消息过大
    #[error("Message too large: {size} bytes, limit {limit} bytes")]
    MessageTooLarge { size: usize, limit: usize },

    /// 序列化错误
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// 通道已满
    #[error("Channel full for agent: {0}")]
    ChannelFull(AgentId),
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new(MessageBusConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscribe_and_direct_message() {
        let bus = MessageBus::default();

        let (_, mut rx) = bus.subscribe("agent-b".to_string(), Some(A2AMessageType::TaskRequest)).await;

        let message = A2AMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: "agent-a".to_string(),
            to: "agent-b".to_string(),
            msg_type: A2AMessageType::TaskRequest,
            payload: serde_json::json!({"task": "analyze"}),
            timestamp: chrono::Utc::now().timestamp(),
        };

        bus.send_direct(message).await.unwrap();

        let received = rx.try_recv().unwrap();
        assert_eq!(received.from, "agent-a");
        assert_eq!(received.msg_type, A2AMessageType::TaskRequest);
    }

    #[tokio::test]
    async fn test_direct_message_not_found() {
        let bus = MessageBus::default();

        let message = A2AMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: "agent-a".to_string(),
            to: "agent-nonexistent".to_string(),
            msg_type: A2AMessageType::TaskRequest,
            payload: serde_json::Value::Null,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = bus.send_direct(message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_broadcast() {
        let bus = MessageBus::default();

        let (_, mut rx1) = bus.subscribe("agent-b".to_string(), Some(A2AMessageType::Event)).await;
        let (_, mut rx2) = bus.subscribe("agent-c".to_string(), Some(A2AMessageType::Event)).await;

        let message = A2AMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: "agent-a".to_string(),
            to: String::new(), // 广播不指定 to
            msg_type: A2AMessageType::Event,
            payload: serde_json::json!({"event": "data_ready"}),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = bus.broadcast(message).await;
        assert_eq!(result.delivered, 2);
        assert_eq!(result.failed, 0);

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_broadcast_no_self_delivery() {
        let bus = MessageBus::default();

        let (_, mut rx) = bus.subscribe("agent-a".to_string(), Some(A2AMessageType::Event)).await;

        let message = A2AMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: "agent-a".to_string(),
            to: String::new(),
            msg_type: A2AMessageType::Event,
            payload: serde_json::Value::Null,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = bus.broadcast(message).await;
        assert_eq!(result.delivered, 0); // 不应投递给自己

        // 频道应为空
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_message_bus_stats() {
        let bus = MessageBus::default();

        bus.subscribe("agent-b".to_string(), Some(A2AMessageType::TaskRequest)).await;

        let stats = bus.stats().await;
        assert_eq!(stats.active_subscriptions, 1);
    }

    #[tokio::test]
    async fn test_message_too_large() {
        let config = MessageBusConfig {
            max_message_size: 10,
            ..Default::default()
        };
        let bus = MessageBus::new(config);

        bus.subscribe("agent-b".to_string(), Some(A2AMessageType::TaskRequest)).await;

        let message = A2AMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: "agent-a".to_string(),
            to: "agent-b".to_string(),
            msg_type: A2AMessageType::TaskRequest,
            payload: serde_json::json!("this is a very long payload that exceeds the limit"),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = bus.send_direct(message).await;
        assert!(result.is_err());
    }
}
