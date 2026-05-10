//! # 心跳监测和故障检测
//!
//! Phase 6: 监控 Agent 的在线状态，检测故障。

use crate::types::*;
use crate::registry::AgentRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

/// 心跳配置
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// 心跳间隔（秒）
    pub interval_secs: u64,
    /// 超时阈值（秒）：超过此时间未收到心跳则标记为 Offline
    pub timeout_secs: u64,
    /// 清理阈值（秒）：超过此时间则从注册表移除
    pub cleanup_secs: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            timeout_secs: 90,
            cleanup_secs: 600,
        }
    }
}

/// 心跳监控器
pub struct HeartbeatMonitor {
    config: HeartbeatConfig,
    registry: Arc<AgentRegistry>,
    /// 监控运行状态
    running: Arc<RwLock<bool>>,
}

impl HeartbeatMonitor {
    /// 创建心跳监控器
    pub fn new(config: HeartbeatConfig, registry: Arc<AgentRegistry>) -> Self {
        Self {
            config,
            registry,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 处理收到的心跳消息
    pub async fn handle_heartbeat(&self, msg: &HeartbeatMessage) -> Result<(), String> {
        self.registry
            .update_heartbeat(&msg.agent_id, msg.status.clone(), msg.active_tasks)
            .await
            .map_err(|e| e.to_string())
    }

    /// 检查超时的 Agent
    pub async fn check_timeouts(&self) -> Vec<AgentId> {
        let now = chrono::Utc::now().timestamp();
        let agents = self.registry.list_all().await;

        let timed_out: Vec<AgentId> = agents
            .iter()
            .filter(|agent| {
                agent.status != AgentStatus::Offline
                    && (now - agent.last_heartbeat) > self.config.timeout_secs as i64
            })
            .map(|agent| agent.id.clone())
            .collect();

        // 标记超时 Agent 为 Offline
        for agent_id in &timed_out {
            if let Err(e) = self.registry.update(agent_id, |info| {
                info.status = AgentStatus::Offline;
            }).await {
                tracing::warn!(agent_id = %agent_id, error = %e, "Failed to mark agent as offline");
            } else {
                tracing::info!(agent_id = %agent_id, "Agent marked as offline due to heartbeat timeout");
            }
        }

        timed_out
    }

    /// 清理长期离线的 Agent
    pub async fn cleanup_offline(&self) -> Vec<AgentId> {
        let now = chrono::Utc::now().timestamp();
        let agents = self.registry.list_all().await;

        let to_remove: Vec<AgentId> = agents
            .iter()
            .filter(|agent| {
                agent.status == AgentStatus::Offline
                    && (now - agent.last_heartbeat) > self.config.cleanup_secs as i64
            })
            .map(|agent| agent.id.clone())
            .collect();

        for agent_id in &to_remove {
            if let Err(e) = self.registry.deregister(agent_id).await {
                tracing::warn!(agent_id = %agent_id, error = %e, "Failed to cleanup agent");
            } else {
                tracing::info!(agent_id = %agent_id, "Agent cleaned up after extended offline");
            }
        }

        to_remove
    }

    /// 启动后台监控任务
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        *running = true;
        drop(running);

        tracing::info!(
            interval = self.config.interval_secs,
            timeout = self.config.timeout_secs,
            "Heartbeat monitor started"
        );
    }

    /// 停止监控
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        tracing::info!("Heartbeat monitor stopped");
    }

    /// 是否正在运行
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// 生成心跳消息（Agent 自身发送）
    pub fn create_heartbeat(&self, agent_id: &str, status: AgentStatus, active_tasks: u32, seq: u64) -> HeartbeatMessage {
        HeartbeatMessage {
            agent_id: agent_id.to_string(),
            seq,
            status,
            active_tasks,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_registry_with_agent(id: &str) -> Arc<AgentRegistry> {
        let registry = Arc::new(AgentRegistry::new());
        let agent = AgentInfo {
            id: id.to_string(),
            name: format!("Agent {}", id),
            description: String::new(),
            version: "1.0".to_string(),
            endpoint: format!("https://{}.example.com", id),
            capabilities: vec![],
            metadata: HashMap::new(),
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            status: AgentStatus::Online,
        };
        registry.register(agent).await.unwrap();
        registry
    }

    #[tokio::test]
    async fn test_handle_heartbeat() {
        let registry = setup_registry_with_agent("agent-1").await;
        let monitor = HeartbeatMonitor::new(HeartbeatConfig::default(), registry.clone());

        let msg = HeartbeatMessage {
            agent_id: "agent-1".to_string(),
            seq: 1,
            status: AgentStatus::Busy,
            active_tasks: 3,
            timestamp: chrono::Utc::now().timestamp(),
        };

        monitor.handle_heartbeat(&msg).await.unwrap();

        let agent = registry.get("agent-1").await.unwrap();
        assert_eq!(agent.status, AgentStatus::Busy);
    }

    #[tokio::test]
    async fn test_check_timeouts() {
        let registry = Arc::new(AgentRegistry::new());

        // 注册一个"旧"Agent（心跳时间在很久以前）
        let mut agent = AgentInfo {
            id: "old-agent".to_string(),
            name: "Old Agent".to_string(),
            description: String::new(),
            version: "1.0".to_string(),
            endpoint: "https://old.example.com".to_string(),
            capabilities: vec![],
            metadata: HashMap::new(),
            registered_at: chrono::Utc::now().timestamp() - 200,
            last_heartbeat: chrono::Utc::now().timestamp() - 200, // 200 秒前
            status: AgentStatus::Online,
        };
        registry.register(agent).await.unwrap();

        // 注册一个"新"Agent
        let fresh_agent = AgentInfo {
            id: "fresh-agent".to_string(),
            name: "Fresh Agent".to_string(),
            description: String::new(),
            version: "1.0".to_string(),
            endpoint: "https://fresh.example.com".to_string(),
            capabilities: vec![],
            metadata: HashMap::new(),
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            status: AgentStatus::Online,
        };
        registry.register(fresh_agent).await.unwrap();

        let config = HeartbeatConfig {
            timeout_secs: 90,
            ..Default::default()
        };
        let monitor = HeartbeatMonitor::new(config, registry.clone());

        let timed_out = monitor.check_timeouts().await;
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], "old-agent");

        let old = registry.get("old-agent").await.unwrap();
        assert_eq!(old.status, AgentStatus::Offline);
    }

    #[tokio::test]
    async fn test_create_heartbeat() {
        let registry = Arc::new(AgentRegistry::new());
        let monitor = HeartbeatMonitor::new(HeartbeatConfig::default(), registry);

        let msg = monitor.create_heartbeat("self", AgentStatus::Online, 2, 42);
        assert_eq!(msg.agent_id, "self");
        assert_eq!(msg.seq, 42);
        assert_eq!(msg.active_tasks, 2);
    }

    use std::collections::HashMap;
}
