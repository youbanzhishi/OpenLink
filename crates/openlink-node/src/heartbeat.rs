//! # 心跳上报模块
//!
//! 定期向 OpenLink Server 上报设备状态。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// 节点状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub node_id: String,
    pub lan_ip: String,
    pub port: u16,
    pub version: String,
    pub capabilities: Vec<String>,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub disk_free_mb: u64,
    pub peer_count: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl NodeStatus {
    pub fn new(node_id: String, lan_ip: String, port: u16, version: String) -> Self {
        Self {
            node_id,
            lan_ip,
            port,
            version,
            capabilities: vec![
                "file_server".to_string(),
                "heartbeat".to_string(),
                "encrypted_transfer".to_string(),
            ],
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_free_mb: 0,
            peer_count: 0,
            timestamp: chrono::Utc::now(),
        }
    }

    /// 从系统获取实时状态
    pub async fn collect() -> Self {
        let mut status = Self::new(
            "local".to_string(),
            local_ip().await.unwrap_or_else(|| "127.0.0.1".to_string()),
            8080,
            env!("CARGO_PKG_VERSION").to_string(),
        );

        #[cfg(unix)]
        {
            status.cpu_usage = get_cpu_usage().await.unwrap_or(0.0);
            status.memory_usage = get_memory_usage().await.unwrap_or(0.0);
            status.disk_free_mb = get_disk_free_mb().await.unwrap_or(0);
        }

        status
    }
}

/// 心跳客户端
pub struct HeartbeatClient {
    client: Client,
    server_url: String,
    node_id: String,
    interval_secs: u64,
}

impl HeartbeatClient {
    pub fn new(server_url: &str, node_id: &str, interval_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            server_url: server_url.to_string(),
            node_id: node_id.to_string(),
            interval_secs,
        }
    }

    pub async fn send_heartbeat(&self) -> Result<(), HeartbeatError> {
        let status = NodeStatus::collect().await;

        let resp = self
            .client
            .post(format!(
                "{}/api/v1/node/heartbeat",
                self.server_url.trim_end_matches('/')
            ))
            .json(&status)
            .send()
            .await
            .map_err(|e| HeartbeatError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(HeartbeatError::ServerError(resp.status().as_u16()));
        }

        tracing::debug!(node_id = %self.node_id, "Heartbeat sent");
        Ok(())
    }

    pub async fn start(self: Arc<Self>) {
        let interval = Duration::from_secs(self.interval_secs);
        tracing::info!(
            node_id = %self.node_id,
            interval_secs = self.interval_secs,
            "Starting heartbeat"
        );

        loop {
            if let Err(e) = self.send_heartbeat().await {
                tracing::warn!(error = %e, "Heartbeat failed");
            }
            tokio::time::sleep(interval).await;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HeartbeatError {
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Server error: {0}")]
    ServerError(u16),
}

async fn local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

async fn get_cpu_usage() -> Option<f32> {
    #[cfg(unix)]
    {
        let stat = tokio::fs::read_to_string("/proc/stat").await.ok()?;
        let first_line = stat.lines().next()?;
        let values: Vec<u64> = first_line
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse().ok())
            .collect();

        if values.len() >= 4 {
            let total: u64 = values.iter().sum();
            let idle = values.get(3).copied().unwrap_or(0);
            return Some((1.0 - idle as f32 / total.max(1) as f32) * 100.0);
        }
    }
    None
}

async fn get_memory_usage() -> Option<f32> {
    #[cfg(unix)]
    {
        let meminfo = tokio::fs::read_to_string("/proc/meminfo").await.ok()?;
        let parse_field = |field: &str| -> Option<u64> {
            meminfo
                .lines()
                .find(|l| l.starts_with(field))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        };

        let total = parse_field("MemTotal:")? * 1024;
        let available = parse_field("MemAvailable:")? * 1024;
        if total > 0 {
            return Some((1.0 - available as f32 / total as f32) * 100.0);
        }
    }
    None
}

async fn get_disk_free_mb() -> Option<u64> {
    #[cfg(unix)]
    {
        let stat = std::fs::read_to_string("/proc/mounts").ok()?;
        if stat.lines().any(|l| l.contains(" / ")) {
            return Some(0); // Simplified
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_status_creation() {
        let status = NodeStatus::new(
            "node-test-1".to_string(),
            "192.168.1.100".to_string(),
            8080,
            "0.2.0".to_string(),
        );
        assert_eq!(status.node_id, "node-test-1");
        assert!(status.capabilities.contains(&"file_server".to_string()));
    }

    #[test]
    fn test_node_status_serialization() {
        let status = NodeStatus::new(
            "node-1".to_string(),
            "192.168.1.100".to_string(),
            8080,
            "0.2.0".to_string(),
        );
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("node-1"));
    }
}
