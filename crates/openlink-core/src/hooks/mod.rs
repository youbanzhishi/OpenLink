//! Hooks 模块 - 路由钩子系统

use crate::primitives::{Action, Context, Route};

pub mod monitor;
pub mod permission;

// WO-080: 导出核心Hook trait和类型（供permission.rs使用）
pub use monitor::{
    ErrorRateMonitorHook, LatencyMonitorHook, MonitorAdvice, MonitorContext, MonitorEngine, MonitorHook,
};
pub use permission::{PermissionContextExt, PermissionHook, PermissionHookConfig};

// WO-080: 核心Hook trait定义（所有Hook需实现）
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn before_route(&self, ctx: &dyn HookContext) -> HookResult;
    fn after_route(&self, ctx: &dyn HookContext) -> HookResult;
}

// WO-080: Hook上下文trait
pub trait HookContext: Send + Sync {
    fn request_id(&self) -> &str;
    fn action(&self) -> Option<&Action>;
    fn context(&self) -> Option<&Context>;
    fn route(&self) -> Option<&Route>;
}

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
