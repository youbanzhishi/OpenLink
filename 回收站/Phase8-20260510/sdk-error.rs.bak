//! # SDK 错误类型

use thiserror::Error;

/// SDK 错误类型
#[derive(Error, Debug)]
pub enum SdkError {
    /// 网络错误
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// HTTP 错误
    #[error("HTTP error: {status} - {message}")]
    Http {
        status: u16,
        message: String,
    },

    /// API 错误
    #[error("API error: {0}")]
    Api(String),

    /// 认证错误
    #[error("Authentication error: {0}")]
    Auth(String),

    /// 文件操作错误
    #[error("File error: {0}")]
    File(String),

    /// 序列化错误
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO 错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 其他错误
    #[error("{0}")]
    Other(String),
}

impl SdkError {
    /// 判断是否是认证错误
    pub fn is_auth_error(&self) -> bool {
        matches!(self, SdkError::Auth(_))
    }

    /// 判断是否是 404 错误
    pub fn is_not_found(&self) -> bool {
        matches!(self, SdkError::Http { status: 404, .. })
    }
}
