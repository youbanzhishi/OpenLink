//! Memory服务 - 跨任务Context缓存
//!
//! 核心理念：避免重复解析，缓存命中率统计
//! - LRU淘汰 + TTL过期 + 按任务ID分区
//! - 与Context原语集成：请求处理前检查缓存，命中则跳过Extension调用

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 缓存Key
pub type CacheKey = String;

/// 任务ID
pub type TaskId = String;

/// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// 缓存Key
    pub key: CacheKey,

    /// 所属任务ID
    pub task_id: TaskId,

    /// 缓存值
    pub value: serde_json::Value,

    /// TTL（秒）
    pub ttl_seconds: u64,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 最后访问时间
    pub last_accessed_at: DateTime<Utc>,

    /// 访问次数
    pub access_count: u64,
}

impl CacheEntry {
    pub fn new(key: CacheKey, task_id: TaskId, value: serde_json::Value, ttl_seconds: u64) -> Self {
        let now = Utc::now();
        Self {
            key,
            task_id,
            value,
            ttl_seconds,
            created_at: now,
            last_accessed_at: now,
            access_count: 0,
        }
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        let now = Utc::now();
        let expires_at = self.created_at + chrono::Duration::seconds(self.ttl_seconds as i64);
        now > expires_at
    }

    /// 标记访问
    pub fn touch(&mut self) {
        self.last_accessed_at = Utc::now();
        self.access_count += 1;
    }
}

/// 缓存命中统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 淘汰次数
    pub evictions: u64,
    /// 当前条目数
    pub entry_count: usize,
    /// 当前内存占用估算（字节）
    pub estimated_size_bytes: u64,
}

impl CacheStats {
    /// 命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Memory服务 trait
#[async_trait::async_trait]
pub trait MemoryService: Send + Sync {
    /// 获取缓存（命中返回Some，未命中返回None）
    async fn get(&self, task_id: &TaskId, key: &CacheKey) -> Option<CacheEntry>;

    /// 设置缓存
    async fn set(&self, entry: CacheEntry) -> Result<(), String>;

    /// 使缓存失效
    async fn invalidate(&self, task_id: &TaskId, key: &CacheKey) -> bool;

    /// 按任务ID清除所有缓存
    async fn invalidate_task(&self, task_id: &TaskId) -> u64;

    /// 清理过期缓存
    async fn cleanup_expired(&self) -> u64;

    /// 获取统计
    async fn stats(&self) -> CacheStats;
}

/// 内存MemoryService实现（LRU + TTL + 任务分区）
#[derive(Debug)]
pub struct InMemoryCache {
    entries: Arc<RwLock<HashMap<(TaskId, CacheKey), CacheEntry>>>,
    max_entries: usize,
    stats: Arc<RwLock<CacheStats>>,
}

impl InMemoryCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }
}

#[async_trait::async_trait]
impl MemoryService for InMemoryCache {
    async fn get(&self, task_id: &TaskId, key: &CacheKey) -> Option<CacheEntry> {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        if let Some(entry) = entries.get_mut(&(task_id.clone(), key.clone())) {
            if entry.is_expired() {
                entries.remove(&(task_id.clone(), key.clone()));
                stats.misses += 1;
                return None;
            }
            entry.touch();
            stats.hits += 1;
            Some(entry.clone())
        } else {
            stats.misses += 1;
            None
        }
    }

