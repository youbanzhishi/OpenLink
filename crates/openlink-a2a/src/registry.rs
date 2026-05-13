//! # Agent 注册表
//!
//! Agent 的注册、发现和查询。

use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// Agent 注册表
pub struct AgentRegistry {
    /// 已注册的 Agent 列表
    agents: Arc<RwLock<HashMap<AgentId, AgentInfo>>>,
}

impl AgentRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册 Agent
    pub async fn register(&self, info: AgentInfo) -> Result<(), RegistryError> {
        let mut agents = self.agents.write().await;

        if agents.contains_key(&info.id) {
            return Err(RegistryError::AlreadyRegistered(info.id));
        }

        tracing::info!(
            agent_id = %info.id,
            name = %info.name,
            capabilities = info.capabilities.len(),
            "Agent registered"
        );

        agents.insert(info.id.clone(), info);
        Ok(())
    }

    /// 注销 Agent
    pub async fn deregister(&self, agent_id: &str) -> Result<AgentInfo, RegistryError> {
        let mut agents = self.agents.write().await;

        agents
            .remove(agent_id)
            .ok_or_else(|| RegistryError::NotFound(agent_id.to_string()))
    }

    /// 更新 Agent 信息
    pub async fn update(&self, agent_id: &str, update_fn: impl FnOnce(&mut AgentInfo)) -> Result<(), RegistryError> {
        let mut agents = self.agents.write().await;

        let info = agents
            .get_mut(agent_id)
            .ok_or_else(|| RegistryError::NotFound(agent_id.to_string()))?;

        update_fn(info);
        Ok(())
    }

    /// 更新心跳
    pub async fn update_heartbeat(
        &self,
        agent_id: &str,
        status: AgentStatus,
        active_tasks: u32,
    ) -> Result<(), RegistryError> {
        let mut agents = self.agents.write().await;

        let info = agents
            .get_mut(agent_id)
            .ok_or_else(|| RegistryError::NotFound(agent_id.to_string()))?;

        info.last_heartbeat = chrono::Utc::now().timestamp();
        tracing::debug!(
            agent_id = %agent_id,
            status = ?status,
            active_tasks,
            "Heartbeat updated"
        );

        info.status = status;

        Ok(())
    }

    /// 获取 Agent 信息
    pub async fn get(&self, agent_id: &str) -> Option<AgentInfo> {
        let agents = self.agents.read().await;
        agents.get(agent_id).cloned()
    }

    /// 发现 Agent（按条件查询）
    pub async fn discover(&self, query: &DiscoveryQuery) -> Vec<AgentInfo> {
        let agents = self.agents.read().await;

        agents
            .values()
            .filter(|agent| {
                // 按能力过滤
                if let Some(ref cap) = query.capability {
                    if !agent.capabilities.iter().any(|c| c.id == *cap) {
                        return false;
                    }
                }

                // 按状态过滤
                if let Some(ref status) = query.status {
                    if agent.status != *status {
                        return false;
                    }
                }

                // 按标签过滤
                if !query.tags.is_empty() {
                    let agent_tags: Vec<&str> = agent
                        .metadata
                        .get("tags")
                        .map(|s| s.split(',').map(|t| t.trim()).collect())
                        .unwrap_or_default();
                    if !query.tags.iter().any(|tag| agent_tags.iter().any(|t| t == tag)) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    /// 列出所有已注册 Agent
    pub async fn list_all(&self) -> Vec<AgentInfo> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// 获取注册 Agent 数量
    pub async fn count(&self) -> usize {
        let agents = self.agents.read().await;
        agents.len()
    }

    /// 获取在线 Agent 数量
    pub async fn online_count(&self) -> usize {
        let agents = self.agents.read().await;
        agents.values().filter(|a| a.status == AgentStatus::Online).count()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 注册表错误
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Agent already registered: {0}")]
    AlreadyRegistered(AgentId),

    #[error("Agent not found: {0}")]
    NotFound(String),

    #[error("Invalid agent info: {0}")]
    InvalidInfo(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_agent(id: &str) -> AgentInfo {
        AgentInfo {
            id: id.to_string(),
            name: format!("Test Agent {}", id),
            description: "A test agent".to_string(),
            version: "1.0.0".to_string(),
            endpoint: format!("https://{}.agent.example.com", id),
            capabilities: vec![Capability {
                id: "text-gen".to_string(),
                name: "Text Generation".to_string(),
                description: "Generate text".to_string(),
                input_format: "text/plain".to_string(),
                output_format: "text/plain".to_string(),
                params: serde_json::Value::Null,
            }],
            metadata: HashMap::new(),
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            status: AgentStatus::Online,
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = AgentRegistry::new();
        let agent = make_test_agent("agent-1");

        registry.register(agent.clone()).await.unwrap();
        let retrieved = registry.get("agent-1").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Agent agent-1");
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let registry = AgentRegistry::new();
        let agent = make_test_agent("agent-1");

        registry.register(agent.clone()).await.unwrap();
        assert!(registry.register(agent).await.is_err());
    }

    #[tokio::test]
    async fn test_deregister() {
        let registry = AgentRegistry::new();
        let agent = make_test_agent("agent-1");

        registry.register(agent).await.unwrap();
        registry.deregister("agent-1").await.unwrap();
        assert!(registry.get("agent-1").await.is_none());
    }

    #[tokio::test]
    async fn test_discover_by_capability() {
        let registry = AgentRegistry::new();

        let mut agent1 = make_test_agent("agent-1");
        agent1.capabilities = vec![Capability {
            id: "text-gen".to_string(),
            name: "Text Generation".to_string(),
            description: String::new(),
            input_format: String::new(),
            output_format: String::new(),
            params: serde_json::Value::Null,
        }];

        let mut agent2 = make_test_agent("agent-2");
        agent2.capabilities = vec![Capability {
            id: "image-analysis".to_string(),
            name: "Image Analysis".to_string(),
            description: String::new(),
            input_format: String::new(),
            output_format: String::new(),
            params: serde_json::Value::Null,
        }];

        registry.register(agent1).await.unwrap();
        registry.register(agent2).await.unwrap();

        let query = DiscoveryQuery {
            capability: Some("text-gen".to_string()),
            status: None,
            min_trust: None,
            tags: vec![],
        };

        let results = registry.discover(&query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "agent-1");
    }

    #[tokio::test]
    async fn test_discover_by_status() {
        let registry = AgentRegistry::new();

        let mut agent1 = make_test_agent("agent-1");
        agent1.status = AgentStatus::Online;

        let mut agent2 = make_test_agent("agent-2");
        agent2.status = AgentStatus::Offline;

        registry.register(agent1).await.unwrap();
        registry.register(agent2).await.unwrap();

        let query = DiscoveryQuery {
            capability: None,
            status: Some(AgentStatus::Online),
            min_trust: None,
            tags: vec![],
        };

        let results = registry.discover(&query).await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_update_heartbeat() {
        let registry = AgentRegistry::new();
        let agent = make_test_agent("agent-1");

        registry.register(agent).await.unwrap();
        registry
            .update_heartbeat("agent-1", AgentStatus::Busy, 5)
            .await
            .unwrap();

        let updated = registry.get("agent-1").await.unwrap();
        assert_eq!(updated.status, AgentStatus::Busy);
    }
}
