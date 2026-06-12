//! # 去中心化路由引擎 (Phase 9)
//!
//! 基于节点拓扑的最短路径路由、多路径冗余、降级策略。
//! 路由表维护和自动更新。

use crate::gossip::{GossipConfig, GossipMembership, GossipMessage, NodeId};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// 路由策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RouteStrategy {
    /// 最短路径（最低延迟）
    #[default]
    ShortestPath,
    /// 多路径冗余（同时走2-3条路径，取最快响应）
    MultiPath,
}

/// 传输降级策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradationStrategy {
    P2P,
    DirectTransfer,
    CloudRelay,
}

impl DegradationStrategy {
    /// 获取降级顺序
    pub fn fallback_chain() -> Vec<DegradationStrategy> {
        vec![
            DegradationStrategy::P2P,
            DegradationStrategy::DirectTransfer,
            DegradationStrategy::CloudRelay,
        ]
    }

    /// 下一个降级策略
    pub fn next_fallback(&self) -> Option<DegradationStrategy> {
        match self {
            DegradationStrategy::P2P => Some(DegradationStrategy::DirectTransfer),
            DegradationStrategy::DirectTransfer => Some(DegradationStrategy::CloudRelay),
            DegradationStrategy::CloudRelay => None,
        }
    }
}

/// 路由路径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutePath {
    /// 路径上的节点序列
    pub nodes: Vec<NodeId>,
    /// 总延迟 (ms)
    pub total_latency_ms: f64,
    /// 总带宽 (取路径中最小值, Mbps)
    pub min_bandwidth_mbps: f64,
    /// 路径可用
    pub available: bool,
    /// 使用的降级策略
    pub strategy: DegradationStrategy,
}

/// 路由结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResult {
    /// 主路径
    pub primary: RoutePath,
    /// 备选路径（多路径模式）
    pub alternatives: Vec<RoutePath>,
    /// 降级策略链
    pub fallback_chain: Vec<DegradationStrategy>,
}

/// 路由表条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTableEntry {
    /// 目标节点
    pub destination: NodeId,
    /// 下一跳节点
    pub next_hop: NodeId,
    /// 总延迟 (ms)
    pub total_latency_ms: f64,
    /// 跳数
    pub hop_count: u32,
    /// 最后更新时间
    pub updated_at: i64,
}

/// 路由表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTable {
    /// 源节点
    pub local_node_id: NodeId,
    /// 目标 → 路由表条目
    entries: HashMap<NodeId, RoutingTableEntry>,
    /// 最后完整更新时间
    last_updated: i64,
}

impl RoutingTable {
    /// 创建空路由表
    pub fn new(local_node_id: NodeId) -> Self {
        Self {
            local_node_id,
            entries: HashMap::new(),
            last_updated: chrono::Utc::now().timestamp(),
        }
    }

    /// 更新路由条目
    pub fn update_entry(&mut self, entry: RoutingTableEntry) {
        let now = chrono::Utc::now().timestamp();
        let should_update = match self.entries.get(&entry.destination) {
            Some(existing) => {
                entry.total_latency_ms < existing.total_latency_ms || entry.updated_at > existing.updated_at
            }
            None => true,
        };

        if should_update {
            self.entries.insert(
                entry.destination.clone(),
                RoutingTableEntry {
                    updated_at: now,
                    ..entry
                },
            );
        }
    }

    /// 查找路由
    pub fn lookup(&self, destination: &NodeId) -> Option<&RoutingTableEntry> {
        self.entries.get(destination)
    }

    /// 删除节点相关路由
    pub fn remove_node(&mut self, node_id: &NodeId) {
        self.entries
            .retain(|_, entry| entry.next_hop != *node_id && entry.destination != *node_id);
    }

    /// 获取所有条目
    pub fn entries(&self) -> &HashMap<NodeId, RoutingTableEntry> {
        &self.entries
    }

    /// 路由表大小
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 路由表是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Dijkstra 最短路径计算的堆元素
#[derive(Debug, Clone)]
struct DijkstraNode {
    node_id: NodeId,
    distance: f64,
}

impl PartialEq for DijkstraNode {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for DijkstraNode {}

impl Ord for DijkstraNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.distance.partial_cmp(&self.distance).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for DijkstraNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 去中心化路由引擎
pub struct DecentralizedRouter {
    membership: GossipMembership,
    routing_table: RoutingTable,
    strategy: RouteStrategy,
}

impl DecentralizedRouter {
    /// 创建路由引擎
    pub fn new(local_node_id: NodeId, gossip_config: GossipConfig, strategy: RouteStrategy) -> Self {
        let membership = GossipMembership::new(local_node_id.clone(), gossip_config);
        let routing_table = RoutingTable::new(local_node_id);
        Self {
            membership,
            routing_table,
            strategy,
        }
    }

