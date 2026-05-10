//! # Redis 缓存实现

use super::traits::{Cache, CacheEntry, CacheError, CacheStats};
use async_trait::async_trait;

/// Redis 缓存
///
/// 需要启用 `redis` feature
pub struct RedisCache {
    url: String,
}

impl RedisCache {
    /// 创建 Redis 缓存
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
        }
    }

    /// 获取 Redis 连接
    async fn get_conn(&self) -> Result<redis::aio::MultiplexedConnection, CacheError> {
        let client = redis::Client::open(self.url.as_str())
            .map_err(|e| CacheError::Connection(e.to_string()))?;

        client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| CacheError::Connection(e.to_string()))
    }
}

#[async_trait]
impl Cache for RedisCache {
    async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        let mut conn = self.get_conn().await?;

        let result: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Backend(e.to_string()))?;

        Ok(result)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), CacheError> {
        let mut conn = self.get_conn().await?;

        if ttl_secs > 0 {
            redis::cmd("SETEX")
                .arg(key)
                .arg(ttl_secs)
                .arg(value)
                .query_async(&mut conn)
                .await
                .map_err(|e| CacheError::Backend(e.to_string()))?;
        } else {
            redis::cmd("SET")
                .arg(key)
                .arg(value)
                .query_async(&mut conn)
                .await
                .map_err(|e| CacheError::Backend(e.to_string()))?;
        }

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        let mut conn = self.get_conn().await?;

        redis::cmd("DEL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CacheError> {
        let mut conn = self.get_conn().await?;

        let result: i64 = redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Backend(e.to_string()))?;

        Ok(result > 0)
    }

    async fn stats(&self) -> Result<CacheStats, CacheError> {
        let mut conn = self.get_conn().await?;

        // 获取 keyspace 信息
        let info: std::collections::HashMap<String, String> = redis::cmd("INFO")
            .arg("keyspace")
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Backend(e.to_string()))?;

        // 简化统计：获取总 key 数
        let db_stats: std::collections::HashMap<String, String> = redis::cmd("INFO")
            .arg("stats")
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Backend(e.to_string()))?;

        let hits = db_stats
            .get("keyspace_hits")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let misses = db_stats
            .get("keyspace_misses")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        Ok(CacheStats {
            entries: 0, // Redis 不提供实时计数
            capacity: 0,
            hit_rate,
            total_requests: total,
            hits,
        })
    }

    async fn clear(&self) -> Result<(), CacheError> {
        let mut conn = self.get_conn().await?;

        redis::cmd("FLUSHDB")
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Backend(e.to_string()))?;

        Ok(())
    }

    fn cache_type(&self) -> &'static str {
        "redis"
    }
}
