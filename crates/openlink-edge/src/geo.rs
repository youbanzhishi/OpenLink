//! # 地理路由（Phase 5）
//!
//! 根据请求来源 IP 选择最近节点。
//! 简化版：基于 IP 段映射到区域，区域映射到节点。
//!
//! ## 设计
//! - IP 段 → 区域映射（静态配置，无外部 GeoIP 依赖）
//! - 区域 → 节点映射（可配置）
//! - 默认回退节点

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;

/// 区域标识
pub type RegionId = String;

/// 节点端点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEndpoint {
    /// 节点 ID
    pub node_id: String,
    /// 节点地址（URL）
    pub address: String,
    /// 区域
    pub region: RegionId,
    /// 优先级（数值越小越优先）
    pub priority: u32,
    /// 是否在线
    pub is_online: bool,
}

/// IP 段到区域的映射规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpRangeRule {
    /// 网络前缀，如 "10.0.0.0/8"、"192.168.1.0/24"
    pub network: String,
    /// 对应区域
    pub region: RegionId,
}

/// 地理路由配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoRouteConfig {
    /// 默认回退节点 ID
    pub default_node_id: String,
    /// IP 段规则列表
    pub ip_rules: Vec<IpRangeRule>,
    /// 区域到节点映射：region → [NodeEndpoint]
    pub region_nodes: HashMap<RegionId, Vec<NodeEndpoint>>,
}

impl Default for GeoRouteConfig {
    fn default() -> Self {
        let mut region_nodes = HashMap::new();

        // 默认配置：3 个区域
        region_nodes.insert(
            "cn-east".to_string(),
            vec![NodeEndpoint {
                node_id: "edge-cn-east-1".to_string(),
                address: "https://cn-east.edge.openlink.dev".to_string(),
                region: "cn-east".to_string(),
                priority: 1,
                is_online: true,
            }],
        );
        region_nodes.insert(
            "cn-south".to_string(),
            vec![NodeEndpoint {
                node_id: "edge-cn-south-1".to_string(),
                address: "https://cn-south.edge.openlink.dev".to_string(),
                region: "cn-south".to_string(),
                priority: 1,
                is_online: true,
            }],
        );
        region_nodes.insert(
            "us-west".to_string(),
            vec![NodeEndpoint {
                node_id: "edge-us-west-1".to_string(),
                address: "https://us-west.edge.openlink.dev".to_string(),
                region: "us-west".to_string(),
                priority: 1,
                is_online: true,
            }],
        );

        Self {
            default_node_id: "edge-cn-east-1".to_string(),
            ip_rules: vec![
                IpRangeRule {
                    network: "10.0.0.0/8".to_string(),
                    region: "cn-east".to_string(),
                },
                IpRangeRule {
                    network: "172.16.0.0/12".to_string(),
                    region: "cn-south".to_string(),
                },
                IpRangeRule {
                    network: "192.168.0.0/16".to_string(),
                    region: "us-west".to_string(),
                },
            ],
            region_nodes,
        }
    }
}

/// 简化版 CIDR 匹配
#[derive(Debug, Clone)]
struct CidrMatch {
    /// 网络地址的数值
    network_addr: u32,
    /// 掩码
    mask: u32,
    /// 对应区域
    region: RegionId,
}

impl CidrMatch {
    fn from_str(cidr: &str, region: RegionId) -> Option<Self> {
        let parts: Vec<&str> = cidr.split('/').collect();
        if parts.len() != 2 {
            return None;
        }

        let ip_str = parts[0];
        let prefix_len: u32 = parts[1].parse().ok()?;

        let addr = parse_ipv4(ip_str)?;
        let mask = if prefix_len == 0 {
            0
        } else {
            !0u32 << (32 - prefix_len)
        };

        Some(Self {
            network_addr: addr & mask,
            mask,
            region,
        })
    }

    fn matches(&self, ip: u32) -> bool {
        (ip & self.mask) == self.network_addr
    }
}

/// 将 IPv4 字符串解析为 u32
fn parse_ipv4(s: &str) -> Option<u32> {
    let octets: Vec<u8> = s.split('.').filter_map(|o| o.parse().ok()).collect();
    if octets.len() != 4 {
        return None;
    }
    Some(
        ((octets[0] as u32) << 24)
            | ((octets[1] as u32) << 16)
            | ((octets[2] as u32) << 8)
            | (octets[3] as u32),
    )
}

/// 将 IpAddr 转为 u32（仅支持 IPv4）
fn ip_to_u32(ip: &IpAddr) -> Option<u32> {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            Some(
                ((octets[0] as u32) << 24)
                    | ((octets[1] as u32) << 16)
                    | ((octets[2] as u32) << 8)
                    | (octets[3] as u32),
            )
        }
        IpAddr::V6(_) => None, // IPv6 简化版暂不支持
    }
}

/// 地理路由器
pub struct GeoRouter {
    config: GeoRouteConfig,
    cidr_rules: Vec<CidrMatch>,
}

impl GeoRouter {
    /// 创建地理路由器
    pub fn new(config: GeoRouteConfig) -> Self {
        let cidr_rules: Vec<CidrMatch> = config
            .ip_rules
            .iter()
            .filter_map(|rule| CidrMatch::from_str(&rule.network, rule.region.clone()))
            .collect();

        tracing::info!(rules = cidr_rules.len(), "Geo router initialized");
        Self { config, cidr_rules }
    }