    /// 获取成员管理器（只读）
    pub fn membership(&self) -> &GossipMembership {
        &self.membership
    }

    /// 获取成员管理器（可变）
    pub fn membership_mut(&mut self) -> &mut GossipMembership {
        &mut self.membership
    }

    /// 获取路由表（只读）
    pub fn routing_table(&self) -> &RoutingTable {
        &self.routing_table
    }

    /// 处理 Gossip 消息并更新路由表
    pub fn handle_gossip(&mut self, msg: &GossipMessage) {
        // 更新成员信息
        self.membership.handle_message(msg);

        // 重新计算路由表
        self.recompute_routes();
    }

    /// 基于 Dijkstra 重新计算路由表
    pub fn recompute_routes(&mut self) {
        let local_id = self.routing_table.local_node_id.clone();
        let links = self.membership.link_states();

        // 构建邻接表
        let mut adjacency: HashMap<NodeId, Vec<(NodeId, f64)>> = HashMap::new();

        for ((from, to), link) in links {
            if link.available {
                adjacency
                    .entry(from.clone())
                    .or_default()
                    .push((to.clone(), link.latency_ms));
                adjacency
                    .entry(to.clone())
                    .or_default()
                    .push((from.clone(), link.latency_ms));
            }
        }

        // Dijkstra
        let mut dist: HashMap<NodeId, f64> = HashMap::new();
        let mut prev: HashMap<NodeId, NodeId> = HashMap::new();
        let mut visited: HashSet<NodeId> = HashSet::new();
        let mut heap = BinaryHeap::new();

        dist.insert(local_id.clone(), 0.0);
        heap.push(DijkstraNode {
            node_id: local_id.clone(),
            distance: 0.0,
        });

        while let Some(DijkstraNode { node_id, distance }) = heap.pop() {
            if visited.contains(&node_id) {
                continue;
            }
            visited.insert(node_id.clone());

            if let Some(neighbors) = adjacency.get(&node_id) {
                for (neighbor, weight) in neighbors {
                    let new_dist = distance + weight;
                    let current_dist = dist.get(neighbor).copied().unwrap_or(f64::INFINITY);

                    if new_dist < current_dist {
                        dist.insert(neighbor.clone(), new_dist);
                        prev.insert(neighbor.clone(), node_id.clone());
                        heap.push(DijkstraNode {
                            node_id: neighbor.clone(),
                            distance: new_dist,
                        });
                    }
                }
            }
        }

        // 构建路由表条目
        let now = chrono::Utc::now().timestamp();
        for (dest, total_latency) in &dist {
            if dest == &local_id {
                continue;
            }

            // 回溯找到下一跳
            let next_hop = self.find_next_hop(&local_id, dest, &prev);

            if let Some(next_hop) = next_hop {
                let hop_count = self.count_hops(dest, &prev);

                self.routing_table.update_entry(RoutingTableEntry {
                    destination: dest.clone(),
                    next_hop,
                    total_latency_ms: *total_latency,
                    hop_count,
                    updated_at: now,
                });
            }
        }

        // 删除不可达节点
        let alive_node_ids: HashSet<NodeId> = self
            .membership
            .alive_nodes()
            .iter()
            .map(|n| n.node_id.clone())
            .collect();
        alive_node_ids.iter().for_each(|_| {});
        // Remove dead nodes from routing table
        let dead_nodes: Vec<NodeId> = self
            .routing_table
            .entries
            .keys()
            .filter(|k| !alive_node_ids.contains(*k))
            .cloned()
            .collect();
        for node_id in dead_nodes {
            self.routing_table.remove_node(&node_id);
        }
    }

    /// 回溯找到下一跳
    fn find_next_hop(&self, source: &NodeId, dest: &NodeId, prev: &HashMap<NodeId, NodeId>) -> Option<NodeId> {
        let mut current = dest.clone();
        let mut next_hop = None;

        while let Some(p) = prev.get(&current) {
            if p == source {
                next_hop = Some(current);
                break;
            }
            current = p.clone();
        }

        next_hop
    }

    /// 计算跳数
    fn count_hops(&self, dest: &NodeId, prev: &HashMap<NodeId, NodeId>) -> u32 {
        let mut count = 0u32;
        let mut current = dest.clone();
        while let Some(p) = prev.get(&current) {
            count += 1;
            current = p.clone();
        }
        count
    }

    /// 路由到目标节点
    pub fn route(&self, destination: &NodeId) -> RouteResult {
        match self.strategy {
            RouteStrategy::ShortestPath => self.route_shortest_path(destination),
            RouteStrategy::MultiPath => self.route_multi_path(destination),
        }
    }

