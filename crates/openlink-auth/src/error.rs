//! 错误类型定义

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Permission not found: {0}")]
    PermissionNotFound(String),

    #[error("Permission not active: {0}")]
    PermissionNotActive(String),

    #[error("Permission expired: {0}")]
    PermissionExpired(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Session expired: {0}")]
    SessionExpired(String),

    #[error("Session revoked: {0}")]
    SessionRevoked(String),

    #[error("Too many concurrent sessions for agent: {0}")]
    TooManySessions(String),

    #[error("Extension not allowed: {0}")]
    ExtensionNotAllowed(String),

    #[error("Operation not allowed: {0}")]
    OperationNotAllowed(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Token error: {0}")]
    TokenError(#[from] TokenError),

    #[error("Store error: {0}")]
    StoreError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Token encoding failed: {0}")]
    Encode(String),

    #[error("Token decoding failed: {0}")]
    Decode(String),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Token expired")]
    Expired,

    #[error("Token already used (jti): {0}")]
    AlreadyUsed(String),

    #[error("Missing key")]
    MissingKey,
}

#[derive(Debug, Error)]
pub enum SessionStoreError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Session already exists: {0}")]
    AlreadyExists(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Concurrency conflict: {0}")]
    Conflict(String),
}
