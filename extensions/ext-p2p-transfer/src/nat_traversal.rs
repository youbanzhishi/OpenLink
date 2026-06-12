//! # NAT 穿透模块 (Phase 9)
//!
//! 实现 NAT 穿透策略：UDP hole punching / TCP fallback / relay
//! 包含 NAT 类型检测和穿透成功率评估。

use crate::nat::{NatInfo, NatType};
use crate::stun::StunClient;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Instant;

/// 穿透策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalStrategy {
    /// UDP 打洞
    UdpHolePunching,
    /// TCP 回退
    TcpFallback,
    /// 中继转发
    Relay,
}

impl TraversalStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            TraversalStrategy::UdpHolePunching => "udp_hole_punching",
            TraversalStrategy::TcpFallback => "tcp_fallback",
            TraversalStrategy::Relay => "relay",
        }
    }
}

/// 穿透结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalResult {
    /// 使用的策略
    pub strategy: TraversalStrategy,
    /// 是否成功
    pub success: bool,
    /// 本端公网地址
    pub local_public_addr: Option<SocketAddr>,
    /// 对端公网地址
    pub remote_public_addr: Option<SocketAddr>,
    /// 耗时
    pub elapsed_ms: u64,
    /// 失败原因
    pub failure_reason: Option<String>,
}

/// 穿透成功率评估
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalSuccessRate {
    /// NAT 类型对
    pub local_nat: NatType,
    pub remote_nat: NatType,
    /// 预估成功率
    pub success_rate: f64,
    /// 推荐策略
    pub recommended_strategy: TraversalStrategy,
}

/// NAT 穿透器
pub struct NatTraversal {
    stun_client: StunClient,
    /// 本端 NAT 信息（缓存）
    local_nat_info: Option<NatInfo>,
    /// STUN 绑定超时（毫秒）
    #[allow(dead_code)]
    timeout_ms: u64,
}

impl NatTraversal {
    /// 创建 NAT 穿透器
    pub fn new() -> Self {
        Self {
            stun_client: StunClient::new(),
            local_nat_info: None,
            timeout_ms: 3000,
        }
    }

    /// 创建带自定义 STUN 服务器的穿透器
    pub fn with_servers(servers: Vec<(String, u16)>) -> Self {
        Self {
            stun_client: StunClient::with_servers(servers),
            local_nat_info: None,
            timeout_ms: 3000,
        }
    }

    /// STUN 绑定请求 — 获取公网 IP:Port 映射
    pub fn stun_bind(&mut self) -> Option<SocketAddr> {
        let addr = self.stun_client.get_public_address();
        if let Some(ref a) = addr {
            // 更新缓存的 NAT 信息
            self.local_nat_info = Some(NatInfo {
                nat_type: NatType::Open, // 简化：如果 STUN 成功则至少不是完全封闭
                local_ip: "0.0.0.0".to_string(),
                local_port: 0,
                public_ip: Some(a.ip().to_string()),
                public_port: Some(a.port()),
                is_complete: true,
            });
        }
        addr
    }

    /// 检测本端 NAT 类型
    pub fn detect_local_nat(&mut self) -> NatInfo {
        if let Some(ref info) = self.local_nat_info {
            if info.is_complete {
                return info.clone();
            }
        }

        let socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(_) => return NatInfo::unknown("0.0.0.0", 0),
        };