    /// 最短路径路由
    fn route_shortest_path(&self, destination: &NodeId) -> RouteResult {
        let primary = self.build_path_to(destination);

        RouteResult {
            primary,
            alternatives: Vec::new(),
            fallback_chain: DegradationStrategy::fallback_chain(),
        }
    }

    /// 多路径路由
    fn route_multi_path(&self, destination: &NodeId) -> RouteResult {
        let primary = self.build_path_to(destination);

        // 尝试找到备选路径（通过不同的中间节点）
        let alternatives = self.find_alternative_paths(destination, 2);

        RouteResult {
            primary,
            alternatives,
            fallback_chain: DegradationStrategy::fallback_chain(),
        }
    }

    /// 构建到目标的路径
    fn build_path_to(&self, destination: &NodeId) -> RoutePath {
        if let Some(entry) = self.routing_table.lookup(destination) {
            // 构建路径节点列表
            let nodes = vec![
                self.routing_table.local_node_id.clone(),
                entry.next_hop.clone(),
                destination.clone(),
            ];

            // 获取带宽信息
            let links = self.membership.link_states();
            let bw = links
                .get(&(self.routing_table.local_node_id.clone(), entry.next_hop.clone()))
                .and_then(|l| l.bandwidth_mbps)
                .unwrap_or(0.0);

            RoutePath {
                nodes,
                total_latency_ms: entry.total_latency_ms,
                min_bandwidth_mbps: bw,
                available: true,
                strategy: DegradationStrategy::P2P,
            }
        } else {
            // 无可达路径，降级为直连
            RoutePath {
                nodes: vec![self.routing_table.local_node_id.clone(), destination.clone()],
                total_latency_ms: f64::INFINITY,
                min_bandwidth_mbps: 0.0,
                available: false,
                strategy: DegradationStrategy::DirectTransfer,
            }
        }
    }

    /// 查找备选路径
    fn find_alternative_paths(&self, destination: &NodeId, max_paths: usize) -> Vec<RoutePath> {
        let mut alternatives = Vec::new();

        // 简化实现：遍历所有中间节点作为备选路径
        let alive_nodes = self.membership.alive_nodes();
        for node in alive_nodes {
            if node.node_id == self.routing_table.local_node_id || node.node_id == *destination {
                continue;
            }

            // 检查通过中间节点的路径
            let link1 = self
                .membership
                .get_link(&self.routing_table.local_node_id, &node.node_id);
            let link2 = self.membership.get_link(&node.node_id, destination);

            if let (Some(l1), Some(l2)) = (link1, link2) {
                if l1.available && l2.available {
                    let total_latency = l1.latency_ms + l2.latency_ms;
                    let bw1 = l1.bandwidth_mbps.unwrap_or(0.0);
                    let bw2 = l2.bandwidth_mbps.unwrap_or(0.0);
                    let min_bw = bw1.min(bw2);

                    alternatives.push(RoutePath {
                        nodes: vec![
                            self.routing_table.local_node_id.clone(),
                            node.node_id.clone(),
                            destination.clone(),
                        ],
                        total_latency_ms: total_latency,
                        min_bandwidth_mbps: min_bw,
                        available: true,
                        strategy: DegradationStrategy::P2P,
                    });

                    if alternatives.len() >= max_paths {
                        break;
                    }
                }
            }
        }

        // 按延迟排序
        alternatives.sort_by(|a, b| {
            a.total_latency_ms
                .partial_cmp(&b.total_latency_ms)
                .unwrap_or(Ordering::Equal)
        });
        alternatives
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_table_basic() {
        let mut table = RoutingTable::new("node-1".to_string());
        assert!(table.is_empty());

        table.update_entry(RoutingTableEntry {
            destination: "node-3".to_string(),
            next_hop: "node-2".to_string(),
            total_latency_ms: 25.0,
            hop_count: 2,
            updated_at: 0,
        });

        assert_eq!(table.len(), 1);
        let entry = table.lookup(&"node-3".to_string()).unwrap();
        assert_eq!(entry.next_hop, "node-2");
    }

    #[test]
    fn test_routing_table_better_path() {
        let mut table = RoutingTable::new("node-1".to_string());

        table.update_entry(RoutingTableEntry {
            destination: "node-3".to_string(),
            next_hop: "node-2".to_string(),
            total_latency_ms: 25.0,
            hop_count: 2,
            updated_at: 1,
        });

        // Better path through node-4
        table.update_entry(RoutingTableEntry {
            destination: "node-3".to_string(),
            next_hop: "node-4".to_string(),
            total_latency_ms: 15.0,
            hop_count: 1,
            updated_at: 2,
        });

        let entry = table.lookup(&"node-3".to_string()).unwrap();
        assert_eq!(entry.next_hop, "node-4"); // Better path replaces old
    }

    #[test]
    fn test_degradation_chain() {
        let chain = DegradationStrategy::fallback_chain();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0], DegradationStrategy::P2P);

