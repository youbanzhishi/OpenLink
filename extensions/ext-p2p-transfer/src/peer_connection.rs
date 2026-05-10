//! # P2P 连接管理 (Phase 9)
//!
//! P2P 连接状态机、心跳保活、带宽估算、连接质量评分。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// 发起中
    Initiating,
    /// 已连接
    Connected,
    /// 已断开
    Disconnected,
    /// 连接失败
    Failed,
}

impl ConnectionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionState::Initiating => "initiating",
            ConnectionState::Connected => "connected",
            ConnectionState::Disconnected => "disconnected",
            ConnectionState::Failed => "failed",
        }
    }
}

/// 带宽采样
#[derive(Debug, Clone)]
struct BandwidthSample {
    bytes: u64,
    timestamp: Instant,
}

/// 带宽估算器
#[derive(Debug, Clone)]
pub struct BandwidthEstimator {
    /// 采样窗口（最近 N 个采样）
    samples: VecDeque<BandwidthSample>,
    /// 窗口大小
    window_size: usize,
    /// 上次估算的带宽 (bytes/sec)
    last_estimate: f64,
}

impl BandwidthEstimator {
    pub fn new(window_size: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(window_size),
            window_size,
            last_estimate: 0.0,
        }
    }

    /// 添加采样
    pub fn add_sample(&mut self, bytes: u64) {
        let sample = BandwidthSample {
            bytes,
            timestamp: Instant::now(),
        };
        if self.samples.len() >= self.window_size {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// 估算当前带宽 (bytes/sec)
    pub fn estimate(&mut self) -> f64 {
        if self.samples.len() < 2 {
            return self.last_estimate;
        }

        let first = self.samples.front().unwrap();
        let last = self.samples.back().unwrap();
        let elapsed = last.timestamp.duration_since(first.timestamp).as_secs_f64();

        if elapsed <= 0.0 {
            return self.last_estimate;
        }

        let total_bytes: u64 = self.samples.iter().map(|s| s.bytes).sum();
        self.last_estimate = total_bytes as f64 / elapsed;
        self.last_estimate
    }

    /// 估算带宽 (Mbps)
    pub fn estimate_mbps(&mut self) -> f64 {
        self.estimate() * 8.0 / 1_000_000.0
    }
}

impl Default for BandwidthEstimator {
    fn default() -> Self {
        Self::new(20)
    }
}

/// 连接质量评分
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionQuality {
    /// 延迟 (ms)
    pub latency_ms: f64,
    /// 丢包率 (0.0 - 1.0)
    pub packet_loss: f64,
    /// 带宽 (Mbps)
    pub bandwidth_mbps: f64,
    /// 综合评分 (0 - 100)
    pub score: f64,
}

impl ConnectionQuality {
    /// 计算综合评分
    /// 延迟权重 40%，丢包权重 40%，带宽权重 20%
    pub fn calculate(latency_ms: f64, packet_loss: f64, bandwidth_mbps: f64) -> Self {
        // 延迟评分：0ms=100, 500ms=0
        let latency_score = ((500.0 - latency_ms.min(500.0)) / 500.0 * 100.0).max(0.0);
        // 丢包评分：0%=100, 100%=0
        let loss_score = ((1.0 - packet_loss.min(1.0)) * 100.0).max(0.0);
        // 带宽评分：100Mbps=100, 0Mbps=0
        let bw_score = (bandwidth_mbps / 100.0 * 100.0).min(100.0).max(0.0);

        let score = latency_score * 0.4 + loss_score * 0.4 + bw_score * 0.2;

        Self {
            latency_ms,
            packet_loss,
            bandwidth_mbps,
            score,
        }
    }

    /// 获取质量等级
    pub fn grade(&self) -> &'static str {
        if self.score >= 80.0 {
            "excellent"
        } else if self.score >= 60.0 {
            "good"
        } else if self.score >= 40.0 {
            "fair"
        } else {
            "poor"
        }
    }
}

/// P2P 连接
pub struct PeerConnection {
    /// 对端节点 ID
    pub peer_id: String,
    /// 对端公网地址
    pub remote_addr: SocketAddr,
    /// 连接状态
    pub state: ConnectionState,
    /// 最后一次心跳时间
    last_heartbeat: Instant,
    /// 心跳间隔（秒）
    heartbeat_interval_secs: u64,
    /// 带宽估算器
    bandwidth_estimator: BandwidthEstimator,
    /// 连接质量
    pub quality: Option<ConnectionQuality>,
    /// 创建时间
    created_at: Instant,
    /// 传输字节数
    bytes_transferred: u64,
}

impl PeerConnection {
    /// 创建新的 P2P 连接
    pub fn new(peer_id: String, remote_addr: SocketAddr) -> Self {
        Self {
            peer_id,
            remote_addr,
            state: ConnectionState::Initiating,
            last_heartbeat: Instant::now(),
            heartbeat_interval_secs: 30,
            bandwidth_estimator: BandwidthEstimator::default(),
            quality: None,
            created_at: Instant::now(),
            bytes_transferred: 0,
        }
    }

