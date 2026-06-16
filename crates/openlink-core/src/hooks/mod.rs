//! Hooks 模块 - 路由钩子系统

use crate::error::CoreError;
use crate::primitives::{Action, Context, Route};

pub mod monitor;
pub mod permission;

// WO-080: 导出核心Hook trait和类型（供permission.rs使用）
pub use monitor::{
    ErrorRateMonitorHook, LatencyMonitorHook, MonitorAdvice, MonitorContext, MonitorEngine, MonitorHook,
};
pub use permission::{PermissionContextExt, PermissionHook, PermissionHookConfig};

// WO-080: HookAdvice - Hook执行建议
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookAdvice {
    /// 继续执行
    Continue,
    /// 拒绝执行（带原因）
    Reject(String),
}

impl HookAdvice {
    pub fn continue_() -> Self {
        Self::Continue
    }
    pub fn reject(reason: impl Into<String>) -> Self {
        Self::Reject(reason.into())
    }
}

// WO-080: HookResult - Hook执行结果
#[derive(Debug, Clone)]
pub struct HookResult {
    pub success: bool,
    pub error: Option<String>,
    pub advice: HookAdvice,
}

impl HookResult {
    pub fn continue_() -> Self {
        Self {
            success: true,
            error: None,
            advice: HookAdvice::Continue,
        }
    }
    pub fn reject(error: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error.into()),
            advice: HookAdvice::Reject("rejected".into()),
        }
    }
}

impl From<Result<HookAdvice, CoreError>> for HookResult {
    fn from(result: Result<HookAdvice, CoreError>) -> Self {
        match result {
            Ok(advice) => Self {
                success: advice == HookAdvice::Continue,
                error: None,
                advice,
            },
            Err(e) => Self {
                success: false,
                error: Some(e.to_string()),
                advice: HookAdvice::Reject(e.to_string()),
            },
        }
    }
}

// WO-080: 核心Hook trait定义（所有Hook需实现）
// 完整签名来自permission.rs的实现要求
pub trait Hook: Send + Sync {
    type Config;

    fn name(&self) -> &'static str;
    fn hook_type(&self) -> &'static str;
    fn execute(&self, ctx: &mut dyn HookContext) -> HookResult;
    fn on_error(&self, ctx: &dyn HookContext, error: &CoreError) -> HookResult;
}

// WO-080: Hook上下文trait
// 完整方法签名来自permission.rs的PermissionHook实现
pub trait HookContext: Send + Sync {
    fn request_id(&self) -> &str;
    fn path(&self) -> &str;
    fn auth_header(&self) -> Option<&str>;
    fn action(&self) -> Option<&Action>;
    fn context(&self) -> Option<&Context>;
    fn route(&self) -> Option<&Route>;
    fn extension_id(&self) -> Option<&str>;
    fn agent_permission(&self) -> Option<&crate::auth::AgentPermissionContext>;
    fn is_extension_allowed(&self, ext_id: &str) -> bool;
    fn is_operation_allowed(&self, action: &Action) -> bool;
}
