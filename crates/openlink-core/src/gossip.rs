//! # Gossip 协议 (Phase 9)
//!
//! 节点发现、链路状态、成员管理、故障检测。
//! Gossip 消息类型：Join/Leave/Heartbeat/LinkState/FullSync

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// 节点 ID
pub type NodeId = String;

/// Gossip 消息类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GossipMessage {
    /// 节点加入网络
    Join {
        node_id: NodeId,
        addr: SocketAddr,
        capabilities: Vec<String>,
        timestamp: i64,
    },
    /// 节点离开网络
    Leave { node_id: NodeId, timestamp: i64 },
    /// 心跳
    Heartbeat { node_id: NodeId, seq: u64, timestamp: i64 },
    /// 链路状态更新
    LinkState {
        from: NodeId,
        to: NodeId,
        latency_ms: f64,
        available: bool,
        bandwidth_mbps: Option<f64>,
        timestamp: i64,
    },
    /// 全量同步（新节点加入时使用）
    FullSync {
        nodes: Vec<NodeInfo>,
        links: Vec<LinkStateEntry>,
        timestamp: i64,
    },
}

/// 节点信息
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub addr: SocketAddr,
    pub capabilities: Vec<String>,
    pub joined_at: i64,
    pub last_heartbeat: i64,
    pub status: NodeStatus,
}

/// 节点状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Alive,
    Suspect,
    Dead,
}

/// 链路状态条目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkStateEntry {
    pub from: NodeId,
    pub to: NodeId,
    pub latency_ms: f64,
    pub available: bool,
    pub bandwidth_mbps: Option<f64>,
    pub updated_at: i64,
}

/// Gossip 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    /// 心跳间隔（秒）
    pub heartbeat_interval_secs: u64,
    /// 故障检测超时（秒），超过此时间未收到心跳则标记为 Suspect
    pub suspect_timeout_secs: u64,
    /// 死亡超时（秒），Suspect 状态持续此时间后标记为 Dead
    pub dead_timeout_secs: u64,
    /// Gossip 扇出（每次传播给多少节点）
    pub fanout: usize,
    /// 协议周期（秒）
    pub protocol_period_secs: u64,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 5,
            suspect_timeout_secs: 15,
            dead_timeout_secs: 30,
            fanout: 3,
            protocol_period_secs: 1,
        }
    }
}

/// Gossip 成员管理器
pub struct GossipMembership {
    config: GossipConfig,
    local_node_id: NodeId,
    /// 已知节点
    nodes: HashMap<NodeId, NodeInfo>,
    /// 链路状态
    links: HashMap<(NodeId, NodeId), LinkStateEntry>,
    /// 心跳序号
    heartbeat_seq: u64,
}

impl GossipMembership {
    /// 创建成员管理器
    pub fn new(local_node_id: NodeId, config: GossipConfig) -> Self {
        Self {
            config,
            local_node_id,
            nodes: HashMap::new(),
            links: HashMap::new(),
            heartbeat_seq: 0,
        }
    }

    /// 获取本节点 ID
    pub fn local_node_id(&self) -> &NodeId {
        &self.local_node_id
    }

    /// 处理 Gossip 消息
    pub fn handle_message(&mut self, msg: &GossipMessage) -> Vec<GossipMessage> {
        match msg {
            GossipMessage::Join {
                node_id,
                addr,
                capabilities,
                timestamp,
            } => self.handle_join(node_id, *addr, capabilities.clone(), *timestamp),
            GossipMessage::Leave { node_id, timestamp } => self.handle_leave(node_id, *timestamp),
            GossipMessage::Heartbeat {
                node_id,
                seq,
                timestamp,
            } => self.handle_heartbeat(node_id, *seq, *timestamp),
            GossipMessage::LinkState {
                from,
                to,
                latency_ms,
                available,
                bandwidth_mbps,
                timestamp,
            } => self.handle_link_state(from, to, *latency_ms, *available, *bandwidth_mbps, *timestamp),
            GossipMessage::FullSync {
                nodes,
                links,
                timestamp: _,
            } => self.handle_full_sync(nodes.clone(), links.clone()),
        }
    }