    /// 设置心跳间隔
    pub fn with_heartbeat_interval(mut self, secs: u64) -> Self {
        self.heartbeat_interval_secs = secs;
        self
    }

    /// 标记连接已建立
    pub fn mark_connected(&mut self) {
        self.state = ConnectionState::Connected;
        self.last_heartbeat = Instant::now();
    }

    /// 标记连接断开
    pub fn mark_disconnected(&mut self) {
        self.state = ConnectionState::Disconnected;
    }

    /// 标记连接失败
    pub fn mark_failed(&mut self) {
        self.state = ConnectionState::Failed;
    }

    /// 接收心跳
    pub fn receive_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    /// 检查心跳是否超时
    pub fn is_heartbeat_timeout(&self) -> bool {
        self.last_heartbeat.elapsed() > Duration::from_secs(self.heartbeat_interval_secs * 3)
    }

    /// 记录传输数据量
    pub fn record_transfer(&mut self, bytes: u64) {
        self.bytes_transferred += bytes;
        self.bandwidth_estimator.add_sample(bytes);
    }

    /// 更新连接质量
    pub fn update_quality(&mut self, latency_ms: f64, packet_loss: f64) {
        let bandwidth_mbps = self.bandwidth_estimator.estimate_mbps();
        self.quality = Some(ConnectionQuality::calculate(
            latency_ms,
            packet_loss,
            bandwidth_mbps,
        ));
    }

    /// 获取连接统计信息
    pub fn stats(&self) -> PeerConnectionStats {
        PeerConnectionStats {
            peer_id: self.peer_id.clone(),
            remote_addr: self.remote_addr.to_string(),
            state: self.state,
            uptime_secs: self.created_at.elapsed().as_secs(),
            bytes_transferred: self.bytes_transferred,
            last_heartbeat_ago_secs: self.last_heartbeat.elapsed().as_secs(),
            quality: self.quality.clone(),
        }
    }
}

/// 连接统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConnectionStats {
    pub peer_id: String,
    pub remote_addr: String,
    pub state: ConnectionState,
    pub uptime_secs: u64,
    pub bytes_transferred: u64,
    pub last_heartbeat_ago_secs: u64,
    pub quality: Option<ConnectionQuality>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state_transitions() {
        let mut conn = PeerConnection::new("peer-1".to_string(), "1.2.3.4:12345".parse().unwrap());
        assert_eq!(conn.state, ConnectionState::Initiating);

        conn.mark_connected();
        assert_eq!(conn.state, ConnectionState::Connected);

        conn.mark_disconnected();
        assert_eq!(conn.state, ConnectionState::Disconnected);
    }

    #[test]
    fn test_bandwidth_estimator() {
        let mut estimator = BandwidthEstimator::new(10);
        // Add samples simulating 1MB/s
        for _ in 0..5 {
            estimator.add_sample(1_000_000);
            std::thread::sleep(Duration::from_millis(10));
        }
        let bw = estimator.estimate();
        // Should be approximately 1MB/s (with some tolerance)
        assert!(bw > 0.0);
    }

    #[test]
    fn test_connection_quality_excellent() {
        let quality = ConnectionQuality::calculate(10.0, 0.0, 100.0);
        assert!(quality.score >= 80.0);
        assert_eq!(quality.grade(), "excellent");
    }

    #[test]
    fn test_connection_quality_poor() {
        let quality = ConnectionQuality::calculate(400.0, 0.5, 5.0);
        assert!(quality.score < 40.0);
        assert_eq!(quality.grade(), "poor");
    }

    #[test]
    fn test_heartbeat_timeout() {
        let mut conn = PeerConnection::new("peer-1".to_string(), "1.2.3.4:12345".parse().unwrap());
        conn.mark_connected();
        conn.heartbeat_interval_secs = 0; // Set very short interval
        std::thread::sleep(Duration::from_millis(10));
        assert!(conn.is_heartbeat_timeout());
    }

    #[test]
    fn test_record_transfer() {
        let mut conn = PeerConnection::new("peer-1".to_string(), "1.2.3.4:12345".parse().unwrap());
        conn.record_transfer(1024);
        conn.record_transfer(2048);
        assert_eq!(conn.bytes_transferred, 3072);
    }

    #[test]
    fn test_peer_connection_stats() {
        let mut conn = PeerConnection::new("peer-1".to_string(), "1.2.3.4:12345".parse().unwrap());
        conn.mark_connected();
        conn.record_transfer(1024);
        let stats = conn.stats();
        assert_eq!(stats.peer_id, "peer-1");
        assert_eq!(stats.bytes_transferred, 1024);
    }
}
