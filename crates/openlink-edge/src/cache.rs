//! # 边缘缓存
//!
//! 轻量级 LRU 缓存，用于热链缓存。

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

/// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// 目标 URL
    pub target_url: String,
    
    /// 状态码
    pub status_code: u16,
    
    /// 创建时间戳
    pub created_at: i64,
    
    /// 访问次数
    pub access_count: u64,
}

/// 边缘缓存
pub struct EdgeCache {
    cache: Arc<Mutex<LruCache<String, CacheEntry>>>,
    ttl_secs: u64,
    max_entries: usize,
}

impl EdgeCache {
    /// 创建新缓存
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        let cache = LruCache::new(NonZeroUsize::new(max_entries.max(1)).unwrap_or(NonZeroUsize::MIN));
        Self {
            cache: Arc::new(Mutex::new(cache)),
            ttl_secs,
            max_entries: max_entries.max(1),
        }
    }
    
    /// 获取缓存条目
    pub async fn get(&self, code: &str) -> Option<CacheEntry> {
        let mut cache = self.cache.lock().await;
        
        // 检查 TTL
        if let Some(entry) = cache.get(code) {
            let now = chrono::Utc::now().timestamp();
            if now - entry.created_at > self.ttl_secs as i64 {
                cache.pop(code);
                return None;
            }
            
            // 更新访问计数
            let mut entry = entry.clone();
            entry.access_count += 1;
            cache.put(code.to_string(), entry.clone());
            return Some(entry);
        }
        
        None
    }
    
    /// 插入缓存条目
    pub async fn put(&self, code: String, target_url: String, status_code: u16) {
        let mut cache = self.cache.lock().await;
        cache.put(code, CacheEntry {
            target_url,
            status_code,
            created_at: chrono::Utc::now().timestamp(),
            access_count: 0,
        });
    }
    
    /// 清除所有缓存
    pub async fn clear(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
    }
    
    /// 获取缓存统计
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().await;
        CacheStats {
            entries: cache.len(),
            capacity: self.max_entries,
        }
    }
}

/// 缓存统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cache_put_get() {
        let cache = EdgeCache::new(100, 3600);
        cache.put("test".to_string(), "https://example.com".to_string(), 302).await;
        
        let entry = cache.get("test").await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().target_url, "https://example.com");
    }
    
    #[tokio::test]
    async fn test_cache_miss() {
        let cache = EdgeCache::new(100, 3600);
        let entry = cache.get("nonexistent").await;
        assert!(entry.is_none());
    }
}
