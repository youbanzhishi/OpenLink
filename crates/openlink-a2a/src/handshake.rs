//! # Agent 间握手和信任建立
//!
//! Phase 6: 实现 Agent 间的信任握手协议。
//!
//! ## 握手流程
//! 1. 发起方发送 HandshakeRequest
//! 2. 接收方验证请求，返回 HandshakeResponse
//! 3. 成功后建立信任关系，生成会话 Token
//! 4. 信任等级随交互次数递增

use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 握手引擎
pub struct HandshakeEngine {
    /// 信任记录表：(from, to) → TrustRecord
    trust_records: Arc<RwLock<HashMap<(AgentId, AgentId), TrustRecord>>>,
    /// 活跃会话 Token：token → SessionInfo
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
    /// 自身 Agent ID
    self_agent_id: AgentId,
    /// 协议版本
    protocol_version: String,
}

/// 会话信息
#[derive(Debug, Clone)]
struct SessionInfo {
    /// 会话 Token
    token: String,
    /// 对方 Agent ID
    peer_agent_id: AgentId,
    /// 创建时间
    created_at: i64,
    /// 最后活跃时间
    last_active: i64,
}

impl HandshakeEngine {
    /// 创建握手引擎
    pub fn new(self_agent_id: AgentId) -> Self {
        Self {
            trust_records: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            self_agent_id,
            protocol_version: "1.0".to_string(),
        }
    }

