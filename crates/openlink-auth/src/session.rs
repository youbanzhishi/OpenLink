//! Session 会话管理 - 会话级临时凭证与生命周期
//!
//! Session = 权限实例，会话结束自动失效

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use crate::permission::PermissionId;
use super::permission::{UserId, AgentId};

/// 会话唯一标识
pub type SessionId = String;

/// 会话状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// 活跃
    Active,
    /// 已过期
    Expired,
    /// 已撤销
    Revoked,
}

/// 会话配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    /// Access Token TTL（秒）
    pub ttl_seconds: u64,

    /// Refresh Token TTL（秒）
    pub refresh_ttl_seconds: u64,

    /// 最大并发会话数
    pub max_concurrent_sessions: u32,

    /// 是否需要重新认证
    pub require_re_auth: bool,

    /// 会话结束时是否自动撤销权限
    pub auto_revoke_on_session_end: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            ttl_seconds: 3600,           // 1小时
            refresh_ttl_seconds: 86400,  // 1天
            max_concurrent_sessions: 5,
            require_re_auth: false,
            auto_revoke_on_session_end: true,
        }
    }
}

/// 会话元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    /// 来源IP
    pub source_ip: Option<String>,

    /// User-Agent
    pub user_agent: Option<String>,

    /// 创建原因
    pub created_reason: String,

    /// 自定义标签
    pub tags: HashMap<String, String>,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            source_ip: None,
            user_agent: None,
            created_reason: "agent_auth_request".into(),
            tags: HashMap::new(),
        }
    }
}

/// 会话 - 权限的运行时实例
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// 会话唯一标识
    pub session_id: SessionId,

    /// 关联的权限ID
    pub permission_id: PermissionId,

    /// Agent ID
    pub agent_id: AgentId,

    /// User ID
    pub user_id: UserId,

    /// Access Token
    pub access_token: String,

    /// Refresh Token
    pub refresh_token: String,

    /// 签发时间
    pub issued_at: DateTime<Utc>,

    /// 过期时间
    pub expires_at: DateTime<Utc>,

    /// 最后刷新时间
    pub last_refreshed_at: Option<DateTime<Utc>>,

    /// 会话状态
    pub status: SessionStatus,

    /// 会话元数据
    pub metadata: SessionMetadata,
}

impl Session {
    /// 创建新会话（token稍后填充）
    pub fn new(
        permission_id: PermissionId,
        agent_id: AgentId,
        user_id: UserId,
        config: &SessionConfig,
    ) -> Self {
        let issued_at = Utc::now();
        let expires_at = issued_at + chrono::Duration::seconds(config.ttl_seconds as i64);

        Self {
            session_id: Uuid::new_v4().to_string(),
            permission_id,
            agent_id,
            user_id,
            access_token: String::new(),
            refresh_token: String::new(),
            issued_at,
            expires_at,
            last_refreshed_at: None,
            status: SessionStatus::Active,
            metadata: SessionMetadata::default(),
        }
    }

    /// 检查会话是否活跃
    pub fn is_active(&self) -> bool {
        self.status == SessionStatus::Active && Utc::now() < self.expires_at
    }

    /// 刷新会话（更新access_token过期时间）
    pub fn refresh(&mut self, ttl_seconds: u64) {
        self.expires_at = Utc::now() + chrono::Duration::seconds(ttl_seconds as i64);
        self.last_refreshed_at = Some(Utc::now());
    }

    /// 撤销会话
    pub fn revoke(&mut self) {
        self.status = SessionStatus::Revoked;
    }

    /// 检查并标记过期
    pub fn check_expiry(&mut self) -> bool {
        if self.status == SessionStatus::Active && Utc::now() >= self.expires_at {
            self.status = SessionStatus::Expired;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let config = SessionConfig::default();
        let session = Session::new(
            "perm-001".into(),
            "agent-001".into(),
            "user-001".into(),
            &config,
        );

        assert!(!session.session_id.is_empty());
        assert_eq!(session.permission_id, "perm-001");
        assert_eq!(session.status, SessionStatus::Active);
        assert!(session.is_active());
        assert!(session.access_token.is_empty()); // 稍后填充
    }

    #[test]
    fn test_session_refresh() {
        let config = SessionConfig::default();
        let mut session = Session::new(
            "perm-001".into(),
            "agent-001".into(),
            "user-001".into(),
            &config,
        );

        let original_expires = session.expires_at;
        session.refresh(7200); // 刷新为2小时

        assert!(session.expires_at > original_expires);
        assert!(session.last_refreshed_at.is_some());
    }

    #[test]
    fn test_session_revoke() {
        let config = SessionConfig::default();
        let mut session = Session::new(
            "perm-001".into(),
            "agent-001".into(),
            "user-001".into(),
            &config,
        );

        assert!(session.is_active());
        session.revoke();
        assert_eq!(session.status, SessionStatus::Revoked);
        assert!(!session.is_active());
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert_eq!(config.ttl_seconds, 3600);
        assert_eq!(config.refresh_ttl_seconds, 86400);
        assert_eq!(config.max_concurrent_sessions, 5);
        assert!(config.auto_revoke_on_session_end);
    }

    #[test]
    fn test_session_metadata() {
        let mut meta = SessionMetadata::default();
        meta.source_ip = Some("192.168.1.1".into());
        meta.tags.insert("env".into(), "test".into());

        assert_eq!(meta.source_ip.as_deref(), Some("192.168.1.1"));
        assert_eq!(meta.tags.get("env").unwrap(), "test");
    }
}
