//! 会话存储抽象 - 内存实现 + SQLite持久化

pub mod memory;

use super::permission::AgentId;
use super::permission::UserId;
use super::session::{Session, SessionId};
use crate::error::SessionStoreError;
use async_trait::async_trait;

/// 会话存储 trait
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// 创建会话
    async fn create(&self, session: &Session) -> Result<(), SessionStoreError>;

    /// 获取会话
    async fn get(&self, session_id: &SessionId) -> Result<Option<Session>, SessionStoreError>;

    /// 更新会话
    async fn update(&self, session: &Session) -> Result<(), SessionStoreError>;

    /// 删除会话
    async fn delete(&self, session_id: &SessionId) -> Result<(), SessionStoreError>;

    /// 列出用户的会话
    async fn list_by_user(&self, user_id: &UserId) -> Result<Vec<Session>, SessionStoreError>;

    /// 列出Agent的会话
    async fn list_by_agent(&self, agent_id: &AgentId) -> Result<Vec<Session>, SessionStoreError>;

    /// 撤销会话
    async fn revoke(&self, session_id: &SessionId) -> Result<(), SessionStoreError>;

    /// 清理过期会话
    async fn cleanup_expired(&self) -> Result<u64, SessionStoreError>;
}
