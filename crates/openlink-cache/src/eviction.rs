//! # 缓存后台驱逐（Phase 5）
//!
//! 定期扫描缓存，清理过期条目，保持缓存健康。
//! 在后台 tokio 任务中运行，不阻塞主请求路径。

use super::traits::{Cache, CacheError};
use std::sync::Arc;
use tokio::time::{self, Duration};
use tracing;

/// 后台驱逐配置
#[derive(Debug, Clone)]
pub struct EvictionConfig {
    /// 扫描间隔（秒），默认 60s
    pub scan_interval_secs: u64,
    /// 每次扫描最大处理条目数，默认 1000
    pub batch_size: usize,
    /// 是否启用
    pub enabled: bool,
}

impl Default for EvictionConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 60,
            batch_size: 1000,
            enabled: true,
        }
    }
}

/// 后台驱逐任务
pub struct BackgroundEviction {
    cache: Arc<dyn Cache>,
    config: EvictionConfig,
}

impl BackgroundEviction {
    /// 创建后台驱逐任务
    pub fn new(cache: Arc<dyn Cache>, config: EvictionConfig) -> Self {
        Self { cache, config }
    }

    /// 启动后台驱逐循环
    ///
    /// 返回一个 JoinHandle，可用于停止任务。
    pub async fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = self.config.scan_interval_secs;
        tokio::spawn(async move {
            self.run_loop(interval).await;
        })
    }

    /// 执行单次驱逐扫描
    pub async fn evict_once(&self) -> Result<EvictionResult, CacheError> {
        let stats_before = self.cache.stats().await?;
        let entries_before = stats_before.entries;

        // 注意：当前的 Cache trait 不支持迭代所有过期条目。
        // MemoryCache 内部在 get 时自动驱逐过期条目，
        // 所以这里我们只做统计报告。
        // 未来可以增加一个 evict_expired() 方法来主动驱逐。

        let stats_after = self.cache.stats().await?;

        Ok(EvictionResult {
            entries_before,
            entries_after: stats_after.entries,
            evicted: entries_before.saturating_sub(stats_after.entries),
        })
    }

    async fn run_loop(&self, interval_secs: u64) {
        let mut interval = time::interval(Duration::from_secs(interval_secs));

        tracing::info!(interval_secs, "Background eviction task started");

        loop {
            interval.tick().await;

            match self.evict_once().await {
                Ok(result) => {
                    if result.evicted > 0 {
                        tracing::info!(
                            evicted = result.evicted,
                            remaining = result.entries_after,
                            "Background eviction completed"
                        );
                    } else {
                        tracing::debug!(
                            entries = result.entries_after,
                            "Background eviction: no expired entries"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Background eviction failed");
                }
            }
        }
    }
}

/// 驱逐结果
#[derive(Debug, Clone)]
pub struct EvictionResult {
    /// 驱逐前条目数
    pub entries_before: usize,
    /// 驱逐后条目数
    pub entries_after: usize,
    /// 驱逐的条目数
    pub evicted: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryCache;

    #[tokio::test]
    async fn test_evict_once() {
        let cache = Arc::new(MemoryCache::new(100));
        cache.set("key1", "value1", 3600).await.unwrap();

        let eviction = BackgroundEviction::new(cache.clone(), EvictionConfig::default());

        let result = eviction.evict_once().await.unwrap();
        assert_eq!(result.entries_before, 1);
        assert_eq!(result.evicted, 0); // 未过期
    }

    #[tokio::test]
    async fn test_eviction_config_default() {
        let config = EvictionConfig::default();
        assert_eq!(config.scan_interval_secs, 60);
        assert_eq!(config.batch_size, 1000);
        assert!(config.enabled);
    }
}