    async fn set(&self, entry: CacheEntry) -> Result<(), String> {
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        // LRU淘汰
        if entries.len() >= self.max_entries {
            // 找到最久未访问的条目
            if let Some(oldest_key) = entries.iter()
                .min_by_key(|(_, e)| e.last_accessed_at)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest_key);
                stats.evictions += 1;
            }
        }

        let composite_key = (entry.task_id.clone(), entry.key.clone());
        entries.insert(composite_key, entry);
        stats.entry_count = entries.len();

        Ok(())
    }

    async fn invalidate(&self, task_id: &TaskId, key: &CacheKey) -> bool {
        let mut entries = self.entries.write().await;
        entries.remove(&(task_id.clone(), key.clone())).is_some()
    }

    async fn invalidate_task(&self, task_id: &TaskId) -> u64 {
        let mut entries = self.entries.write().await;
        let keys_to_remove: Vec<_> = entries.keys()
            .filter(|(tid, _)| tid == task_id)
            .cloned()
            .collect();
        let count = keys_to_remove.len() as u64;
        for key in keys_to_remove {
            entries.remove(&key);
        }
        count
    }

    async fn cleanup_expired(&self) -> u64 {
        let mut entries = self.entries.write().await;
        let expired_keys: Vec<_> = entries.iter()
            .filter(|(_, e)| e.is_expired())
            .map(|(k, _)| k.clone())
            .collect();
        let count = expired_keys.len() as u64;
        for key in expired_keys {
            entries.remove(&key);
        }
        count
    }

    async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        let mut stats = self.stats.read().await.clone();
        stats.entry_count = entries.len();
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_cache_set_and_get() {
        let cache = InMemoryCache::new(100);
        let entry = CacheEntry::new(
            "key1".into(), "task1".into(), json!({"data": "hello"}), 3600,
        );

        cache.set(entry).await.unwrap();
        let result = cache.get(&"task1".into(), &"key1".into()).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().value["data"], "hello");
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = InMemoryCache::new(100);
        let result = cache.get(&"task1".into(), &"nonexistent".into()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_lru_eviction() {
        let cache = InMemoryCache::new(2);

        cache.set(CacheEntry::new("k1".into(), "t1".into(), json!(1), 3600)).await.unwrap();
        cache.set(CacheEntry::new("k2".into(), "t1".into(), json!(2), 3600)).await.unwrap();

        // 访问k1使其更"新"
        cache.get(&"t1".into(), &"k1".into()).await;

        // 添加第3个，应该淘汰k2（最久未访问）
        cache.set(CacheEntry::new("k3".into(), "t1".into(), json!(3), 3600)).await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.evictions, 1);
        assert!(cache.get(&"t1".into(), &"k1".into()).await.is_some());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = InMemoryCache::new(100);
        cache.set(CacheEntry::new("k1".into(), "t1".into(), json!(1), 3600)).await.unwrap();

        let removed = cache.invalidate(&"t1".into(), &"k1".into()).await;
        assert!(removed);

        let result = cache.get(&"t1".into(), &"k1".into()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_task_partition() {
        let cache = InMemoryCache::new(100);
        cache.set(CacheEntry::new("k1".into(), "t1".into(), json!("v1"), 3600)).await.unwrap();
        cache.set(CacheEntry::new("k1".into(), "t2".into(), json!("v2"), 3600)).await.unwrap();

        // 同key不同task是不同条目
        let r1 = cache.get(&"t1".into(), &"k1".into()).await.unwrap();
        let r2 = cache.get(&"t2".into(), &"k1".into()).await.unwrap();
        assert_eq!(r1.value, json!("v1"));
        assert_eq!(r2.value, json!("v2"));

        // 清除t1不影响t2
        cache.invalidate_task(&"t1".into()).await;
        assert!(cache.get(&"t1".into(), &"k1".into()).await.is_none());
        assert!(cache.get(&"t2".into(), &"k1".into()).await.is_some());
    }

    #[tokio::test]
    async fn test_cache_hit_rate_stats() {
        let cache = InMemoryCache::new(100);
        cache.set(CacheEntry::new("k1".into(), "t1".into(), json!(1), 3600)).await.unwrap();

        // 1 hit
        cache.get(&"t1".into(), &"k1".into()).await;
        // 1 miss
        cache.get(&"t1".into(), &"nonexistent".into()).await;

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.5).abs() < 0.01);
    }
}