    /// 根据客户端 IP 选择最近节点
    pub fn resolve(&self, client_ip: &str) -> &NodeEndpoint {
        // 尝试解析 IP
        let ip = match IpAddr::from_str(client_ip) {
            Ok(ip) => ip,
            Err(_) => return self.default_node(),
        };

        // 匹配 IP 段规则
        if let Some(ip_u32) = ip_to_u32(&ip) {
            for rule in &self.cidr_rules {
                if rule.matches(ip_u32) {
                    if let Some(node) = self.find_best_node(&rule.region) {
                        tracing::debug!(
                            client_ip = %client_ip,
                            region = %rule.region,
                            node = %node.node_id,
                            "Geo route matched"
                        );
                        return node;
                    }
                }
            }
        }

        tracing::debug!(client_ip = %client_ip, "No geo rule matched, using default");
        self.default_node()
    }

    /// 获取所有在线节点
    pub fn online_nodes(&self) -> Vec<&NodeEndpoint> {
        self.config
            .region_nodes
            .values()
            .flat_map(|nodes| nodes.iter())
            .filter(|n| n.is_online)
            .collect()
    }

    /// 获取指定区域的在线节点
    pub fn nodes_in_region(&self, region: &str) -> Vec<&NodeEndpoint> {
        self.config
            .region_nodes
            .get(region)
            .map(|nodes| nodes.iter().filter(|n| n.is_online).collect())
            .unwrap_or_default()
    }

    /// 在指定区域中找到最优节点
    fn find_best_node(&self, region: &str) -> Option<&NodeEndpoint> {
        let mut nodes = self.nodes_in_region(region);
        nodes.sort_by_key(|n| n.priority);
        nodes.into_iter().next()
    }

    /// 默认回退节点
    fn default_node(&self) -> &NodeEndpoint {
        // 先尝试在所有区域中找默认节点
        for nodes in self.config.region_nodes.values() {
            for node in nodes {
                if node.node_id == self.config.default_node_id && node.is_online {
                    return node;
                }
            }
        }

        // 如果默认节点不在线，取第一个在线节点
        for nodes in self.config.region_nodes.values() {
            if let Some(node) = nodes.iter().find(|n| n.is_online) {
                return node;
            }
        }

        // 最终回退：取第一个节点（无论如何）
        self.config
            .region_nodes
            .values()
            .next()
            .and_then(|nodes| nodes.first())
            .expect("At least one node must be configured")
    }

    /// 标记节点在线/离线
    pub fn set_node_status(&mut self, node_id: &str, online: bool) {
        for nodes in self.config.region_nodes.values_mut() {
            for node in nodes.iter_mut() {
                if node.node_id == node_id {
                    node.is_online = online;
                    tracing::info!(node_id = %node_id, online, "Node status updated");
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_route_cn_east() {
        let router = GeoRouter::new(GeoRouteConfig::default());
        let node = router.resolve("10.0.1.100");
        assert_eq!(node.region, "cn-east");
    }

    #[test]
    fn test_geo_route_cn_south() {
        let router = GeoRouter::new(GeoRouteConfig::default());
        let node = router.resolve("172.16.5.20");
        assert_eq!(node.region, "cn-south");
    }

    #[test]
    fn test_geo_route_us_west() {
        let router = GeoRouter::new(GeoRouteConfig::default());
        let node = router.resolve("192.168.1.1");
        assert_eq!(node.region, "us-west");
    }

    #[test]
    fn test_geo_route_unknown_ip() {
        let router = GeoRouter::new(GeoRouteConfig::default());
        // 8.8.8.8 不在规则中，应回退到默认节点
        let node = router.resolve("8.8.8.8");
        assert_eq!(node.node_id, "edge-cn-east-1");
    }

    #[test]
    fn test_geo_route_invalid_ip() {
        let router = GeoRouter::new(GeoRouteConfig::default());
        let node = router.resolve("not-an-ip");
        assert_eq!(node.node_id, "edge-cn-east-1"); // 回退到默认
    }

    #[test]
    fn test_parse_ipv4() {
        assert_eq!(parse_ipv4("10.0.0.0"), Some(0x0A000000));
        assert_eq!(parse_ipv4("192.168.1.1"), Some(0xC0A80101));
        assert_eq!(parse_ipv4("0.0.0.0"), Some(0));
        assert_eq!(parse_ipv4("invalid"), None);
    }

    #[test]
    fn test_cidr_match() {
        let cidr = CidrMatch::from_str("10.0.0.0/8", "test".to_string()).unwrap();
        assert!(cidr.matches(0x0A000001)); // 10.0.0.1
        assert!(!cidr.matches(0x0B000001)); // 11.0.0.1
    }

    #[test]
    fn test_set_node_status() {
        let mut router = GeoRouter::new(GeoRouteConfig::default());
        router.set_node_status("edge-cn-east-1", false);

        // 默认节点下线后，应回退到其他在线节点
        let node = router.resolve("10.0.1.1");
        // cn-east 区域的节点下线了，应回退到其他区域
        assert!(node.is_online);
    }

    #[test]
    fn test_online_nodes() {
        let router = GeoRouter::new(GeoRouteConfig::default());
        let nodes = router.online_nodes();
        assert_eq!(nodes.len(), 3); // 3 个默认在线节点
    }
}
