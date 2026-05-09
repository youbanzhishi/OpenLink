//! # 存储层错误类型

use thiserror::Error;

/// 存储层错误
#[derive(Error, Debug)]
pub enum StoreError {
    /// 资源未找到
    #[error("Not found: {0}")]
    NotFound(String),

    /// 重复（如短码冲突）
    #[error("Duplicate: {0}")]
    Duplicate(String),

    /// 数据库错误
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// 内部错误
    #[error("Internal error: {0}")]
    InternalError(String),
}

// 从 sqlx 错误转换
impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => StoreError::NotFound("Row not found".to_string()),
            sqlx::Error::Database(ref db_err) => {
                // SQLite UNIQUE constraint violation
                if db_err.message().contains("UNIQUE constraint failed") {
                    StoreError::Duplicate(db_err.message().to_string())
                } else {
                    StoreError::DatabaseError(db_err.message().to_string())
                }
            }
            _ => StoreError::DatabaseError(format!("{}", e)),
        }
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::InternalError(format!("JSON error: {}", e))
    }
}