    /// 处理节点加入
    fn handle_join(
        &mut self,
        node_id: &NodeId,
        addr: SocketAddr,
        capabilities: Vec<String>,
        timestamp: i64,
    ) -> Vec<GossipMessage> {
        let now = chrono::Utc::now().timestamp();

        if let Some(existing) = self.nodes.get(node_id) {
            // 已存在，更新心跳时间
            if timestamp > existing.last_heartbeat {
                let updated = NodeInfo {
                    last_heartbeat: now,
                    status: NodeStatus::Alive,
                    ..existing.clone()
                };
                self.nodes.insert(node_id.clone(), updated);
            }
        } else {
            // 新节点
            let node_info = NodeInfo {
                node_id: node_id.clone(),
                addr,
                capabilities,
                joined_at: timestamp,
                last_heartbeat: now,
                status: NodeStatus::Alive,
            };
            self.nodes.insert(node_id.clone(), node_info);
            tracing::info!(node_id = %node_id, "Node joined via gossip");
        }

        // 传播 Join 消息给其他节点
        vec![GossipMessage::Join {
            node_id: node_id.clone(),
            addr,
            capabilities: self
                .nodes
                .get(node_id)
                .map(|n| n.capabilities.clone())
                .unwrap_or_default(),
            timestamp,
        }]
    }

    /// 处理节点离开
    fn handle_leave(&mut self, node_id: &NodeId, _timestamp: i64) -> Vec<GossipMessage> {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.status = NodeStatus::Dead;
            tracing::info!(node_id = %node_id, "Node left via gossip");
        }

