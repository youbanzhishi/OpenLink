//! # ext-a2a-discovery — Agent-to-Agent 发现扩展
//!
//! 实现 A2A Discovery Action 和 Condition，注册到 Extension Registry。
//!
//! ## 功能
//! - **Action**: `a2a_discovery` — 发现指定能力的 Agent
//! - **Action**: `a2a_handshake` — 与目标 Agent 建立握手
//! - **Action**: `a2a_register` — 注册 Agent 到注册表
//! - **Action**: `a2a_heartbeat` — 发送心跳
//! - **Condition**: `agent-capability` — 检查目标 Agent 是否具备指定能力
//!
//! ## 用法示例
//! ```json
//! {
//!   "action": "a2a_discovery",
//!   "params": {
//!     "capability": "text-generation",
//!     "status": "online"
//!   }
//! }
//! ```

use std::sync::Arc;
use async_trait::async_trait;
use openlink_core::{
    ActionHandler, ConditionHandler, ExtensionRegistry, CoreError,
    ActionResult, Context, Target,
};
use openlink_a2a::{
    AgentRegistry, HandshakeEngine, HeartbeatMonitor,
    DiscoveryQuery, Capability, AgentStatus, AgentInfo, HeartbeatMessage,
};
use serde::{Deserialize, Serialize};

// ─── A2A Discovery Action ──────────────────────────────────

/// A2A 发现 Action
pub struct A2aDiscoveryAction {
    registry: Arc<AgentRegistry>,
}

impl A2aDiscoveryAction {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ActionHandler for A2aDiscoveryAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let capability = target.params.get("capability")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let status = target.params.get("status")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "online" => Some(AgentStatus::Online),
                "offline" => Some(AgentStatus::Offline),
                "busy" => Some(AgentStatus::Busy),
                _ => None,
            });

        let query = DiscoveryQuery {
            capability: if capability.is_empty() { None } else { Some(capability.to_string()) },
            status,
            min_trust: None,
            tags: vec![],
        };

        let agents = self.registry.discover(&query).await;

        let results: Vec<serde_json::Value> = agents.iter().map(|a| {
            serde_json::json!({
                "id": a.id,
                "name": a.name,
                "endpoint": a.endpoint,
                "status": format!("{:?}", a.status).to_lowercase(),
                "capabilities": a.capabilities.iter().map(|c| &c.id).collect::<Vec<_>>(),
            })
        }).collect();

        Ok(ActionResult::Json(serde_json::json!({
            "discovered": results,
            "count": results.len(),
        })))
    }

    fn name(&self) -> &'static str {
        "a2a_discovery"
    }
}

// ─── A2A Handshake Action ──────────────────────────────────

/// A2A 握手 Action
pub struct A2aHandshakeAction {
    handshake: Arc<HandshakeEngine>,
    registry: Arc<AgentRegistry>,
}

impl A2aHandshakeAction {
    pub fn new(handshake: Arc<HandshakeEngine>, registry: Arc<AgentRegistry>) -> Self {
        Self { handshake, registry }
    }
}

#[async_trait]
impl ActionHandler for A2aHandshakeAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let target_agent = target.params.get("target_agent")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::InvalidInput("target_agent is required".to_string()))?;

        let offered: Vec<String> = target.params.get("offered_capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let requested: Vec<String> = target.params.get("requested_capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        // 创建握手请求
        let request = self.handshake.create_handshake_request(
            target_agent,
            offered,
            requested,
        );

        // 获取目标 Agent 的能力
        let agent_info = self.registry.get(target_agent).await;
        let capabilities = agent_info
            .map(|a| a.capabilities)
            .unwrap_or_default();

        // 处理握手
        let response = self.handshake.handle_handshake_request(&request, &capabilities).await;

        Ok(ActionResult::Json(serde_json::json!({
            "accepted": response.accepted,
            "provided_capabilities": response.provided_capabilities,
            "session_token": response.session_token,
            "reject_reason": response.reject_reason,
        })))
    }

    fn name(&self) -> &'static str {
        "a2a_handshake"
    }
}

// ─── A2A Register Action (Phase 6 新增) ────────────────────

/// A2A 注册 Action
pub struct A2aRegisterAction {
    registry: Arc<AgentRegistry>,
}

