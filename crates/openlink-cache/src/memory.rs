//! # 内存缓存实现（Phase 5 增强）
//!
//! Phase 5 增强：
//! - 缓存预热接口
//! - 按前缀删除
//! - 主动失效（TTL + 显式删除）

use super::traits::{Cache, CacheEntry, CacheStats, CacheError};
use async_trait::async_trait;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// 内存缓存
pub struct MemoryCache {
    cache: Arc<Mutex<LruCache<String, CacheEntry>>>,
    /// 访问统计
    stats: Arc<RwLock<CacheInternalStats>>,
    capacity: usize,
}

/// 内部统计
#[derive(Debug, Clone, Default)]
struct CacheInternalStats {
    total_requests: u64,
    hits: u64,
}

impl MemoryCache {
    /// 创建新内存缓存
    pub fn new(capacity: usize) -> Self {
        let cache = LruCache::new(
            NonZeroUsize::new(capacity.max(1)).unwrap_or(NonZeroUsize::MIN)
        );

        Self {
            cache: Arc::new(Mutex::new(cache)),
            stats: Arc::new(RwLock::new(CacheInternalStats::default())),
            capacity: capacity.max(1),
        }
    }

    /// 获取缓存条目（带过期检查）
    async fn get_entry(&self, key: &str) -> Result<Option<CacheEntry>, CacheError> {
        let mut cache = self.cache.lock().await;

        if let Some(entry) = cache.get(key) {
            if entry.is_expired() {
                cache.pop(key);
                return Ok(None);
            }

            // 更新访问计数
            let mut entry = entry.clone();
            entry.access_count += 1;
            cache.put(key.to_string(), entry.clone());

            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    /// 获取所有键列表（用于前缀匹配）
    async fn keys(&self) -> Vec<String> {
        let cache = self.cache.lock().await;
        cache.iter().map(|(k, _)| k.clone()).collect()
    }
}

#[async_trait]
impl Cache for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
        }

