//! # 边缘缓存（Phase 5 增强）
//!
//! 轻量级 LRU 缓存，用于热链缓存。
//! Phase 5 增强：
//! - 热链缓存：高频短链的边缘缓存策略
//! - TTL + 主动失效
//! - 缓存命中率统计
//! - 批量预热接口

use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// 目标 URL
    pub target_url: String,

    /// 状态码
    pub status_code: u16,

    /// 创建时间戳
    pub created_at: i64,

    /// 过期时间戳（TTL-based）
    pub expires_at: i64,

    /// 访问次数
    pub access_count: u64,

    /// 是否为热链（高频访问标记）
    pub is_hot: bool,
}

impl CacheEntry {
    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now >= self.expires_at
    }

    /// 标记为热链
    pub fn mark_hot(&mut self) {
        self.is_hot = true;
    }

    /// 创建新条目
    pub fn new(target_url: String, status_code: u16, ttl_secs: u64) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            target_url,
            status_code,
            created_at: now,
            expires_at: now + ttl_secs as i64,
            access_count: 0,
            is_hot: false,
        }
    }
}

/// 热链阈值配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotLinkConfig {
    /// 标记为热链的访问次数阈值
    pub hot_threshold: u64,
    /// 热链延长 TTL 倍数
    pub hot_ttl_multiplier: u64,
}

impl Default for HotLinkConfig {
    fn default() -> Self {
        Self {
            hot_threshold: 10,
            hot_ttl_multiplier: 3,
        }
    }
}

/// 边缘缓存
pub struct EdgeCache {
    cache: Arc<Mutex<LruCache<String, CacheEntry>>>,
    ttl_secs: u64,
    max_entries: usize,
    hot_config: HotLinkConfig,
    /// 统计：命中次数
    hits: Arc<Mutex<u64>>,
    /// 统计：未命中次数
    misses: Arc<Mutex<u64>>,
    /// 统计：过期失效次数
    expirations: Arc<Mutex<u64>>,
    /// 统计：主动失效次数
    invalidations: Arc<Mutex<u64>>,
}

