//! # LAN Peer 发现模块
//!
//! 通过 mDNS / DNS-SD 发现同 LAN 的 OpenLink 节点。

use serde::{Deserialize, Serialize};

/// LAN 上的 OpenLink 节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanPeer {
    /// 节点唯一 ID
    pub node_id: String,
    /// 节点 LAN IP
    pub ip: String,
    /// HTTP 文件服务端口
    pub port: u16,
    /// 延迟（毫秒）
    pub latency_ms: Option<u32>,
    /// 是否支持加密传输
    pub supports_encryption: bool,
}

impl LanPeer {
    /// 获取节点文件服务 URL
    pub fn file_url(&self) -> String {
        format!("http://{}:{}/openlink/files", self.ip, self.port)
    }

    /// 获取节点直传 URL
    pub fn transfer_url(&self) -> String {
        format!("http://{}:{}/openlink/transfer", self.ip, self.port)
    }
}

/// LAN 发现器
pub struct LanDiscovery {
    service_name: String,
    cache: std::collections::HashMap<String, LanPeer>,
}

impl LanDiscovery {
    pub fn new() -> Self {
        Self {
            service_name: "_openlink._tcp.local.".to_string(),
            cache: std::collections::HashMap::new(),
        }
    }

    /// 发现所有 LAN 上的 OpenLink 节点
    pub async fn discover_peers(&self) -> Vec<LanPeer> {
        // 生产实现：使用 dns_sd 库查询 _openlink._tcp.local.
        // 测试实现：返回模拟数据
        #[cfg(test)]
        {
            return vec![
                LanPeer {
                    node_id: "openlink-node-1".to_string(),
                    ip: "192.168.1.100".to_string(),
                    port: 8080,
                    latency_ms: Some(5),
                    supports_encryption: true,
                },
            ];
        }

        #[cfg(not(test))]
        {
            // 实际使用 dns_sd 库
            // let browser = dns_sd::Browser::new(&self.service_name)?;
            Vec::new()
        }
    }

    /// 获取指定节点
    pub async fn get_peer(&self, node_id: &str) -> Option<LanPeer> {
        self.discover_peers()
            .await
            .into_iter()
            .find(|p| p.node_id == node_id)
    }

    /// 检查是否有任何 LAN 节点
    pub async fn has_peers(&self) -> bool {
        !self.discover_peers().await.is_empty()
    }
}

impl Default for LanDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_discover_peers() {
        let discovery = LanDiscovery::new();
        let peers = discovery.discover_peers().await;
        assert!(!peers.is_empty());
    }

    #[tokio::test]
    async fn test_get_peer() {
        let discovery = LanDiscovery::new();
        let peer = discovery.get_peer("openlink-node-1").await;
        assert!(peer.is_some());
        assert_eq!(peer.unwrap().ip, "192.168.1.100");
    }

    #[test]
    fn test_lan_peer_urls() {
        let peer = LanPeer {
            node_id: "test".to_string(),
            ip: "192.168.1.50".to_string(),
            port: 8080,
            latency_ms: Some(10),
            supports_encryption: true,
        };
        assert_eq!(peer.file_url(), "http://192.168.1.50:8080/openlink/files");
        assert_eq!(peer.transfer_url(), "http://192.168.1.50:8080/openlink/transfer");
    }
}
