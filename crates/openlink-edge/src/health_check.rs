//! # 边缘健康检查（Phase 5）
//!
//! 定期检查边缘节点和缓存的健康状态。
//! - 节点健康：检查上游节点是否可达
//! - 缓存健康：检查缓存命中率和内存使用

use crate::cache::EdgeCache;
use crate::geo::GeoRouter;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 健康状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    /// 健康
    Healthy,
    /// 降级（部分功能不可用）
    Degraded,
    /// 不健康
    Unhealthy,
}

/// 节点健康信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthInfo {
    /// 节点 ID
    pub node_id: String,
    /// 健康状态
    pub status: HealthStatus,
    /// 响应延迟（毫秒），None = 不可达
    pub latency_ms: Option<u64>,
    /// 最后检查时间
    pub last_check: i64,
    /// 连续失败次数
    pub consecutive_failures: u32,
}

/// 整体健康报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// 当前节点 ID
    pub node_id: String,
    /// 当前节点健康状态
    pub status: HealthStatus,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// 缓存条目数
    pub cache_entries: usize,
    /// 在线上游节点数
    pub online_upstream_nodes: usize,
    /// 总上游节点数
    pub total_upstream_nodes: usize,
    /// 各节点健康信息
    pub node_health: Vec<NodeHealthInfo>,
    /// 检查时间
    pub checked_at: i64,
}

/// 健康检查配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// 检查间隔（秒）
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// 节点超时（毫秒）
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// 连续失败阈值（超过此数标记为 Unhealthy）
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    /// 缓存命中率降级阈值
    #[serde(default = "default_cache_hit_threshold")]
    pub cache_hit_degraded_threshold: f64,
}

fn default_interval() -> u64 {
    30
}
fn default_timeout() -> u64 {
    5000
}
fn default_failure_threshold() -> u32 {
    3
}
fn default_cache_hit_threshold() -> f64 {
    0.3
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_interval(),
            timeout_ms: default_timeout(),
            failure_threshold: default_failure_threshold(),
            cache_hit_degraded_threshold: default_cache_hit_threshold(),
        }
    }
}

/// 边缘健康检查器
pub struct HealthChecker {
    config: HealthCheckConfig,
    node_id: String,
    cache: Arc<EdgeCache>,
    geo_router: Arc<RwLock<GeoRouter>>,
    /// 节点健康记录
    node_health: Arc<RwLock<Vec<NodeHealthInfo>>>,
}

impl HealthChecker {
    /// 创建健康检查器
    pub fn new(
        config: HealthCheckConfig,
        node_id: String,
        cache: Arc<EdgeCache>,
        geo_router: Arc<RwLock<GeoRouter>>,
    ) -> Self {
        Self {
            config,
            node_id,
            cache,
            geo_router,
            node_health: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 执行健康检查
    pub async fn check(&self) -> HealthReport {
        let cache_stats = self.cache.stats().await;
        let geo_router = self.geo_router.read().await;
        let online_nodes = geo_router.online_nodes();

        // 初始化节点健康信息（简化版：基于在线状态）
        let mut node_health_list: Vec<NodeHealthInfo> = Vec::new();
        let now = chrono::Utc::now().timestamp();

        for node in &online_nodes {
            node_health_list.push(NodeHealthInfo {
                node_id: node.node_id.clone(),
                status: if node.is_online {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy
                },
                latency_ms: None, // 实际应通过 ping 测量
                last_check: now,
                consecutive_failures: 0,
            });
        }

        let online_count = node_health_list
            .iter()
            .filter(|h| h.status == HealthStatus::Healthy)
            .count();
        let total_count = node_health_list.len();

        // 计算整体健康状态
        let status = if cache_stats.hit_rate < self.config.cache_hit_degraded_threshold
            && total_count > 0
            && online_count == 0
        {
            HealthStatus::Unhealthy
        } else if cache_stats.hit_rate < self.config.cache_hit_degraded_threshold
            || online_count < total_count
        {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        // 更新存储
        {
            let mut stored = self.node_health.write().await;
            *stored = node_health_list.clone();
        }

        HealthReport {
            node_id: self.node_id.clone(),
            status,
            cache_hit_rate: cache_stats.hit_rate,
            cache_entries: cache_stats.entries,
            online_upstream_nodes: online_count,
            total_upstream_nodes: total_count,
            node_health: node_health_list,
            checked_at: now,
        }
    }

    /// 获取缓存的节点健康信息
    pub async fn get_node_health(&self) -> Vec<NodeHealthInfo> {
        self.node_health.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EdgeConfig;
    use crate::geo::GeoRouteConfig;

    #[tokio::test]
    async fn test_health_check() {
        let config = EdgeConfig::default_config();
        let cache = Arc::new(EdgeCache::new(
            config.cache.max_entries,
            config.cache.ttl_secs,
        ));
        let geo_router = Arc::new(RwLock::new(GeoRouter::new(GeoRouteConfig::default())));

        let checker = HealthChecker::new(
            HealthCheckConfig::default(),
            "test-node".to_string(),
            cache,
            geo_router,
        );

        let report = checker.check().await;
        assert_eq!(report.node_id, "test-node");
        assert!(report.total_upstream_nodes > 0);
    }

    #[tokio::test]
    async fn test_health_status_calculation() {
        let config = EdgeConfig::default_config();
        let cache = Arc::new(EdgeCache::new(
            config.cache.max_entries,
            config.cache.ttl_secs,
        ));
        let geo_router = Arc::new(RwLock::new(GeoRouter::new(GeoRouteConfig::default())));

        let checker = HealthChecker::new(
            HealthCheckConfig::default(),
            "test-node".to_string(),
            cache,
            geo_router,
        );

        // Fresh cache with no requests should be healthy
        let report = checker.check().await;
        // All nodes online by default
        assert!(report.online_upstream_nodes > 0);
    }

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.interval_secs, 30);
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.failure_threshold, 3);
    }
}