impl EdgeCache {
    /// 创建新缓存
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self::with_hot_config(max_entries, ttl_secs, HotLinkConfig::default())
    }

    /// 创建新缓存（带热链配置）
    pub fn with_hot_config(max_entries: usize, ttl_secs: u64, hot_config: HotLinkConfig) -> Self {
        let cache =
            LruCache::new(NonZeroUsize::new(max_entries.max(1)).unwrap_or(NonZeroUsize::MIN));
        Self {
            cache: Arc::new(Mutex::new(cache)),
            ttl_secs,
            max_entries: max_entries.max(1),
            hot_config,
            hits: Arc::new(Mutex::new(0)),
            misses: Arc::new(Mutex::new(0)),
            expirations: Arc::new(Mutex::new(0)),
            invalidations: Arc::new(Mutex::new(0)),
        }
    }

    /// 获取缓存条目
    pub async fn get(&self, code: &str) -> Option<CacheEntry> {
        let mut cache = self.cache.lock().await;

        if let Some(entry) = cache.get(code) {
            // 检查 TTL
            if entry.is_expired() {
                cache.pop(code);
                let mut expirations = self.expirations.lock().await;
                *expirations += 1;
                let mut misses = self.misses.lock().await;
                *misses += 1;
                return None;
            }

            // 更新访问计数
            let mut entry = entry.clone();
            entry.access_count += 1;

            // 热链检测：达到阈值后标记并延长 TTL
            if !entry.is_hot && entry.access_count >= self.hot_config.hot_threshold {
                entry.mark_hot();
                entry.expires_at = chrono::Utc::now().timestamp()
                    + (self.ttl_secs as i64 * self.hot_config.hot_ttl_multiplier as i64);
                tracing::info!(
                    code = %code,
                    access_count = entry.access_count,
                    "Link marked as hot, TTL extended"
                );
            }

            cache.put(code.to_string(), entry.clone());

            let mut hits = self.hits.lock().await;
            *hits += 1;

            return Some(entry);
        }

        let mut misses = self.misses.lock().await;
        *misses += 1;
        None
    }

    /// 插入缓存条目
    pub async fn put(&self, code: String, target_url: String, status_code: u16) {
        let mut cache = self.cache.lock().await;
        let entry = CacheEntry::new(target_url, status_code, self.ttl_secs);
        cache.put(code, entry);
    }

    /// 插入缓存条目（带自定义 TTL）
    pub async fn put_with_ttl(
        &self,
        code: String,
        target_url: String,
        status_code: u16,
        ttl_secs: u64,
    ) {
        let mut cache = self.cache.lock().await;
        let entry = CacheEntry::new(target_url, status_code, ttl_secs);
        cache.put(code, entry);
    }

    /// 批量插入（用于缓存预热）
    pub async fn warmup(&self, entries: Vec<(String, String, u16)>) {
        let mut cache = self.cache.lock().await;
        for (code, target_url, status_code) in entries {
            let entry = CacheEntry::new(target_url, status_code, self.ttl_secs);
            cache.put(code, entry);
        }
        tracing::info!(count = cache.len(), "Cache warmup completed");
    }

    /// 主动失效：按 key 删除
    pub async fn invalidate(&self, code: &str) -> bool {
        let mut cache = self.cache.lock().await;
        let removed = cache.pop(code).is_some();
        if removed {
            let mut invalidations = self.invalidations.lock().await;
            *invalidations += 1;
        }
        removed
    }

    /// 主动失效：按前缀批量删除
    pub async fn invalidate_prefix(&self, prefix: &str) -> usize {
        let mut cache = self.cache.lock().await;
        let keys_to_remove: Vec<String> = cache
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| k.clone())
            .collect();

        let count = keys_to_remove.len();
        for key in keys_to_remove {
            cache.pop(&key);
        }

        if count > 0 {
            let mut invalidations = self.invalidations.lock().await;
            *invalidations += count as u64;
        }

        tracing::info!(prefix = %prefix, count, "Prefix invalidation completed");
        count
    }

    /// 主动失效：全部清除
    pub async fn clear(&self) {
        let mut cache = self.cache.lock().await;
        let count = cache.len();
        cache.clear();

        let mut invalidations = self.invalidations.lock().await;
        *invalidations += count as u64;

        tracing::info!("Cache cleared");
    }

    /// 获取缓存统计
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().await;
        let hits = *self.hits.lock().await;
        let misses = *self.misses.lock().await;
        let expirations = *self.expirations.lock().await;
        let invalidations = *self.invalidations.lock().await;
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        // 统计热链数量
        let hot_count = cache.iter().filter(|(_, v)| v.is_hot).count();

        CacheStats {
            entries: cache.len(),
            capacity: self.max_entries,
            hit_rate,
            hits,
            misses,
            expirations,
            invalidations,
            hot_count,
        }
    }

    /// 获取所有热链条目
    pub async fn get_hot_links(&self) -> Vec<(String, CacheEntry)> {
        let cache = self.cache.lock().await;
        cache
            .iter()
            .filter(|(_, v)| v.is_hot)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// 缓存统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub capacity: usize,
    pub hit_rate: f64,
    pub hits: u64,
    pub misses: u64,
    pub expirations: u64,
    pub invalidations: u64,
    pub hot_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_put_get() {
        let cache = EdgeCache::new(100, 3600);
        cache
            .put("test".to_string(), "https://example.com".to_string(), 302)
            .await;

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

    #[tokio::test]
    async fn test_cache_expiry() {
        let cache = EdgeCache::new(100, 3600);
        cache
            .put_with_ttl(
                "ttl1".to_string(),
                "https://example.com".to_string(),
                302,
                1,
            )
            .await;

        // 短暂等待让 TTL 过期
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let entry = cache.get("ttl1").await;
        assert!(entry.is_none());
    }

    #[tokio::test]
    async fn test_hot_link_detection() {
        let hot_config = HotLinkConfig {
            hot_threshold: 3,
            hot_ttl_multiplier: 5,
        };
        let cache = EdgeCache::with_hot_config(100, 3600, hot_config);
        cache
            .put("hot1".to_string(), "https://example.com".to_string(), 302)
            .await;

        // 访问 3 次达到热链阈值
        for _ in 0..3 {
            let _ = cache.get("hot1").await;
        }

        // 第 3 次访问后应该被标记为热链
        let entry = cache.get("hot1").await.unwrap();
        assert!(entry.is_hot);
        assert!(entry.access_count >= 4); // 3次 + 这次 get
    }

    #[tokio::test]
    async fn test_cache_warmup() {
        let cache = EdgeCache::new(100, 3600);
        let entries = vec![
            ("link1".to_string(), "https://a.com".to_string(), 301),
            ("link2".to_string(), "https://b.com".to_string(), 302),
            ("link3".to_string(), "https://c.com".to_string(), 302),
        ];
        cache.warmup(entries).await;

        assert!(cache.get("link1").await.is_some());
        assert!(cache.get("link2").await.is_some());
        assert!(cache.get("link3").await.is_some());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = EdgeCache::new(100, 3600);
        cache
            .put("del1".to_string(), "https://example.com".to_string(), 302)
            .await;

        assert!(cache.get("del1").await.is_some());
        assert!(cache.invalidate("del1").await);
        assert!(cache.get("del1").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate_prefix() {
        let cache = EdgeCache::new(100, 3600);
        cache
            .put("abc_1".to_string(), "https://a.com".to_string(), 302)
            .await;
        cache
            .put("abc_2".to_string(), "https://b.com".to_string(), 302)
            .await;
        cache
            .put("xyz_1".to_string(), "https://c.com".to_string(), 302)
            .await;

        let removed = cache.invalidate_prefix("abc_").await;
        assert_eq!(removed, 2);
        assert!(cache.get("abc_1").await.is_none());
        assert!(cache.get("abc_2").await.is_none());
        assert!(cache.get("xyz_1").await.is_some());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = EdgeCache::new(100, 3600);
        cache
            .put("key1".to_string(), "https://example.com".to_string(), 302)
            .await;

        cache.get("nonexistent").await; // miss
        cache.get("key1").await; // hit

        let stats = cache.stats().await;
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.5).abs() < 0.01);
    }
}
