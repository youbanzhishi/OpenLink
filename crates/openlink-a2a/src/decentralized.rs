//! # 去中心化路由增强 (Phase 10)
//!
//! 基于 Gossip 协议的能力发现、DHT 路由表、能力路由和分区容错。
//! 与 Phase 9 的 openlink-core gossip/decentralized 模块集成。

use openlink_core::decentralized::DegradationStrategy;
use openlink_core::gossip::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// DHT 键（用于分布式哈希表查找）
pub type DhtKey = String;

/// DHT 值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtValue {
    /// 值内容
    pub data: serde_json::Value,
    /// 值的版本号
    pub version: u64,
    /// 存储节点
    pub stored_at: NodeId,
    /// 存储时间
    pub timestamp: i64,
}

/// 能力路由条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRoute {
    /// 能力 ID
    pub capability_id: String,
    /// 能力名称
    pub capability_name: String,
    /// 提供此能力的节点列表
    pub providers: Vec<CapabilityProvider>,
    /// 最后更新时间
    pub updated_at: i64,
}

/// 能力提供者
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProvider {
    /// 节点 ID
    pub node_id: NodeId,
    /// 节点地址
    pub endpoint: String,
    /// 能力置信度 (0.0-1.0)
    pub confidence: f64,
    /// 平均响应延迟 (ms)
    pub avg_latency_ms: f64,
    /// 负载因子 (0.0-1.0，1.0 = 满载)
    pub load_factor: f64,
}

/// Gossip 能力传播消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGossip {
    /// 传播节点
    pub source_node: NodeId,
    /// 能力信息
    pub capabilities: Vec<CapabilityAnnouncement>,
    /// 传播序列号
    pub seq: u64,
    /// 时间戳
    pub timestamp: i64,
}

/// 能力公告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAnnouncement {
    /// 能力 ID
    pub capability_id: String,
    /// 能力名称
    pub capability_name: String,
    /// 能力描述
    pub description: String,
    /// 提供节点
    pub provider_node: NodeId,
    /// 端点
    pub endpoint: String,
    /// 置信度
    pub confidence: f64,
}

/// 网络分区状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionStatus {
    /// 正常（无分区）
    Normal,
    /// 轻度分区（少量节点不可达）
    Minor,
    /// 严重分区（大部分节点不可达）
    Major,
    /// 完全隔离
    Isolated,
}

/// 分区容错策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionPolicy {
    /// 分区阈值：不可达节点比例超过此值视为分区
    #[serde(default = "default_partition_threshold")]
    pub partition_threshold: f64,
    /// 降级策略
    #[serde(default = "default_degradation")]
    pub degradation: DegradationStrategy,
    /// 本地缓存 TTL（秒），分区时使用缓存
    #[serde(default = "default_cache_ttl")]
    pub local_cache_ttl_secs: u64,
}

fn default_partition_threshold() -> f64 {
    0.3
}
fn default_cache_ttl() -> u64 {
    300
}
fn default_degradation() -> DegradationStrategy {
    DegradationStrategy::CloudRelay
}

impl Default for PartitionPolicy {
    fn default() -> Self {
        Self {
            partition_threshold: default_partition_threshold(),
            degradation: default_degradation(),
            local_cache_ttl_secs: default_cache_ttl(),
        }
    }
}

/// 去中心化能力路由器
pub struct DecentralizedCapabilityRouter {
    /// DHT 路由表
    dht: Arc<RwLock<HashMap<DhtKey, DhtValue>>>,
    /// 能力路由表
    capability_routes: Arc<RwLock<HashMap<String, CapabilityRoute>>>,
    /// Gossip 传播序列号
    gossip_seq: Arc<RwLock<u64>>,
    /// 分区状态
    partition_status: Arc<RwLock<PartitionStatus>>,
    /// 分区策略
    partition_policy: PartitionPolicy,
    /// 本地节点 ID
    local_node_id: NodeId,
    /// 已知节点数（用于分区检测）
    known_nodes_count: Arc<RwLock<usize>>,
    /// 可达节点数
    reachable_nodes_count: Arc<RwLock<usize>>,
}

