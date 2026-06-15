//! OpenLink Auth Module - Agent Permission & Session Management
//!
//! 实现User+Agent双维度权限模型和Session生命周期管理

pub mod permission;
pub mod session;
pub mod token;
pub mod store;
pub mod error;

pub use permission::{AgentPermission, ResourceLimits, Operation, AgentType, PermissionStatus, PermissionId};
pub use session::{Session, SessionId, SessionStatus, SessionConfig, SessionMetadata};
pub use token::TokenGenerator;
pub use error::AuthError;
