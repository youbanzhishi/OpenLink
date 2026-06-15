//! OpenLink Auth Module - Agent Permission & Session Management
//!
//! 实现User+Agent双维度权限模型和Session生命周期管理

pub mod error;
pub mod permission;
pub mod session;
pub mod store;
pub mod token;

pub use error::AuthError;
pub use permission::{AgentPermission, AgentType, Operation, PermissionId, PermissionStatus, ResourceLimits};
pub use session::{Session, SessionConfig, SessionId, SessionMetadata, SessionStatus};
pub use token::TokenGenerator;