impl DecentralizedCapabilityRouter {
    /// 创建去中心化能力路由器
    pub fn new(local_node_id: NodeId, partition_policy: PartitionPolicy) -> Self {
        Self {
            dht: Arc::new(RwLock::new(HashMap::new())),
            capability_routes: Arc::new(RwLock::new(HashMap::new())),
            gossip_seq: Arc::new(RwLock::new(0)),
            partition_status: Arc::new(RwLock::new(PartitionStatus::Normal)),
            partition_policy,
            local_node_id,
            known_nodes_count: Arc::new(RwLock::new(1)),
            reachable_nodes_count: Arc::new(RwLock::new(1)),
        }
    }

    // ─── DHT 操作 ───────────────────────────────────────────

    /// DHT PUT：存储键值对
    pub async fn dht_put(&self, key: DhtKey, value: serde_json::Value) -> DhtValue {
        let mut dht = self.dht.write().await;
        let now = chrono::Utc::now().timestamp();
        let version = dht.get(&key).map(|v| v.version + 1).unwrap_or(1);

        let dht_value = DhtValue {
            data: value,
            version,
            stored_at: self.local_node_id.clone(),
            timestamp: now,
        };

        tracing::debug!(key = %key, version = version, "DHT PUT");
        dht.insert(key, dht_value.clone());
        dht_value
    }

    /// DHT GET：查找键对应的值
    pub async fn dht_get(&self, key: &str) -> Option<DhtValue> {
        let dht = self.dht.read().await;
        dht.get(key).cloned()
    }

    /// DHT DELETE：删除键
    pub async fn dht_remove(&self, key: &str) -> Option<DhtValue> {
        let mut dht = self.dht.write().await;
        dht.remove(key)
    }

    // ─── 能力路由 ───────────────────────────────────────────

    /// 注册能力提供者
    pub async fn register_capability(&self, provider: CapabilityProvider, cap_id: &str, cap_name: &str) {
        let mut routes = self.capability_routes.write().await;
        let now = chrono::Utc::now().timestamp();

        let route = routes.entry(cap_id.to_string()).or_insert_with(|| CapabilityRoute {
            capability_id: cap_id.to_string(),
            capability_name: cap_name.to_string(),
            providers: vec![],
            updated_at: now,
        });

        // 检查是否已存在
        if let Some(existing) = route.providers.iter_mut().find(|p| p.node_id == provider.node_id) {
            *existing = provider;
        } else {
            route.providers.push(provider);
        }

        route.updated_at = now;
        tracing::info!(capability = %cap_id, providers = route.providers.len(), "Capability route updated");
    }