        match self.get_entry(key).await {
            Ok(Some(entry)) => {
                let mut stats = self.stats.write().await;
                stats.hits += 1;
                Ok(Some(entry.value))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), CacheError> {
        let mut cache = self.cache.lock().await;

        cache.put(key.to_string(), CacheEntry {
            value: value.to_string(),
            created_at: chrono::Utc::now().timestamp(),
            ttl_secs,
            access_count: 0,
        });

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        let mut cache = self.cache.lock().await;
        if cache.pop(key).is_none() {
            return Err(CacheError::NotFound(key.to_string()));
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CacheError> {
        match self.get_entry(key).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn stats(&self) -> Result<CacheStats, CacheError> {
        let cache = self.cache.lock().await;
        let internal = self.stats.read().await;

        let total = internal.total_requests;
        let hits = internal.hits;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        Ok(CacheStats {
            entries: cache.len(),
            capacity: self.capacity,
            hit_rate,
            total_requests: total,
            hits,
        })
    }

    async fn clear(&self) -> Result<(), CacheError> {
        let mut cache = self.cache.lock().await;
        cache.clear();
        Ok(())
    }

    fn cache_type(&self) -> &'static str {
        "memory"
    }

    async fn warmup(&self, entries: Vec<(&str, &str, u64)>) -> Result<(), CacheError> {
        let mut cache = self.cache.lock().await;
        let now = chrono::Utc::now().timestamp();
        for (key, value, ttl) in entries {
            cache.put(key.to_string(), CacheEntry {
                value: value.to_string(),
                created_at: now,
                ttl_secs: ttl,
                access_count: 0,
            });
        }
        tracing::info!(count = cache.len(), "Cache warmup completed");
        Ok(())
    }

    async fn delete_batch(&self, keys: &[&str]) -> Result<usize, CacheError> {
        let mut cache = self.cache.lock().await;
        let mut deleted = 0;
        for key in keys {
            if cache.pop(*key).is_some() {
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    async fn delete_by_prefix(&self, prefix: &str) -> Result<usize, CacheError> {
        let keys_to_delete = {
            let cache = self.cache.lock().await;
            cache
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, _)| k.clone())
                .collect::<Vec<_>>()
        };

        let mut cache = self.cache.lock().await;
        let mut deleted = 0;
        for key in keys_to_delete {
            if cache.pop(&key).is_some() {
                deleted += 1;
            }
        }

        tracing::info!(prefix = %prefix, deleted, "Prefix deletion completed");
        Ok(deleted)
    }
}

/// 层叠缓存（L1 内存 + L2 可扩展）
///
/// 查询时先查 L1，miss 后查 L2 并回填 L1。
/// 写入时同时写 L1 和 L2。
pub struct LayeredCache {
    l1: MemoryCache,
    /// L2 缓存（可选，如 Redis）
    l2: Option<Arc<dyn Cache>>,
}

impl LayeredCache {
    /// 创建两层缓存
    pub fn new(l1_capacity: usize, l2: Option<Arc<dyn Cache>>) -> Self {
        Self {
            l1: MemoryCache::new(l1_capacity),
            l2,
        }
    }
}

#[async_trait]
impl Cache for LayeredCache {
    async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        // 先查 L1
        if let Some(value) = self.l1.get(key).await? {
            return Ok(Some(value));
        }

        // L1 miss，查 L2
        if let Some(l2) = &self.l2 {
            if let Some(value) = l2.get(key).await? {
                // 回填 L1
                self.l1.set(key, &value, 3600).await?;
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), CacheError> {
        // 同时写 L1 和 L2
        self.l1.set(key, value, ttl_secs).await?;
        if let Some(l2) = &self.l2 {
            l2.set(key, value, ttl_secs).await?;
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.l1.delete(key).await.ok(); // L1 可能不存在
        if let Some(l2) = &self.l2 {
            l2.delete(key).await.ok();
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CacheError> {
        if self.l1.exists(key).await? {
            return Ok(true);
        }
        if let Some(l2) = &self.l2 {
            return l2.exists(key).await;
        }
        Ok(false)
    }

    async fn stats(&self) -> Result<CacheStats, CacheError> {
        self.l1.stats().await
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.l1.clear().await?;
        if let Some(l2) = &self.l2 {
            l2.clear().await?;
        }
        Ok(())
    }

    fn cache_type(&self) -> &'static str {
        "layered"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get() {
        let cache = MemoryCache::new(100);
        cache.set("key1", "value1", 3600).await.unwrap();

        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_get_miss() {
        let cache = MemoryCache::new(100);
        let result = cache.get("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_delete() {
        let cache = MemoryCache::new(100);
        cache.set("key1", "value1", 3600).await.unwrap();
        cache.delete("key1").await.unwrap();

        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_expiry() {
        let cache = MemoryCache::new(100);
        cache.set("key1", "value1", 1).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_stats() {
        let cache = MemoryCache::new(100);
        cache.set("key1", "value1", 3600).await.unwrap();

        cache.get("nonexistent").await.unwrap();
        cache.get("key1").await.unwrap();

        let stats = cache.stats().await.unwrap();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.hits, 1);
        assert!((stats.hit_rate - 0.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_eviction() {
        let cache = MemoryCache::new(3);

        cache.set("a", "1", 3600).await.unwrap();
        cache.set("b", "2", 3600).await.unwrap();
        cache.set("c", "3", 3600).await.unwrap();

        cache.get("a").await.unwrap();

        cache.set("d", "4", 3600).await.unwrap();

        let result = cache.get("b").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_warmup() {
        let cache = MemoryCache::new(100);
        cache.warmup(vec![
            ("k1", "v1", 3600),
            ("k2", "v2", 3600),
            ("k3", "v3", 3600),
        ]).await.unwrap();

        assert_eq!(cache.get("k1").await.unwrap(), Some("v1".to_string()));
        assert_eq!(cache.get("k2").await.unwrap(), Some("v2".to_string()));
        assert_eq!(cache.get("k3").await.unwrap(), Some("v3".to_string()));
    }

    #[tokio::test]
    async fn test_delete_by_prefix() {
        let cache = MemoryCache::new(100);
        cache.set("abc_1", "v1", 3600).await.unwrap();
        cache.set("abc_2", "v2", 3600).await.unwrap();
        cache.set("xyz_1", "v3", 3600).await.unwrap();

        let deleted = cache.delete_by_prefix("abc_").await.unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(cache.get("abc_1").await.unwrap(), None);
        assert_eq!(cache.get("xyz_1").await.unwrap(), Some("v3".to_string()));
    }

    #[tokio::test]
    async fn test_delete_batch() {
        let cache = MemoryCache::new(100);
        cache.set("a", "1", 3600).await.unwrap();
        cache.set("b", "2", 3600).await.unwrap();
        cache.set("c", "3", 3600).await.unwrap();

        let deleted = cache.delete_batch(&["a", "b", "nonexistent"]).await.unwrap();
        assert_eq!(deleted, 2);
    }

    #[tokio::test]
    async fn test_layered_cache() {
        let layered = LayeredCache::new(50, None); // 无 L2
        layered.set("key1", "value1", 3600).await.unwrap();
        assert_eq!(layered.get("key1").await.unwrap(), Some("value1".to_string()));
        assert_eq!(layered.cache_type(), "layered");
    }
}
