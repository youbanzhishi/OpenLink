//! # NAT 类型检测
//!
//! 检测本地网络的 NAT 类型，用于判断 P2P 连通性。

use serde::{Deserialize, Serialize};

/// NAT 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatType {
    /// 完全开放（公网 IP，无 NAT）
    Open,
    /// 全锥型 NAT
    FullCone,
    /// 限制性锥型 NAT
    RestrictedCone,
    /// 端口限制性锥型 NAT
    PortRestrictedCone,
    /// 对称型 NAT（最难穿透）
    Symmetric,
}

impl NatType {
    /// 判断是否能穿透
    pub fn is_traversable(&self) -> bool {
        match self {
            NatType::Open => true,
            NatType::FullCone => true,
            NatType::RestrictedCone => true,
            NatType::PortRestrictedCone => true,
            NatType::Symmetric => false, // 对称型 NAT 需要中继
        }
    }

    /// 穿透难度评分（越低越容易）
    pub fn difficulty_score(&self) -> u8 {
        match self {
            NatType::Open => 0,
            NatType::FullCone => 1,
            NatType::RestrictedCone => 2,
            NatType::PortRestrictedCone => 3,
            NatType::Symmetric => 10, // 需要中继
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            NatType::Open => "open",
            NatType::FullCone => "full_cone",
            NatType::RestrictedCone => "restricted_cone",
            NatType::PortRestrictedCone => "port_restricted_cone",
            NatType::Symmetric => "symmetric",
        }
    }
}

impl std::fmt::Display for NatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// NAT 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatInfo {
    /// NAT 类型
    pub nat_type: NatType,
    /// 本地 IP
    pub local_ip: String,
    /// 本地端口
    pub local_port: u16,
    /// 公网 IP（STUN 获取）
    pub public_ip: Option<String>,
    /// 公网端口
    pub public_port: Option<u16>,
    /// 检测是否完成
    pub is_complete: bool,
}

impl NatInfo {
    /// 创建默认的 NAT 信息（假设开放）
    pub fn default_open(local_ip: &str, local_port: u16) -> Self {
        Self {
            nat_type: NatType::Open,
            local_ip: local_ip.to_string(),
            local_port,
            public_ip: Some(local_ip.to_string()),
            public_port: Some(local_port),
            is_complete: true,
        }
    }

    /// 创建未检测的 NAT 信息
    pub fn unknown(local_ip: &str, local_port: u16) -> Self {
        Self {
            nat_type: NatType::Symmetric, // 保守假设
            local_ip: local_ip.to_string(),
            local_port,
            public_ip: None,
            public_port: None,
            is_complete: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nat_traversable() {
        assert!(NatType::Open.is_traversable());
        assert!(NatType::FullCone.is_traversable());
        assert!(NatType::Symmetric.is_traversable() == false);
    }

    #[test]
    fn test_difficulty_score() {
        assert_eq!(NatType::Open.difficulty_score(), 0);
        assert_eq!(NatType::Symmetric.difficulty_score(), 10);
    }
}