        vec![GossipMessage::Leave {
            node_id: node_id.clone(),
            timestamp: chrono::Utc::now().timestamp(),
        }]
    }

    /// 处理心跳
    fn handle_heartbeat(&mut self, node_id: &NodeId, _seq: u64, _timestamp: i64) -> Vec<GossipMessage> {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.last_heartbeat = chrono::Utc::now().timestamp();
            node.status = NodeStatus::Alive;
        }
        // 心跳不需要进一步传播（除非是间接探测）
        Vec::new()
    }

    /// 处理链路状态
    fn handle_link_state(
        &mut self,
        from: &NodeId,
        to: &NodeId,
        latency_ms: f64,
        available: bool,
        bandwidth_mbps: Option<f64>,
        timestamp: i64,
    ) -> Vec<GossipMessage> {
        let key = (from.clone(), to.clone());
        let now = chrono::Utc::now().timestamp();

        let should_update = match self.links.get(&key) {
            Some(existing) => timestamp > existing.updated_at,
            None => true,
        };

        if should_update {
            self.links.insert(
                key,
                LinkStateEntry {
                    from: from.clone(),
                    to: to.clone(),
                    latency_ms,
                    available,
                    bandwidth_mbps,
                    updated_at: now,
                },
            );
        }

        // 传播链路状态
        vec![GossipMessage::LinkState {
            from: from.clone(),
            to: to.clone(),
            latency_ms,
            available,
            bandwidth_mbps,
            timestamp,
        }]
    }

    /// 处理全量同步
    fn handle_full_sync(&mut self, nodes: Vec<NodeInfo>, links: Vec<LinkStateEntry>) -> Vec<GossipMessage> {
        for node in nodes {
            if node.node_id != self.local_node_id {
                self.nodes.insert(node.node_id.clone(), node);
            }
        }

        for link in links {
            let key = (link.from.clone(), link.to.clone());
            self.links.insert(key, link);
        }

        tracing::info!(
            nodes = self.nodes.len(),
            links = self.links.len(),
            "Full sync completed"
        );

        Vec::new()
    }

    /// 生成本节点心跳消息
    pub fn generate_heartbeat(&mut self) -> GossipMessage {
        self.heartbeat_seq += 1;
        GossipMessage::Heartbeat {
            node_id: self.local_node_id.clone(),
            seq: self.heartbeat_seq,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 检测故障节点
    pub fn detect_failures(&mut self) -> Vec<NodeId> {
        let now = chrono::Utc::now().timestamp();
        let suspect_timeout = self.config.suspect_timeout_secs as i64;
        let dead_timeout = self.config.dead_timeout_secs as i64;

        let mut failed = Vec::new();

        for (node_id, node) in self.nodes.iter_mut() {
            if node_id == &self.local_node_id {
                continue;
            }

            let elapsed = now - node.last_heartbeat;

            match node.status {
                NodeStatus::Alive => {
                    if elapsed > suspect_timeout {
                        node.status = NodeStatus::Suspect;
                        tracing::warn!(node_id = %node_id, "Node marked as suspect");
                    }
                }
                NodeStatus::Suspect => {
                    if elapsed > dead_timeout {
                        node.status = NodeStatus::Dead;
                        tracing::warn!(node_id = %node_id, "Node marked as dead");
                        failed.push(node_id.clone());
                    }
                }
                NodeStatus::Dead => {}
            }
        }

        failed
    }

    /// 获取存活节点列表
    pub fn alive_nodes(&self) -> Vec<&NodeInfo> {
        self.nodes.values().filter(|n| n.status == NodeStatus::Alive).collect()
    }

    /// 获取所有节点
    pub fn all_nodes(&self) -> &HashMap<NodeId, NodeInfo> {
        &self.nodes
    }

    /// 获取链路状态
    pub fn link_states(&self) -> &HashMap<(NodeId, NodeId), LinkStateEntry> {
        &self.links
    }

    /// 获取两个节点之间的链路
    pub fn get_link(&self, from: &NodeId, to: &NodeId) -> Option<&LinkStateEntry> {
        self.links.get(&(from.clone(), to.clone()))
    }

    /// 获取配置
    pub fn config(&self) -> &GossipConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_node() {
        let mut membership = GossipMembership::new("node-1".to_string(), GossipConfig::default());

        let msg = GossipMessage::Join {
            node_id: "node-2".to_string(),
            addr: "10.0.0.2:8080".parse().unwrap(),
            capabilities: vec!["p2p".to_string(), "edge".to_string()],
            timestamp: chrono::Utc::now().timestamp(),
        };

        let propagated = membership.handle_message(&msg);
        assert_eq!(propagated.len(), 1);
        assert!(membership.nodes.contains_key("node-2"));
        assert_eq!(membership.nodes["node-2"].status, NodeStatus::Alive);
    }

    #[test]
    fn test_leave_node() {
        let mut membership = GossipMembership::new("node-1".to_string(), GossipConfig::default());

        // First join
        let join_msg = GossipMessage::Join {
            node_id: "node-2".to_string(),
            addr: "10.0.0.2:8080".parse().unwrap(),
            capabilities: vec![],
            timestamp: chrono::Utc::now().timestamp(),
        };
        membership.handle_message(&join_msg);

        // Then leave
        let leave_msg = GossipMessage::Leave {
            node_id: "node-2".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        };
        membership.handle_message(&leave_msg);
        assert_eq!(membership.nodes["node-2"].status, NodeStatus::Dead);
    }

    #[test]
    fn test_heartbeat() {
        let mut membership = GossipMembership::new("node-1".to_string(), GossipConfig::default());

        let join_msg = GossipMessage::Join {
            node_id: "node-2".to_string(),
            addr: "10.0.0.2:8080".parse().unwrap(),
            capabilities: vec![],
            timestamp: chrono::Utc::now().timestamp(),
        };
        membership.handle_message(&join_msg);

        let hb = GossipMessage::Heartbeat {
            node_id: "node-2".to_string(),
            seq: 1,
            timestamp: chrono::Utc::now().timestamp(),
        };
        let propagated = membership.handle_message(&hb);
        assert!(propagated.is_empty()); // Heartbeats don't propagate
    }

    #[test]
    fn test_link_state() {
        let mut membership = GossipMembership::new("node-1".to_string(), GossipConfig::default());

        let msg = GossipMessage::LinkState {
            from: "node-1".to_string(),
            to: "node-2".to_string(),
            latency_ms: 15.0,
            available: true,
            bandwidth_mbps: Some(100.0),
            timestamp: chrono::Utc::now().timestamp(),
        };

        membership.handle_message(&msg);

        let link = membership.get_link(&"node-1".to_string(), &"node-2".to_string());
        assert!(link.is_some());
        assert_eq!(link.unwrap().latency_ms, 15.0);
    }

    #[test]
    fn test_full_sync() {
        let mut membership = GossipMembership::new("node-1".to_string(), GossipConfig::default());

        let nodes = vec![
            NodeInfo {
                node_id: "node-2".to_string(),
                addr: "10.0.0.2:8080".parse().unwrap(),
                capabilities: vec!["p2p".to_string()],
                joined_at: 0,
                last_heartbeat: chrono::Utc::now().timestamp(),
                status: NodeStatus::Alive,
            },
            NodeInfo {
                node_id: "node-3".to_string(),
                addr: "10.0.0.3:8080".parse().unwrap(),
                capabilities: vec!["edge".to_string()],
                joined_at: 0,
                last_heartbeat: chrono::Utc::now().timestamp(),
                status: NodeStatus::Alive,
            },
        ];

        let msg = GossipMessage::FullSync {
            nodes,
            links: vec![],
            timestamp: chrono::Utc::now().timestamp(),
        };

        membership.handle_message(&msg);
        assert_eq!(membership.nodes.len(), 2); // node-2 and node-3 (not self)
    }

    #[test]
    fn test_generate_heartbeat() {
        let mut membership = GossipMembership::new("node-1".to_string(), GossipConfig::default());

        let hb1 = membership.generate_heartbeat();
        let hb2 = membership.generate_heartbeat();

        match (&hb1, &hb2) {
            (GossipMessage::Heartbeat { seq: s1, .. }, GossipMessage::Heartbeat { seq: s2, .. }) => {
                assert_eq!(*s2, *s1 + 1);
            }
            _ => panic!("Expected Heartbeat messages"),
        }
    }
}