impl A2aRegisterAction {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ActionHandler for A2aRegisterAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let id = target.params.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::InvalidInput("id is required".to_string()))?;

        let name = target.params.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::InvalidInput("name is required".to_string()))?;

        let endpoint = target.params.get("endpoint")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::InvalidInput("endpoint is required".to_string()))?;

        let version = target.params.get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0.0");

        let description = target.params.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // 解析能力列表
        let capabilities: Vec<Capability> = target.params.get("capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|v| {
                    let cap_id = v.get("id")?.as_str()?.to_string();
                    Some(Capability {
                        id: cap_id,
                        name: v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        description: String::new(),
                        input_format: String::new(),
                        output_format: String::new(),
                        params: serde_json::Value::Null,
                    })
                }).collect()
            })
            .unwrap_or_default();

        let agent_info = AgentInfo {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            version: version.to_string(),
            endpoint: endpoint.to_string(),
            capabilities,
            metadata: std::collections::HashMap::new(),
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            status: AgentStatus::Online,
        };

        match self.registry.register(agent_info).await {
            Ok(()) => Ok(ActionResult::Json(serde_json::json!({
                "status": "registered",
                "agent_id": id,
            }))),
            Err(e) => Ok(ActionResult::Json(serde_json::json!({
                "status": "error",
                "error": format!("{}", e),
            }))),
        }
    }

    fn name(&self) -> &'static str {
        "a2a_register"
    }
}

// ─── A2A Heartbeat Action (Phase 6 新增) ───────────────────

/// A2A 心跳 Action
pub struct A2aHeartbeatAction {
    monitor: Arc<HeartbeatMonitor>,
}

impl A2aHeartbeatAction {
    pub fn new(monitor: Arc<HeartbeatMonitor>) -> Self {
        Self { monitor }
    }
}

#[async_trait]
impl ActionHandler for A2aHeartbeatAction {
    async fn execute(
        &self,
        _ctx: &Context,
        target: &Target,
    ) -> Result<ActionResult, CoreError> {
        let agent_id = target.params.get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::InvalidInput("agent_id is required".to_string()))?;

        let status_str = target.params.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("online");

        let status = match status_str {
            "online" => AgentStatus::Online,
            "busy" => AgentStatus::Busy,
            "offline" => AgentStatus::Offline,
            _ => AgentStatus::Online,
        };

        let active_tasks = target.params.get("active_tasks")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        // 创建心跳消息
        let heartbeat = HeartbeatMessage {
            agent_id: agent_id.to_string(),
            seq: 0, // 由 monitor 递增
            status,
            active_tasks,
            timestamp: chrono::Utc::now().timestamp(),
        };

        // 处理心跳（内部会更新注册表）
        self.monitor.handle_heartbeat(&heartbeat).await.map_err(|e| CoreError::InternalError(e))?;

        Ok(ActionResult::Json(serde_json::json!({
            "status": "acknowledged",
            "agent_id": agent_id,
        })))
    }

    fn name(&self) -> &'static str {
        "a2a_heartbeat"
    }
}

// ─── Agent Capability Condition ─────────────────────────────

/// Agent 能力条件
pub struct AgentCapabilityCondition {
    registry: Arc<AgentRegistry>,
}

impl AgentCapabilityCondition {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ConditionHandler for AgentCapabilityCondition {
    async fn evaluate(
        &self,
        _ctx: &Context,
        params: &serde_json::Value,
    ) -> Result<bool, CoreError> {
        let agent_id = params.get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let capability = params.get("capability")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if agent_id.is_empty() || capability.is_empty() {
            return Ok(false);
        }

        let agent = self.registry.get(agent_id).await;
        match agent {
            Some(info) => {
                Ok(info.capabilities.iter().any(|c| c.id == capability))
            }
            None => Ok(false),
        }
    }

    fn name(&self) -> &'static str {
        "agent-capability"
    }
}