    /// 发起握手请求
    pub fn create_handshake_request(
        &self,
        target_agent: &str,
        offered_capabilities: Vec<String>,
        requested_capabilities: Vec<String>,
    ) -> HandshakeRequest {
        HandshakeRequest {
            from_agent: self.self_agent_id.clone(),
            to_agent: target_agent.to_string(),
            offered_capabilities,
            requested_capabilities,
            protocol_version: self.protocol_version.clone(),
            challenge: Some(uuid::Uuid::new_v4().to_string()),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 处理收到的握手请求
    pub async fn handle_handshake_request(
        &self,
        request: &HandshakeRequest,
        available_capabilities: &[Capability],
    ) -> HandshakeResponse {
        // 检查协议版本兼容性
        if request.protocol_version != self.protocol_version {
            return HandshakeResponse {
                accepted: false,
                provided_capabilities: vec![],
                challenge_response: None,
                session_token: None,
                timestamp: chrono::Utc::now().timestamp(),
                reject_reason: Some(format!(
                    "Protocol version mismatch: expected {}, got {}",
                    self.protocol_version, request.protocol_version
                )),
            };
        }

        // 检查请求的能力是否可用
        let available_ids: Vec<&str> = available_capabilities.iter().map(|c| c.id.as_str()).collect();
        let provided: Vec<String> = request
            .requested_capabilities
            .iter()
            .filter(|cap| available_ids.iter().any(|id| *id == cap.as_str()))
            .cloned()
            .collect();

        // 至少满足一个请求的能力才接受握手
        let accepted = !provided.is_empty();

        if accepted {
            // 生成会话 Token
            let session_token = uuid::Uuid::new_v4().to_string();

            // 记录会话
            {
                let mut sessions = self.sessions.write().await;
                let now = chrono::Utc::now().timestamp();
                sessions.insert(
                    session_token.clone(),
                    SessionInfo {
                        token: session_token.clone(),
                        peer_agent_id: request.from_agent.clone(),
                        created_at: now,
                        last_active: now,
                    },
                );
            }

            // 建立初始信任
            {
                let mut records = self.trust_records.write().await;
                let key = (request.from_agent.clone(), self.self_agent_id.clone());
                records
                    .entry(key)
                    .or_insert_with(|| TrustRecord {
                        from_agent: request.from_agent.clone(),
                        to_agent: self.self_agent_id.clone(),
                        trust_level: TrustLevel::Basic,
                        success_count: 0,
                        failure_count: 0,
                        first_interaction: chrono::Utc::now().timestamp(),
                        last_interaction: chrono::Utc::now().timestamp(),
                    });
            }

            tracing::info!(
                from = %request.from_agent,
                provided_capabilities = ?provided,
                "Handshake accepted"
            );

            HandshakeResponse {
                accepted: true,
                provided_capabilities: provided,
                challenge_response: request.challenge.clone(),
                session_token: Some(session_token),
                timestamp: chrono::Utc::now().timestamp(),
                reject_reason: None,
            }
        } else {
            tracing::info!(
                from = %request.from_agent,
                requested = ?request.requested_capabilities,
                available = ?available_ids,
                "Handshake rejected: no matching capabilities"
            );

            HandshakeResponse {
                accepted: false,
                provided_capabilities: vec![],
                challenge_response: None,
                session_token: None,
                timestamp: chrono::Utc::now().timestamp(),
                reject_reason: Some("No matching capabilities".to_string()),
            }
        }
    }

    /// 记录成功交互
    pub async fn record_success(&self, peer_agent: &str) {
        let mut records = self.trust_records.write().await;
        let key = (peer_agent.to_string(), self.self_agent_id.clone());

        if let Some(record) = records.get_mut(&key) {
            record.success_count += 1;
            record.last_interaction = chrono::Utc::now().timestamp();

            // 根据交互次数提升信任等级
            if record.success_count >= 100 {
                record.trust_level = TrustLevel::Trusted;
            } else if record.success_count >= 20 {
                record.trust_level = TrustLevel::Verified;
            }

            tracing::debug!(
                peer = %peer_agent,
                success_count = record.success_count,
                trust_level = ?record.trust_level,
                "Interaction success recorded"
            );
        }
    }

    /// 记录失败交互
    pub async fn record_failure(&self, peer_agent: &str) {
        let mut records = self.trust_records.write().await;
        let key = (peer_agent.to_string(), self.self_agent_id.clone());

        if let Some(record) = records.get_mut(&key) {
            record.failure_count += 1;
            record.last_interaction = chrono::Utc::now().timestamp();

            tracing::debug!(
                peer = %peer_agent,
                failure_count = record.failure_count,
                "Interaction failure recorded"
            );
        }
    }

    /// 获取信任记录
    pub async fn get_trust(&self, peer_agent: &str) -> Option<TrustRecord> {
        let records = self.trust_records.read().await;
        let key = (peer_agent.to_string(), self.self_agent_id.clone());
        records.get(&key).cloned()
    }

    /// 验证会话 Token
    pub async fn validate_session(&self, token: &str) -> Option<AgentId> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(token) {
            session.last_active = chrono::Utc::now().timestamp();
            Some(session.peer_agent_id.clone())
        } else {
            None
        }
    }

    /// 清理过期会话
    pub async fn cleanup_expired_sessions(&self, max_age_secs: i64) -> usize {
        let mut sessions = self.sessions.write().await;
        let now = chrono::Utc::now().timestamp();
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| now - s.last_active > max_age_secs)
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired.len();
        for token in expired {
            sessions.remove(&token);
        }

        if count > 0 {
            tracing::info!(count, "Expired sessions cleaned up");
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_handshake_request() {
        let engine = HandshakeEngine::new("agent-a".to_string());
        let request = engine.create_handshake_request(
            "agent-b",
            vec!["text-gen".to_string()],
            vec!["image-analysis".to_string()],
        );

        assert_eq!(request.from_agent, "agent-a");
        assert_eq!(request.to_agent, "agent-b");
        assert!(request.challenge.is_some());
    }

    #[tokio::test]
    async fn test_handshake_accepted() {
        let engine = HandshakeEngine::new("agent-b".to_string());
        let request = HandshakeRequest {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            offered_capabilities: vec!["text-gen".to_string()],
            requested_capabilities: vec!["image-analysis".to_string()],
            protocol_version: "1.0".to_string(),
            challenge: Some("challenge-123".to_string()),
            timestamp: 1234567890,
        };

        let capabilities = vec![Capability {
            id: "image-analysis".to_string(),
            name: "Image Analysis".to_string(),
            description: String::new(),
            input_format: String::new(),
            output_format: String::new(),
            params: serde_json::Value::Null,
        }];

        let response = engine.handle_handshake_request(&request, &capabilities).await;
        assert!(response.accepted);
        assert!(response.session_token.is_some());
        assert_eq!(response.provided_capabilities, vec!["image-analysis"]);
    }

    #[tokio::test]
    async fn test_handshake_rejected_no_capabilities() {
        let engine = HandshakeEngine::new("agent-b".to_string());
        let request = HandshakeRequest {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            offered_capabilities: vec![],
            requested_capabilities: vec!["nonexistent".to_string()],
            protocol_version: "1.0".to_string(),
            challenge: None,
            timestamp: 1234567890,
        };

        let response = engine.handle_handshake_request(&request, &[]).await;
        assert!(!response.accepted);
        assert!(response.reject_reason.is_some());
    }

    #[tokio::test]
    async fn test_handshake_version_mismatch() {
        let engine = HandshakeEngine::new("agent-b".to_string());
        let request = HandshakeRequest {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            offered_capabilities: vec![],
            requested_capabilities: vec!["cap".to_string()],
            protocol_version: "2.0".to_string(),
            challenge: None,
            timestamp: 1234567890,
        };

        let response = engine.handle_handshake_request(&request, &[]).await;
        assert!(!response.accepted);
    }

    #[tokio::test]
    async fn test_trust_record() {
        let engine = HandshakeEngine::new("agent-b".to_string());

        // 先建立握手
        let request = HandshakeRequest {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            offered_capabilities: vec![],
            requested_capabilities: vec!["cap".to_string()],
            protocol_version: "1.0".to_string(),
            challenge: None,
            timestamp: 1234567890,
        };
        let caps = vec![Capability {
            id: "cap".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            input_format: String::new(),
            output_format: String::new(),
            params: serde_json::Value::Null,
        }];
        engine.handle_handshake_request(&request, &caps).await;

        // 记录成功交互
        engine.record_success("agent-a").await;

        let trust = engine.get_trust("agent-a").await.unwrap();
        assert_eq!(trust.trust_level, TrustLevel::Basic);
        assert_eq!(trust.success_count, 1);
    }

    #[tokio::test]
    async fn test_session_validation() {
        let engine = HandshakeEngine::new("agent-b".to_string());

        let request = HandshakeRequest {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            offered_capabilities: vec![],
            requested_capabilities: vec!["cap".to_string()],
            protocol_version: "1.0".to_string(),
            challenge: None,
            timestamp: 1234567890,
        };
        let caps = vec![Capability {
            id: "cap".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            input_format: String::new(),
            output_format: String::new(),
            params: serde_json::Value::Null,
        }];
        let response = engine.handle_handshake_request(&request, &caps).await;

        let token = response.session_token.unwrap();
        let peer = engine.validate_session(&token).await;
        assert_eq!(peer, Some("agent-a".to_string()));

        // 无效 token
        let invalid = engine.validate_session("invalid-token").await;
        assert!(invalid.is_none());
    }
}
