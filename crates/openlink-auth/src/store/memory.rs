//! 内存SessionStore实现 - 用于测试和轻量部署

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::SessionStoreError;
use super::super::super::session::{Session, SessionId, SessionStatus};
use super::super::super::permission::{UserId, AgentId};
use super::SessionStore;

/// 内存会话存储
#[derive(Debug, Clone)]
pub struct InMemorySessionStore {
    sessions: Arc<RwLock<HashMap<SessionId, Session>>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(&self, session: &Session) -> Result<(), SessionStoreError> {
        let mut sessions = self.sessions.write().await;
        if sessions.contains_key(&session.session_id) {
            return Err(SessionStoreError::AlreadyExists(session.session_id.clone()));
        }
        sessions.insert(session.session_id.clone(), session.clone());
        Ok(())
    }

    async fn get(&self, session_id: &SessionId) -> Result<Option<Session>, SessionStoreError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    async fn update(&self, session: &Session) -> Result<(), SessionStoreError> {
        let mut sessions = self.sessions.write().await;
        if !sessions.contains_key(&session.session_id) {
            return Err(SessionStoreError::NotFound(session.session_id.clone()));
        }
        sessions.insert(session.session_id.clone(), session.clone());
        Ok(())
    }

    async fn delete(&self, session_id: &SessionId) -> Result<(), SessionStoreError> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id)
            .ok_or(SessionStoreError::NotFound(session_id.clone()))?;
        Ok(())
    }

    async fn list_by_user(&self, user_id: &UserId) -> Result<Vec<Session>, SessionStoreError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values()
            .filter(|s| s.user_id == *user_id)
            .cloned()
            .collect())
    }

    async fn list_by_agent(&self, agent_id: &AgentId) -> Result<Vec<Session>, SessionStoreError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values()
            .filter(|s| s.agent_id == *agent_id)
            .cloned()
            .collect())
    }

    async fn revoke(&self, session_id: &SessionId) -> Result<(), SessionStoreError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(session_id)
            .ok_or(SessionStoreError::NotFound(session_id.clone()))?;
        session.revoke();
        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<u64, SessionStoreError> {
        let mut sessions = self.sessions.write().await;
        let expired_ids: Vec<SessionId> = sessions.iter()
            .filter(|(_, s)| s.status != SessionStatus::Active)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired_ids.len() as u64;
        for id in expired_ids {
            sessions.remove(&id);
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionConfig;

    async fn test_store() -> InMemorySessionStore {
        InMemorySessionStore::new()
    }

    fn test_session() -> Session {
        Session::new(
            "perm-001".into(),
            "agent-001".into(),
            "user-001".into(),
            &SessionConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let store = test_store().await;
        let session = test_session();

        store.create(&session).await.unwrap();
        let retrieved = store.get(&session.session_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().session_id, session.session_id);
    }

    #[tokio::test]
    async fn test_duplicate_create() {
        let store = test_store().await;
        let session = test_session();

        store.create(&session).await.unwrap();
        let result = store.create(&session).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_session() {
        let store = test_store().await;
        let mut session = test_session();
        store.create(&session).await.unwrap();

        session.access_token = "new-token".into();
        store.update(&session).await.unwrap();

        let retrieved = store.get(&session.session_id).await.unwrap().unwrap();
        assert_eq!(retrieved.access_token, "new-token");
    }

    #[tokio::test]
    async fn test_delete_session() {
        let store = test_store().await;
        let session = test_session();
        store.create(&session).await.unwrap();

        store.delete(&session.session_id).await.unwrap();
        let retrieved = store.get(&session.session_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_list_by_user() {
        let store = test_store().await;
        let session1 = Session::new("p1".into(), "a1".into(), "user-001".into(), &SessionConfig::default());
        let session2 = Session::new("p2".into(), "a2".into(), "user-001".into(), &SessionConfig::default());
        let session3 = Session::new("p3".into(), "a3".into(), "user-002".into(), &SessionConfig::default());

        store.create(&session1).await.unwrap();
        store.create(&session2).await.unwrap();
        store.create(&session3).await.unwrap();

        let user_sessions = store.list_by_user(&"user-001".into()).await.unwrap();
        assert_eq!(user_sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_agent() {
        let store = test_store().await;
        let session1 = Session::new("p1".into(), "agent-001".into(), "u1".into(), &SessionConfig::default());
        let session2 = Session::new("p2".into(), "agent-002".into(), "u2".into(), &SessionConfig::default());

        store.create(&session1).await.unwrap();
        store.create(&session2).await.unwrap();

        let agent_sessions = store.list_by_agent(&"agent-001".into()).await.unwrap();
        assert_eq!(agent_sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_revoke_session() {
        let store = test_store().await;
        let session = test_session();
        store.create(&session).await.unwrap();

        store.revoke(&session.session_id).await.unwrap();
        let retrieved = store.get(&session.session_id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, SessionStatus::Revoked);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let store = test_store().await;
        let mut session1 = test_session();
        session1.revoke(); // 设为Revoked
        let session2 = test_session();

        store.create(&session1).await.unwrap();
        store.create(&session2).await.unwrap();

        let cleaned = store.cleanup_expired().await.unwrap();
        assert_eq!(cleaned, 1);

        // Active的还在
        let remaining = store.get(&session2.session_id).await.unwrap();
        assert!(remaining.is_some());
    }
}
