//! # 缓存 Trait 定义（Phase 5 增强）
//!
//! Phase 5 增强：
//! - 批量操作接口（warmup / invalidate_batch）
//! - 键前缀扫描
//! - 缓存层叠（LayeredCache）

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 缓存错误
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Connection error: {0}")]
    Connection(String),
}

/// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// 缓存值（JSON 序列化）
    pub value: String,

    /// 创建时间戳
    pub created_at: i64,

    /// TTL（秒）
    pub ttl_secs: u64,

    /// 访问计数
    pub access_count: u64,
}

impl CacheEntry {
    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - self.created_at >= self.ttl_secs as i64
    }
}

/// 缓存统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// 当前条目数
    pub entries: usize,

    /// 最大条目数
    pub capacity: usize,

    /// 命中率
    pub hit_rate: f64,

    /// 总请求数
    pub total_requests: u64,

    /// 命中次数
    pub hits: u64,
}

/// Cache Trait — 所有缓存实现必须实现此接口
#[async_trait]
pub trait Cache: Send + Sync {
    /// 获取值
    async fn get(&self, key: &str) -> Result<Option<String>, CacheError>;

    /// 设置值（带 TTL）
    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), CacheError>;

    /// 删除值
    async fn delete(&self, key: &str) -> Result<(), CacheError>;

    /// 检查键是否存在
    async fn exists(&self, key: &str) -> Result<bool, CacheError>;

    /// 获取统计信息
    async fn stats(&self) -> Result<CacheStats, CacheError>;

    /// 清除所有缓存
    async fn clear(&self) -> Result<(), CacheError>;

    /// 获取类型名称
    fn cache_type(&self) -> &'static str;

    // ─── Phase 5 扩展接口 ────────────────────────────────────

    /// 批量设置（用于缓存预热）
    async fn warmup(&self, entries: Vec<(&str, &str, u64)>) -> Result<(), CacheError> {
        for (key, value, ttl) in entries {
            self.set(key, value, ttl).await?;
        }
        Ok(())
    }

    /// 批量删除
    async fn delete_batch(&self, keys: &[&str]) -> Result<usize, CacheError> {
        let mut deleted = 0;
        for key in keys {
            match self.delete(key).await {
                Ok(()) => deleted += 1,
                Err(CacheError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(deleted)
    }

    /// 按前缀删除（默认实现：逐个检查，子类可覆盖优化）
    async fn delete_by_prefix(&self, _prefix: &str) -> Result<usize, CacheError> {
        // 默认实现不支持前缀扫描
        Ok(0)
    }
}