    /// 查找最佳能力提供者
    ///
    /// 综合考虑延迟、负载和置信度。
    pub async fn find_capability_provider(&self, capability_id: &str) -> Option<CapabilityProvider> {
        let routes = self.capability_routes.read().await;
        let route = routes.get(capability_id)?;

        if route.providers.is_empty() {
            return None;
        }

        // 按综合评分排序：低延迟 * 高置信度 * 低负载
        let mut sorted: Vec<&CapabilityProvider> = route.providers.iter().collect();
        sorted.sort_by(|a, b| {
            let score_a = a.confidence * (1.0 - a.load_factor) / (a.avg_latency_ms.max(1.0));
            let score_b = b.confidence * (1.0 - b.load_factor) / (b.avg_latency_ms.max(1.0));
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        sorted.first().cloned().cloned()
    }

    /// 根据能力智能路由请求
    pub async fn route_by_capability(&self, capability_id: &str) -> Option<CapabilityProvider> {
        let provider = self.find_capability_provider(capability_id).await;

        if provider.is_some() {
            return provider;
        }

        // 本地没有找到，尝试 DHT 查找
        let dht_value = self.dht_get(&format!("cap:{}", capability_id)).await;
        if let Some(value) = dht_value {
            if let Some(endpoint) = value.data.get("endpoint").and_then(|v| v.as_str()) {
                return Some(CapabilityProvider {
                    node_id: value.stored_at.clone(),
                    endpoint: endpoint.to_string(),
                    confidence: value.data.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    avg_latency_ms: value.data.get("latency_ms").and_then(|v| v.as_f64()).unwrap_or(100.0),
                    load_factor: value.data.get("load").and_then(|v| v.as_f64()).unwrap_or(0.5),
                });
            }
        }

        None
    }

    // ─── Gossip 能力传播 ──────────────────────────────────────

    /// 创建能力 Gossip 消息
    pub async fn create_capability_gossip(&self, capabilities: Vec<CapabilityAnnouncement>) -> CapabilityGossip {
        let mut seq = self.gossip_seq.write().await;
        *seq += 1;

        CapabilityGossip {
            source_node: self.local_node_id.clone(),
            capabilities,
            seq: *seq,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 处理收到的能力 Gossip 消息
    pub async fn handle_capability_gossip(&self, gossip: &CapabilityGossip) -> usize {
        let mut updated = 0;
        let mut routes = self.capability_routes.write().await;
        let now = chrono::Utc::now().timestamp();

        for announcement in &gossip.capabilities {
            let route = routes
                .entry(announcement.capability_id.clone())
                .or_insert_with(|| CapabilityRoute {
                    capability_id: announcement.capability_id.clone(),
                    capability_name: announcement.capability_name.clone(),
                    providers: vec![],
                    updated_at: now,
                });

            let provider = CapabilityProvider {
                node_id: announcement.provider_node.clone(),
                endpoint: announcement.endpoint.clone(),
                confidence: announcement.confidence,
                avg_latency_ms: 100.0, // 默认值，后续通过心跳更新
                load_factor: 0.0,
            };

            if let Some(existing) = route.providers.iter_mut().find(|p| p.node_id == provider.node_id) {
                *existing = provider;
            } else {
                route.providers.push(provider);
            }

            route.updated_at = now;
            updated += 1;
        }

        tracing::debug!(
            source = %gossip.source_node,
            capabilities = updated,
            seq = gossip.seq,
            "Capability gossip processed"
        );

        updated
    }

    // ─── 分区容错 ───────────────────────────────────────────

    /// 更新网络拓扑信息
    pub async fn update_network_state(&self, known: usize, reachable: usize) {
        let mut known_count = self.known_nodes_count.write().await;
        let mut reachable_count = self.reachable_nodes_count.write().await;
        *known_count = known;
        *reachable_count = reachable;

        // 检测分区
        let unreachable_ratio = if known > 0 {
            1.0 - (reachable as f64 / known as f64)
        } else {
            0.0
        };

        let mut status = self.partition_status.write().await;
        *status = if reachable == 0 {
            PartitionStatus::Isolated
        } else if unreachable_ratio > 0.7 {
            PartitionStatus::Major
        } else if unreachable_ratio > self.partition_policy.partition_threshold {
            PartitionStatus::Minor
        } else {
            PartitionStatus::Normal
        };

        tracing::info!(
            known = known,
            reachable = reachable,
            unreachable_ratio = format!("{:.2}", unreachable_ratio),
            partition_status = ?*status,
            "Network state updated"
        );
    }

    /// 获取分区状态
    pub async fn partition_status(&self) -> PartitionStatus {
        self.partition_status.read().await.clone()
    }

    /// 获取降级策略（根据分区状态）
    pub async fn degradation_strategy(&self) -> DegradationStrategy {
        let status = self.partition_status.read().await;
        match *status {
            PartitionStatus::Normal => DegradationStrategy::P2P,
            PartitionStatus::Minor => DegradationStrategy::DirectTransfer,
            PartitionStatus::Major | PartitionStatus::Isolated => self.partition_policy.degradation,
        }
    }

    /// 列出所有已知能力路由
    pub async fn list_capability_routes(&self) -> Vec<CapabilityRoute> {
        let routes = self.capability_routes.read().await;
        routes.values().cloned().collect()
    }

    /// 获取本地节点 ID
    pub fn local_node_id(&self) -> &NodeId {
        &self.local_node_id
    }
}

// ─── 单元测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dht_put_and_get() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        router.dht_put("key1".to_string(), serde_json::json!("value1")).await;
        let value = router.dht_get("key1").await.unwrap();
        assert_eq!(value.data, serde_json::json!("value1"));
        assert_eq!(value.version, 1);
    }

    #[tokio::test]
    async fn test_dht_version_increment() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        router.dht_put("key1".to_string(), serde_json::json!("v1")).await;
        router.dht_put("key1".to_string(), serde_json::json!("v2")).await;

        let value = router.dht_get("key1").await.unwrap();
        assert_eq!(value.version, 2);
        assert_eq!(value.data, serde_json::json!("v2"));
    }

    #[tokio::test]
    async fn test_dht_remove() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        router.dht_put("key1".to_string(), serde_json::json!("value1")).await;
        let removed = router.dht_remove("key1").await;
        assert!(removed.is_some());
        assert!(router.dht_get("key1").await.is_none());
    }

