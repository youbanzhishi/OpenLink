//! # 内存缓存实现

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
        cache.pop(key);
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
        // TTL 为 0 应该立即过期
        cache.set("key1", "value1", 1).await.unwrap();
        
        // 短暂延迟
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, None);
    }
    
    #[tokio::test]
    async fn test_stats() {
        let cache = MemoryCache::new(100);
        cache.set("key1", "value1", 3600).await.unwrap();
        
        // 第一次 get - miss
        cache.get("nonexistent").await.unwrap();
        // 第二次 get - hit
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
        
        // 触发 a 的访问
        cache.get("a").await.unwrap();
        
        // 添加新条目，应该驱逐 b
        cache.set("d", "4", 3600).await.unwrap();
        
        let result = cache.get("b").await.unwrap();
        assert_eq!(result, None); // b 已被驱逐
    }
}
