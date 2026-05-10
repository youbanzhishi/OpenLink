//! # 缓存预热（Phase 5）
//!
//! 启动时从外部数据源加载高频链到缓存。
//! 支持从文件、HTTP API 等数据源预热缓存。

use super::traits::{Cache, CacheError};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tracing;

/// 预热条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadEntry {
    /// 缓存键
    pub key: String,
    /// 缓存值
    pub value: String,
    /// TTL（秒）
    pub ttl_secs: u64,
}

/// 预热数据源 trait — 由外部实现
#[async_trait::async_trait]
pub trait PreloadSource: Send + Sync {
    /// 加载预热条目
    async fn load(&self) -> Result<Vec<PreloadEntry>, PreloadError>;

    /// 数据源名称
    fn name(&self) -> &str;
}

/// 预热错误
#[derive(Debug, thiserror::Error)]
pub enum PreloadError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Data source unavailable: {0}")]
    Unavailable(String),
}

/// 文件预热数据源
///
/// 从 JSON 文件加载预热条目。
/// 文件格式：`[{"key": "...", "value": "...", "ttl_secs": 3600}, ...]`
pub struct FilePreloadSource {
    path: String,
}

impl FilePreloadSource {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait::async_trait]
impl PreloadSource for FilePreloadSource {
    async fn load(&self) -> Result<Vec<PreloadEntry>, PreloadError> {
        let content = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|e| PreloadError::Io(e.to_string()))?;

        let entries: Vec<PreloadEntry> = serde_json::from_str(&content)
            .map_err(|e| PreloadError::Parse(e.to_string()))?;

        tracing::info!(
            path = %self.path,
            count = entries.len(),
            "Preload entries loaded from file"
        );

        Ok(entries)
    }

    fn name(&self) -> &str {
        "file"
    }
}

/// 静态预热数据源
///
/// 直接在代码中提供预热条目，用于测试和快速启动。
pub struct StaticPreloadSource {
    entries: Vec<PreloadEntry>,
}

impl StaticPreloadSource {
    pub fn new(entries: Vec<PreloadEntry>) -> Self {
        Self { entries }
    }
}

#[async_trait::async_trait]
impl PreloadSource for StaticPreloadSource {
    async fn load(&self) -> Result<Vec<PreloadEntry>, PreloadError> {
        Ok(self.entries.clone())
    }

    fn name(&self) -> &str {
        "static"
    }
}

/// 缓存预热器
pub struct CachePreloader {
    cache: Arc<dyn Cache>,
}

impl CachePreloader {
    pub fn new(cache: Arc<dyn Cache>) -> Self {
        Self { cache }
    }

    /// 从数据源预热缓存
    pub async fn preload(&self, source: &dyn PreloadSource) -> Result<PreloadResult, CacheError> {
        let entries = source.load().await
            .map_err(|e| CacheError::Backend(format!("Preload source error: {}", e)))?;

        let total = entries.len();
        let mut loaded = 0;
        let mut failed = 0;

        let warmup_entries: Vec<(&str, &str, u64)> = entries.iter()
            .map(|e| (e.key.as_str(), e.value.as_str(), e.ttl_secs))
            .collect();

        match self.cache.warmup(warmup_entries).await {
            Ok(()) => loaded = total,
            Err(e) => {
                tracing::error!(error = %e, "Batch warmup failed, falling back to individual sets");
                // 逐个加载
                for entry in &entries {
                    match self.cache.set(&entry.key, &entry.value, entry.ttl_secs).await {
                        Ok(()) => loaded += 1,
                        Err(e) => {
                            tracing::warn!(key = %entry.key, error = %e, "Failed to preload entry");
                            failed += 1;
                        }
                    }
                }
            }
        }

        tracing::info!(
            source = %source.name(),
            total,
            loaded,
            failed,
            "Cache preload completed"
        );

        Ok(PreloadResult {
            source: source.name().to_string(),
            total,
            loaded,
            failed,
        })
    }
}

/// 预热结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreloadResult {
    /// 数据源名称
    pub source: String,
    /// 总条目数
    pub total: usize,
    /// 成功加载条目数
    pub loaded: usize,
    /// 失败条目数
    pub failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryCache;

    #[tokio::test]
    async fn test_static_preload() {
        let cache = Arc::new(MemoryCache::new(100));
        let preloader = CachePreloader::new(cache.clone());

        let source = StaticPreloadSource::new(vec![
            PreloadEntry {
                key: "key1".to_string(),
                value: "value1".to_string(),
                ttl_secs: 3600,
            },
            PreloadEntry {
                key: "key2".to_string(),
                value: "value2".to_string(),
                ttl_secs: 1800,
            },
        ]);

        let result = preloader.preload(&source).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.loaded, 2);
        assert_eq!(result.failed, 0);

        // 验证缓存中有数据
        assert_eq!(cache.get("key1").await.unwrap(), Some("value1".to_string()));
        assert_eq!(cache.get("key2").await.unwrap(), Some("value2".to_string()));
    }

    #[tokio::test]
    async fn test_empty_preload() {
        let cache = Arc::new(MemoryCache::new(100));
        let preloader = CachePreloader::new(cache.clone());

        let source = StaticPreloadSource::new(vec![]);
        let result = preloader.preload(&source).await.unwrap();
        assert_eq!(result.total, 0);
        assert_eq!(result.loaded, 0);
    }
}