    #[tokio::test]
    async fn test_register_and_find_capability() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        let provider = CapabilityProvider {
            node_id: "node-2".to_string(),
            endpoint: "https://node-2.example.com".to_string(),
            confidence: 0.9,
            avg_latency_ms: 50.0,
            load_factor: 0.3,
        };

        router
            .register_capability(provider, "text-gen", "Text Generation")
            .await;

        let found = router.find_capability_provider("text-gen").await.unwrap();
        assert_eq!(found.node_id, "node-2");
    }

    #[tokio::test]
    async fn test_find_best_capability_provider() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        // Provider 1: high latency, high load
        let p1 = CapabilityProvider {
            node_id: "node-2".to_string(),
            endpoint: "https://node-2.example.com".to_string(),
            confidence: 0.8,
            avg_latency_ms: 200.0,
            load_factor: 0.9,
        };

        // Provider 2: low latency, low load
        let p2 = CapabilityProvider {
            node_id: "node-3".to_string(),
            endpoint: "https://node-3.example.com".to_string(),
            confidence: 0.9,
            avg_latency_ms: 30.0,
            load_factor: 0.1,
        };

        router.register_capability(p1, "text-gen", "Text Generation").await;
        router.register_capability(p2, "text-gen", "Text Generation").await;

        let found = router.find_capability_provider("text-gen").await.unwrap();
        assert_eq!(found.node_id, "node-3"); // Better overall score
    }

    #[tokio::test]
    async fn test_capability_gossip() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        let gossip = router
            .create_capability_gossip(vec![CapabilityAnnouncement {
                capability_id: "text-gen".to_string(),
                capability_name: "Text Generation".to_string(),
                description: "Generate text".to_string(),
                provider_node: "node-1".to_string(),
                endpoint: "https://node-1.example.com".to_string(),
                confidence: 0.9,
            }])
            .await;

        assert_eq!(gossip.source_node, "node-1");
        assert_eq!(gossip.capabilities.len(), 1);
        assert_eq!(gossip.seq, 1);
    }

    #[tokio::test]
    async fn test_handle_capability_gossip() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        let gossip = CapabilityGossip {
            source_node: "node-2".to_string(),
            capabilities: vec![CapabilityAnnouncement {
                capability_id: "image-analysis".to_string(),
                capability_name: "Image Analysis".to_string(),
                description: "Analyze images".to_string(),
                provider_node: "node-2".to_string(),
                endpoint: "https://node-2.example.com".to_string(),
                confidence: 0.85,
            }],
            seq: 1,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let updated = router.handle_capability_gossip(&gossip).await;
        assert_eq!(updated, 1);

        let found = router.find_capability_provider("image-analysis").await.unwrap();
        assert_eq!(found.node_id, "node-2");
    }

    #[tokio::test]
    async fn test_partition_detection() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        // Normal: 10 known, 9 reachable
        router.update_network_state(10, 9).await;
        assert_eq!(router.partition_status().await, PartitionStatus::Normal);

        // Minor partition: 10 known, 5 reachable
        router.update_network_state(10, 5).await;
        assert_eq!(router.partition_status().await, PartitionStatus::Minor);

        // Major partition: 10 known, 2 reachable
        router.update_network_state(10, 2).await;
        assert_eq!(router.partition_status().await, PartitionStatus::Major);

        // Isolated: 0 reachable
        router.update_network_state(10, 0).await;
        assert_eq!(router.partition_status().await, PartitionStatus::Isolated);
    }

    #[tokio::test]
    async fn test_degradation_strategy_by_partition() {
        let router = DecentralizedCapabilityRouter::new("node-1".to_string(), PartitionPolicy::default());

        router.update_network_state(10, 9).await;
        assert_eq!(router.degradation_strategy().await, DegradationStrategy::P2P);

        router.update_network_state(10, 2).await;
        // Major partition -> use policy degradation (CloudRelay by default)
        assert_eq!(router.degradation_strategy().await, DegradationStrategy::CloudRelay);
    }
}