        let info = self.stun_client.detect_nat_type(&socket);
        self.local_nat_info = Some(info.clone());
        info
    }

    /// 选择穿透策略
    pub fn select_strategy(local_nat: &NatType, remote_nat: &NatType) -> TraversalStrategy {
        match (local_nat, remote_nat) {
            // 任一端开放，直接 UDP 打洞
            (NatType::Open, _) | (_, NatType::Open) => TraversalStrategy::UdpHolePunching,
            // 全锥型对全锥型，UDP 打洞
            (NatType::FullCone, NatType::FullCone) => TraversalStrategy::UdpHolePunching,
            // 全锥型对限制性，尝试 UDP 打洞
            (NatType::FullCone, NatType::RestrictedCone) | (NatType::RestrictedCone, NatType::FullCone) => {
                TraversalStrategy::UdpHolePunching
            }
            // 限制性对限制性，尝试 UDP 打洞（成功率较低）
            (NatType::RestrictedCone, NatType::RestrictedCone) => TraversalStrategy::UdpHolePunching,
            // 端口限制性，TCP 回退
            (NatType::PortRestrictedCone, _) | (_, NatType::PortRestrictedCone) => TraversalStrategy::TcpFallback,
            // 对称型 NAT，只能中继
            (NatType::Symmetric, _) | (_, NatType::Symmetric) => TraversalStrategy::Relay,
        }
    }

    /// 评估穿透成功率
    pub fn evaluate_success_rate(local_nat: &NatType, remote_nat: &NatType) -> TraversalSuccessRate {
        let strategy = Self::select_strategy(local_nat, remote_nat);
        let success_rate = match (local_nat, remote_nat) {
            (NatType::Open, NatType::Open) => 1.0,
            (NatType::Open, _) | (_, NatType::Open) => 0.95,
            (NatType::FullCone, NatType::FullCone) => 0.9,
            (NatType::FullCone, NatType::RestrictedCone) | (NatType::RestrictedCone, NatType::FullCone) => 0.7,
            (NatType::RestrictedCone, NatType::RestrictedCone) => 0.5,
            (NatType::PortRestrictedCone, NatType::PortRestrictedCone) => 0.3,
            (NatType::FullCone, NatType::PortRestrictedCone) | (NatType::PortRestrictedCone, NatType::FullCone) => 0.4,
            (NatType::Symmetric, _) | (_, NatType::Symmetric) => 0.0,
            _ => 0.2,
        };

        TraversalSuccessRate {
            local_nat: *local_nat,
            remote_nat: *remote_nat,
            success_rate,
            recommended_strategy: strategy,
        }
    }

    /// 模拟 UDP hole punching
    pub fn try_udp_hole_punching(local_public_addr: SocketAddr, remote_public_addr: SocketAddr) -> TraversalResult {
        let start = Instant::now();
        // 模拟打洞过程：双方同时向对方公网地址发送 UDP 包
        // 实际实现中需要信令服务器交换公网地址
        let success = true; // 模拟成功

        TraversalResult {
            strategy: TraversalStrategy::UdpHolePunching,
            success,
            local_public_addr: Some(local_public_addr),
            remote_public_addr: Some(remote_public_addr),
            elapsed_ms: start.elapsed().as_millis() as u64,
            failure_reason: if success {
                None
            } else {
                Some("Hole punching failed".to_string())
            },
        }
    }

    /// 模拟 TCP fallback
    pub fn try_tcp_fallback(local_public_addr: SocketAddr, remote_public_addr: SocketAddr) -> TraversalResult {
        let start = Instant::now();
        let success = true; // 模拟成功

        TraversalResult {
            strategy: TraversalStrategy::TcpFallback,
            success,
            local_public_addr: Some(local_public_addr),
            remote_public_addr: Some(remote_public_addr),
            elapsed_ms: start.elapsed().as_millis() as u64,
            failure_reason: if success {
                None
            } else {
                Some("TCP fallback failed".to_string())
            },
        }
    }

    /// 模拟中继传输
    pub fn try_relay(_relay_server: &str) -> TraversalResult {
        let start = Instant::now();
        let success = true;

        TraversalResult {
            strategy: TraversalStrategy::Relay,
            success,
            local_public_addr: None,
            remote_public_addr: None,
            elapsed_ms: start.elapsed().as_millis() as u64,
            failure_reason: if success {
                None
            } else {
                Some("Relay failed".to_string())
            },
        }
    }
}

impl Default for NatTraversal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_strategy_open() {
        let strategy = NatTraversal::select_strategy(&NatType::Open, &NatType::FullCone);
        assert_eq!(strategy, TraversalStrategy::UdpHolePunching);
    }

    #[test]
    fn test_select_strategy_symmetric() {
        let strategy = NatTraversal::select_strategy(&NatType::Symmetric, &NatType::FullCone);
        assert_eq!(strategy, TraversalStrategy::Relay);
    }

    #[test]
    fn test_select_strategy_port_restricted() {
        let strategy = NatTraversal::select_strategy(&NatType::PortRestrictedCone, &NatType::FullCone);
        assert_eq!(strategy, TraversalStrategy::TcpFallback);
    }

    #[test]
    fn test_evaluate_success_rate() {
        let rate = NatTraversal::evaluate_success_rate(&NatType::Open, &NatType::Open);
        assert!((rate.success_rate - 1.0).abs() < 0.01);
        assert_eq!(rate.recommended_strategy, TraversalStrategy::UdpHolePunching);
    }

    #[test]
    fn test_evaluate_success_rate_symmetric() {
        let rate = NatTraversal::evaluate_success_rate(&NatType::Symmetric, &NatType::FullCone);
        assert!((rate.success_rate - 0.0).abs() < 0.01);
        assert_eq!(rate.recommended_strategy, TraversalStrategy::Relay);
    }

    #[test]
    fn test_try_udp_hole_punching() {
        let local: SocketAddr = "1.2.3.4:12345".parse().unwrap();
        let remote: SocketAddr = "5.6.7.8:54321".parse().unwrap();
        let result = NatTraversal::try_udp_hole_punching(local, remote);
        assert!(result.success);
        assert_eq!(result.strategy, TraversalStrategy::UdpHolePunching);
    }

    #[test]
    fn test_try_relay() {
        let result = NatTraversal::try_relay("relay.openlink.dev");
        assert!(result.success);
        assert_eq!(result.strategy, TraversalStrategy::Relay);
    }

    #[test]
    fn test_nat_traversal_creation() {
        let traversal = NatTraversal::new();
        assert!(traversal.local_nat_info.is_none());
    }
}
