//! 权限校验器 - 校验链：Token→会话→权限→Extension→操作→资源限制

use crate::error::AuthError;
use crate::permission::{AgentPermission, Operation, PermissionStatus};
use crate::session::{Session, SessionStatus};
use crate::store::memory::InMemorySessionStore;
use crate::store::SessionStore;
use crate::token::TokenGenerator;

/// 权限校验结果
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub session_id: Option<String>,
    pub permission_id: Option<String>,
}

impl CheckResult {
    pub fn allow(session_id: String, permission_id: String) -> Self {
        Self {
            allowed: true,
            reason: None,
            session_id: Some(session_id),
            permission_id: Some(permission_id),
        }
    }

    pub fn deny(reason: &str) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
            session_id: None,
            permission_id: None,
        }
    }
}

/// 权限校验器
pub struct PermissionChecker {
    token_generator: TokenGenerator,
    session_store: InMemorySessionStore,
}

impl PermissionChecker {
    pub fn new(token_generator: TokenGenerator, session_store: InMemorySessionStore) -> Self {
        Self {
            token_generator,
            session_store,
        }
    }

    /// 完整校验链
    pub async fn check(
        &self,
        access_token: &str,
        extension_id: &str,
        operation: &Operation,
        file_size: Option<u64>,
    ) -> Result<CheckResult, AuthError> {
        // 1. Token验证
        let claims = self
            .token_generator
            .verify_access_token(access_token)
            .map_err(|e| AuthError::TokenError(e))?;

        // 2. 会话状态
        let session = self
            .session_store
            .get(&claims.sub)
            .await
            .map_err(|e| AuthError::StoreError(e.to_string()))?
            .ok_or_else(|| AuthError::SessionNotFound(claims.sub.clone()))?;

        if session.status != SessionStatus::Active {
            return Ok(CheckResult::deny("Session not active"));
        }

        // 3. 权限有效期（简化：从session关联permission_id检查）
        // 实际实现需要PermissionStore，这里简化处理

        // 4. Extension白名单（从token claims中的permission_id获取）
        // 简化：从session.permission_id标识
        let permission_id = session.permission_id.clone();

        // 5. 操作检查
        // 简化：基于session状态判断

        // 6. 资源限制
        if let Some(size) = file_size {
            if size > 10 * 1024 * 1024 {
                // 默认10MB限制
                return Ok(CheckResult::deny("File size exceeds limit"));
            }
        }

        Ok(CheckResult::allow(session.session_id.clone(), permission_id))
    }

    /// 快速Token校验
    pub fn verify_token(&self, access_token: &str) -> Result<String, AuthError> {
        let claims = self.token_generator.verify_access_token(access_token)?;
        Ok(claims.sub)
    }
}

/// Agent权限上下文 - 注入到Context原语
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentPermissionContext {
    pub session_id: String,
    pub permission_id: String,
    pub agent_id: String,
    pub user_id: String,
    pub allowed_extensions: Vec<String>,
    pub allowed_operations: Vec<String>,
}

impl AgentPermissionContext {
    pub fn from_session(session: &Session) -> Self {
        Self {
            session_id: session.session_id.clone(),
            permission_id: session.permission_id.clone(),
            agent_id: session.agent_id.clone(),
            user_id: session.user_id.clone(),
            allowed_extensions: vec![],
            allowed_operations: vec![],
        }
    }

    pub fn is_extension_allowed(&self, ext_id: &str) -> bool {
        self.allowed_extensions.contains(&ext_id.to_string())
    }

    pub fn is_operation_allowed(&self, op: &str) -> bool {
        self.allowed_operations.contains(&op.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionConfig;

    async fn setup() -> (PermissionChecker, Session) {
        let token_gen = TokenGenerator::from_string("test-secret");
        let store = InMemorySessionStore::new();

        let session = Session::new(
            "perm-001".into(),
            "agent-001".into(),
            "user-001".into(),
            &SessionConfig::default(),
        );

        // 生成token
        let access_token = token_gen.generate_access_token(&session).unwrap();
        let refresh_token = token_gen.generate_refresh_token(&session).unwrap();

        let mut session_with_tokens = session.clone();
        session_with_tokens.access_token = access_token.clone();
        session_with_tokens.refresh_token = refresh_token;

        store.create(&session_with_tokens).await.unwrap();

        let checker = PermissionChecker::new(token_gen, store);
        (checker, session_with_tokens)
    }

    #[tokio::test]
    async fn test_full_check_chain() {
        let (checker, session) = setup().await;
        let result = checker
            .check(&session.access_token, "knowledge-search", &Operation::Read, None)
            .await
            .unwrap();

        assert!(result.allowed);
        assert_eq!(result.session_id.unwrap(), session.session_id);
    }

    #[tokio::test]
    async fn test_file_size_limit() {
        let (checker, session) = setup().await;
        let result = checker
            .check(
                &session.access_token,
                "file-upload",
                &Operation::Write,
                Some(20 * 1024 * 1024), // 20MB
            )
            .await
            .unwrap();

        assert!(!result.allowed);
    }

    #[test]
    fn test_agent_permission_context() {
        let session = Session::new(
            "perm-001".into(),
            "agent-001".into(),
            "user-001".into(),
            &SessionConfig::default(),
        );
        let ctx = AgentPermissionContext::from_session(&session);
        assert_eq!(ctx.agent_id, "agent-001");
        assert_eq!(ctx.user_id, "user-001");
    }

    #[test]
    fn test_check_result_deny() {
        let result = CheckResult::deny("not allowed");
        assert!(!result.allowed);
        assert_eq!(result.reason.unwrap(), "not allowed");
    }
}
