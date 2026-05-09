//! # 统一错误类型
//!
//! OpenLink Core 的统一错误定义，所有核心层错误都通过此类型返回。

use thiserror::Error;

/// 核心层统一错误类型
#[derive(Error, Debug)]
pub enum CoreError {
    /// 资源未找到
    #[error("Not found: {0}")]
    NotFound(String),

    /// 输入校验失败
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// 扩展相关错误（注册/查找/执行）
    #[error("Extension error: {0}")]
    ExtensionError(String),

    /// 路由引擎错误（匹配/调度失败）
    #[error("Routing error: {0}")]
    RoutingError(String),

    /// 内部错误（不应暴露给外部的实现细节）
    #[error("Internal error: {0}")]
    InternalError(String),
}

// 实现 From 以方便错误转换
impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::InternalError(format!("JSON error: {}", e))
    }
}