        assert_eq!(
            DegradationStrategy::P2P.next_fallback(),
            Some(DegradationStrategy::DirectTransfer)
        );
        assert_eq!(DegradationStrategy::CloudRelay.next_fallback(), None);
    }

    #[test]
    fn test_router_creation() {
        let router = DecentralizedRouter::new(
            "node-1".to_string(),
            GossipConfig::default(),
            RouteStrategy::ShortestPath,
        );
        assert!(router.routing_table().is_empty());
    }

    #[test]
    fn test_router_with_links() {
        let mut router = DecentralizedRouter::new(
            "node-1".to_string(),
            GossipConfig::default(),
            RouteStrategy::ShortestPath,
        );

        // Join nodes
        let join2 = GossipMessage::Join {
            node_id: "node-2".to_string(),
            addr: "10.0.0.2:8080".parse().unwrap(),
            capabilities: vec![],
            timestamp: 0,
        };
        let join3 = GossipMessage::Join {
            node_id: "node-3".to_string(),
            addr: "10.0.0.3:8080".parse().unwrap(),
            capabilities: vec![],
            timestamp: 0,
        };
        router.membership_mut().handle_message(&join2);
        router.membership_mut().handle_message(&join3);

        // Add links
        let link12 = GossipMessage::LinkState {
            from: "node-1".to_string(),
            to: "node-2".to_string(),
            latency_ms: 10.0,
            available: true,
            bandwidth_mbps: Some(100.0),
            timestamp: 0,
        };
        let link23 = GossipMessage::LinkState {
            from: "node-2".to_string(),
            to: "node-3".to_string(),
            latency_ms: 15.0,
            available: true,
            bandwidth_mbps: Some(50.0),
            timestamp: 0,
        };
        router.membership_mut().handle_message(&link12);
        router.membership_mut().handle_message(&link23);

        router.recompute_routes();

        // Should have a route to node-2 (1 hop) and node-3 (2 hops)
        let entry2 = router.routing_table().lookup(&"node-2".to_string());
        assert!(entry2.is_some());
        assert_eq!(entry2.unwrap().next_hop, "node-2");
        assert!((entry2.unwrap().total_latency_ms - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_multi_path_routing() {
        let mut router =
            DecentralizedRouter::new("node-1".to_string(), GossipConfig::default(), RouteStrategy::MultiPath);

        let join2 = GossipMessage::Join {
            node_id: "node-2".to_string(),
            addr: "10.0.0.2:8080".parse().unwrap(),
            capabilities: vec![],
            timestamp: 0,
        };
        let join3 = GossipMessage::Join {
            node_id: "node-3".to_string(),
            addr: "10.0.0.3:8080".parse().unwrap(),
            capabilities: vec![],
            timestamp: 0,
        };
        router.membership_mut().handle_message(&join2);
        router.membership_mut().handle_message(&join3);

        let link12 = GossipMessage::LinkState {
            from: "node-1".to_string(),
            to: "node-2".to_string(),
            latency_ms: 10.0,
            available: true,
            bandwidth_mbps: Some(100.0),
            timestamp: 0,
        };
        let link13 = GossipMessage::LinkState {
            from: "node-1".to_string(),
            to: "node-3".to_string(),
            latency_ms: 5.0,
            available: true,
            bandwidth_mbps: Some(200.0),
            timestamp: 0,
        };
        let link23 = GossipMessage::LinkState {
            from: "node-2".to_string(),
            to: "node-3".to_string(),
            latency_ms: 15.0,
            available: true,
            bandwidth_mbps: Some(50.0),
            timestamp: 0,
        };
        router.membership_mut().handle_message(&link12);
        router.membership_mut().handle_message(&link13);
        router.membership_mut().handle_message(&link23);

        router.recompute_routes();

        let result = router.route(&"node-3".to_string());
        assert!(result.primary.available);
    }

    #[test]
    fn test_remove_node_from_routing_table() {
        let mut table = RoutingTable::new("node-1".to_string());

        table.update_entry(RoutingTableEntry {
            destination: "node-3".to_string(),
            next_hop: "node-2".to_string(),
            total_latency_ms: 25.0,
            hop_count: 2,
            updated_at: 0,
        });

        table.remove_node(&"node-2".to_string());
        assert!(table.lookup(&"node-3".to_string()).is_none());
    }
}
