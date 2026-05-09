//! # mDNS 节点发现模块
//!
//! 使用 DNS-SD / mDNS 自动发现同 LAN 的 OpenLink 节点。
//! 
//! 服务类型：`_openlink._tcp.local.`
//! 
//! 每个节点广播以下 TXT 记录：
//! - `node_id`: 节点唯一标识
//! - `version`: 节点软件版本
//! - `capabilities`: 节点支持的能力（file_server, heartbeat, encrypted_transfer）
//! - `port`: HTTP 服务端口

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

/// 已发现的 OpenLink 节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredNode {
    /// 节点唯一 ID
    pub node_id: String,
    /// 节点 IP 地址
    pub ip: String,
    /// HTTP 服务端口
    pub port: u16,
    /// 软件版本
    pub version: String,
    /// 节点能力
    pub capabilities: Vec<String>,
    /// 最后发现时间
    pub discovered_at: chrono::DateTime<chrono::Utc>,
    /// 延迟（毫秒）
    pub latency_ms: Option<u32>,
}

impl DiscoveredNode {
    /// 获取节点的文件服务 URL
    pub fn file_service_url(&self) -> String {
        format!("http://{}:{}/openlink/files", self.ip, self.port)
    }

    /// 获取节点的心跳 URL
    pub fn heartbeat_url(&self, server_url: &str) -> String {
        format!("{}/api/v1/node/heartbeat", server_url.trim_end_matches('/'))
    }

    /// 检查节点是否支持指定能力
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

/// 节点发现器
pub struct NodeDiscovery {
    service_name: String,
    /// 本节点信息
    local_node: Option<DiscoveredNode>,
}

impl NodeDiscovery {
    /// 创建节点发现器
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
            local_node: None,
        }
    }

    /// 启动广播（将本节点注册到 mDNS）
    pub async fn start_broadcast(&mut self, node: DiscoveredNode) -> Result<(), DiscoveryError> {
        self.local_node = Some(node.clone());
        tracing::info!(
            node_id = %node.node_id,
            ip = %node.ip,
            port = node.port,
            "Starting mDNS broadcast"
        );
        // 实际实现：使用 dns_sd 库注册服务
        // dns_sd::register(...);
        Ok(())
    }

    /// 停止广播
    pub async fn stop_broadcast(&mut self) {
        if self.local_node.is_some() {
            tracing::info!("Stopping mDNS broadcast");
            self.local_node = None;
        }
    }

    /// 发现所有同 LAN 节点
    pub async fn discover(&self) -> Result<Vec<DiscoveredNode>, DiscoveryError> {
        tracing::debug!(service = %self.service_name, "Discovering LAN nodes");
        // 实际实现：使用 dns_sd 库浏览服务
        // let browser = dns_sd::Browser::new(&self.service_name)?;
        // for event in browser.flat_map(|e| e) { ... }
        Ok(Vec::new())
    }

    /// 等待发现至少 N 个节点
    pub async fn discover_at_least(&self, min_count: usize, timeout: Duration) -> Result<Vec<DiscoveredNode>, DiscoveryError> {
        let start = std::time::Instant::now();
        loop {
            let peers = self.discover().await?;
            if peers.len() >= min_count {
                return Ok(peers);
            }
            if start.elapsed() > timeout {
                return Ok(peers); // 超时也返回已发现的节点
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// 获取本节点信息
    pub fn local_node(&self) -> Option<&DiscoveredNode> {
        self.local_node.as_ref()
    }
}

/// 发现错误
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("mDNS error: {0}")]
    MdnsError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Timeout")]
    Timeout,
}

/// TXT 记录解析
pub fn parse_txt_record(txt: &[u8]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < txt.len() {
        let len = txt[i] as usize;
        i += 1;
        if i + len <= txt.len() {
            let entry = String::from_utf8_lossy(&txt[i..i + len]);
            if let Some((k, v)) = entry.split_once('=') {
                result.push((k.to_string(), v.to_string()));
            }
        }
        i += len;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_txt_record() {
        let txt = b"\x0bnode_id=abc\x0aversion=1.0";
        let parsed = parse_txt_record(txt);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "node_id");
        assert_eq!(parsed[0].1, "abc");
    }

    #[test]
    fn test_discovered_node_url() {
        let node = DiscoveredNode {
            node_id: "node-1".to_string(),
            ip: "192.168.1.100".to_string(),
            port: 8080,
            version: "0.2.0".to_string(),
            capabilities: vec!["file_server".to_string()],
            discovered_at: chrono::Utc::now(),
            latency_ms: Some(5),
        };
        assert_eq!(node.file_service_url(), "http://192.168.1.100:8080/openlink/files");
    }

    #[test]
    fn test_node_supports_capability() {
        let node = DiscoveredNode {
            node_id: "node-1".to_string(),
            ip: "192.168.1.100".to_string(),
            port: 8080,
            version: "0.2.0".to_string(),
            capabilities: vec!["file_server".to_string(), "encrypted_transfer".to_string()],
            discovered_at: chrono::Utc::now(),
            latency_ms: None,
        };
        assert!(node.supports("file_server"));
        assert!(node.supports("encrypted_transfer"));
        assert!(!node.supports("heartbeat"));
    }
}