/// 注册 A2A Discovery 扩展到 Extension Registry
pub fn register(
    registry: &mut ExtensionRegistry,
    agent_registry: Arc<AgentRegistry>,
    handshake_engine: Arc<HandshakeEngine>,
    heartbeat_monitor: Arc<HeartbeatMonitor>,
) -> Result<(), CoreError> {
    registry.register_action(Arc::new(A2aDiscoveryAction::new(agent_registry.clone())))?;
    registry.register_action(Arc::new(A2aHandshakeAction::new(handshake_engine, agent_registry.clone())))?;
    registry.register_action(Arc::new(A2aRegisterAction::new(agent_registry.clone())))?;
    registry.register_action(Arc::new(A2aHeartbeatAction::new(agent_registry, heartbeat_monitor)))?;
    registry.register_condition(Arc::new(AgentCapabilityCondition::new(agent_registry)))?;
    Ok(())
}

/// 注册 A2A Discovery 扩展（向后兼容版本，不含心跳）
pub fn register_simple(
    registry: &mut ExtensionRegistry,
    agent_registry: Arc<AgentRegistry>,
    handshake_engine: Arc<HandshakeEngine>,
) -> Result<(), CoreError> {
    let monitor = Arc::new(HeartbeatMonitor::new(
        openlink_a2a::HeartbeatConfig::default(),
        agent_registry.clone(),
    ));
    register(registry, agent_registry, handshake_engine, monitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::Action;

    #[tokio::test]
    async fn test_a2a_discovery_action() {
        let agent_registry = Arc::new(AgentRegistry::new());
        agent_registry.register(AgentInfo {
            id: "agent-1".to_string(),
            name: "Test Agent".to_string(),
            description: String::new(),
            version: "1.0".to_string(),
            endpoint: "https://agent-1.example.com".to_string(),
            capabilities: vec![Capability {
                id: "text-gen".to_string(),
                name: "Text Generation".to_string(),
                description: String::new(),
                input_format: String::new(),
                output_format: String::new(),
                params: serde_json::Value::Null,
            }],
            metadata: std::collections::HashMap::new(),
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            status: AgentStatus::Online,
        }).await.unwrap();

        let action = A2aDiscoveryAction::new(agent_registry);
        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("a2a_discovery".to_string()),
            params: serde_json::json!({"capability": "text-gen"}),
        };

        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["count"], 1);
            }
            _ => panic!("Expected Json result"),
        }
    }

    #[tokio::test]
    async fn test_a2a_register_action() {
        let agent_registry = Arc::new(AgentRegistry::new());
        let action = A2aRegisterAction::new(agent_registry.clone());

        let ctx = Context::from_request(None, None);
        let target = Target {
            action: Action::Custom("a2a_register".to_string()),
            params: serde_json::json!({
                "id": "agent-new",
                "name": "New Agent",
                "endpoint": "https://new.example.com",
                "capabilities": [{"id": "text-gen", "name": "Text Gen"}]
            }),
        };

        let result = action.execute(&ctx, &target).await.unwrap();
        match result {
            ActionResult::Json(val) => {
                assert_eq!(val["status"], "registered");
                assert_eq!(val["agent_id"], "agent-new");
            }
            _ => panic!("Expected Json result"),
        }

        // 验证注册成功
        let agent = agent_registry.get("agent-new").await;
        assert!(agent.is_some());
    }

    #[tokio::test]
    async fn test_agent_capability_condition() {
        let agent_registry = Arc::new(AgentRegistry::new());
        agent_registry.register(AgentInfo {
            id: "agent-1".to_string(),
            name: "Test Agent".to_string(),
            description: String::new(),
            version: "1.0".to_string(),
            endpoint: "https://agent-1.example.com".to_string(),
            capabilities: vec![Capability {
                id: "text-gen".to_string(),
                name: "Text Generation".to_string(),
                description: String::new(),
                input_format: String::new(),
                output_format: String::new(),
                params: serde_json::Value::Null,
            }],
            metadata: std::collections::HashMap::new(),
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            status: AgentStatus::Online,
        }).await.unwrap();

        let condition = AgentCapabilityCondition::new(agent_registry);
        let ctx = Context::from_request(None, None);

        // Should match
        let result = condition.evaluate(&ctx, &serde_json::json!({
            "agent_id": "agent-1",
            "capability": "text-gen"
        })).await.unwrap();
        assert!(result);

        // Should not match
        let result = condition.evaluate(&ctx, &serde_json::json!({
            "agent_id": "agent-1",
            "capability": "image-analysis"
        })).await.unwrap();
        assert!(!result);
    }
}
