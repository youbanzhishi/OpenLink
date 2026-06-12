//! # P2P 传输实现
//!
//! 传输路由策略：直连 > P2P > 中继 > 云存储

use crate::nat::{NatInfo, NatType};
use crate::stun::StunClient;
use async_trait::async_trait;
use openlink_core::{ActionHandler, ActionResult, Context, CoreError, Target};
use serde::{Deserialize, Serialize};
use std::net::UdpSocket;

/// 传输模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
    /// 自动选择最优路径
    Auto,
    /// 强制直连（仅限 LAN）
    Direct,
    /// STUN 穿透尝试
    P2p,
    /// 中继降级
    Relay,
    /// 云存储
    Cloud,
}

impl TransferMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransferMode::Auto => "auto",
            TransferMode::Direct => "direct",
            TransferMode::P2p => "p2p",
            TransferMode::Relay => "relay",
            TransferMode::Cloud => "cloud",
        }
    }
}

/// P2P 传输参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pTransferParams {
    /// 文件 ID
    #[serde(default)]
    pub file_id: Option<String>,

    /// 传输方向
    #[serde(default = "default_direction")]
    pub direction: String,

    /// 传输模式
    #[serde(default = "default_mode")]
    pub mode: String,

    /// 目标节点 ID
    #[serde(default)]
    pub peer_node_id: Option<String>,

    /// 传输令牌（用于验证）
    #[serde(default)]
    pub transfer_token: Option<String>,

    /// 传输超时（秒）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_direction() -> String {
    "transfer".to_string()
}

fn default_mode() -> String {
    "auto".to_string()
}

fn default_timeout() -> u64 {
    300
}

impl P2pTransferParams {
    fn parse_mode(&self) -> TransferMode {
        match self.mode.as_str() {
            "direct" => TransferMode::Direct,
            "p2p" => TransferMode::P2p,
            "relay" => TransferMode::Relay,
            "cloud" => TransferMode::Cloud,
            _ => TransferMode::Auto,
        }
    }
}

/// P2P 传输响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pResponse {
    /// 响应类型
    #[serde(rename = "type")]
    pub response_type: String,

    /// 使用的传输模式
    pub mode: String,

    /// 文件 ID
    pub file_id: Option<String>,

    /// 节点信息
    pub peer: Option<PeerInfo>,

    /// 是否降级
    pub fallback: bool,

    /// 降级原因
    pub fallback_reason: Option<String>,

    /// 预估速度 (Mbps)
    pub estimated_speed_mbps: Option<f64>,

    /// 传输 URL 或指令
    pub transfer_url: Option<String>,

    /// NAT 信息
    pub nat_info: Option<NatInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub public_ip: String,
    pub public_port: u16,
    pub local_ip: String,
    pub local_port: u16,
}

/// P2P 传输 Action
pub struct P2pTransferAction {
    stun_client: StunClient,
    nat_info: Option<NatInfo>,
}

impl P2pTransferAction {
    pub fn new() -> Self {
        Self {
            stun_client: StunClient::new(),
            nat_info: None,
        }
    }

    fn parse_params(params: &serde_json::Value) -> Result<P2pTransferParams, CoreError> {
        serde_json::from_value(params.clone())
            .map_err(|e| CoreError::ExtensionError(format!("Invalid P2P transfer params: {}", e)))
    }

    /// 获取/检测 NAT 信息
    fn get_nat_info(&mut self) -> NatInfo {
        if let Some(ref info) = self.nat_info {
            return info.clone();
        }

        // 创建 UDP socket 用于检测
        let socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(_) => {
                return NatInfo::unknown("0.0.0.0", 0);
            }
        };

        let info = self.stun_client.detect_nat_type(&socket);
        self.nat_info = Some(info.clone());
        info
    }

    /// 选择传输路径
    fn select_transfer_path(&self, params: &P2pTransferParams) -> P2pResponse {
        let mode = params.parse_mode();

        match mode {
            TransferMode::Direct => {
                // 直连模式：假设 peer 在同一 LAN
                P2pResponse {
                    response_type: "direct_transfer".to_string(),
                    mode: "direct".to_string(),
                    file_id: params.file_id.clone(),
                    peer: params.peer_node_id.as_ref().map(|id| PeerInfo {
                        node_id: id.clone(),
                        public_ip: "192.168.x.x".to_string(),
                        public_port: 0,
                        local_ip: "192.168.x.x".to_string(),
                        local_port: 0,
                    }),
                    fallback: false,
                    fallback_reason: None,
                    estimated_speed_mbps: Some(1000.0),
                    transfer_url: Some(format!(
                        "udp://peer-lan-address:{}",
                        params.peer_node_id.as_deref().unwrap_or("")
                    )),
                    nat_info: None,
                }
            }
            TransferMode::P2p => {
                // P2P 模式：需要 NAT 信息
                P2pResponse {
                    response_type: "p2p_transfer".to_string(),
                    mode: "p2p".to_string(),
                    file_id: params.file_id.clone(),
                    peer: params.peer_node_id.as_ref().map(|id| PeerInfo {
                        node_id: id.clone(),
                        public_ip: "awaiting-disco".to_string(),
                        public_port: 0,
                        local_ip: "awaiting-disco".to_string(),
                        local_port: 0,
                    }),
                    fallback: false,
                    fallback_reason: None,
                    estimated_speed_mbps: Some(100.0),
                    transfer_url: Some("p2p://initiate-hole-punching".to_string()),
                    nat_info: None,
                }
            }
            TransferMode::Relay => {
                // 中继模式
                P2pResponse {
                    response_type: "relay_transfer".to_string(),
                    mode: "relay".to_string(),
                    file_id: params.file_id.clone(),
                    peer: None,
                    fallback: false,
                    fallback_reason: None,
                    estimated_speed_mbps: Some(10.0),
                    transfer_url: Some(format!(
                        "https://relay.openlink.dev/transfer/{}",
                        params.file_id.as_deref().unwrap_or("")
                    )),
                    nat_info: None,
                }
            }
            TransferMode::Cloud => {
                // 云存储模式
                P2pResponse {
                    response_type: "cloud_transfer".to_string(),
                    mode: "cloud".to_string(),
                    file_id: params.file_id.clone(),
                    peer: None,
                    fallback: false,
                    fallback_reason: None,
                    estimated_speed_mbps: Some(50.0),
                    transfer_url: Some(format!(
                        "https://api.openlink.dev/api/v1/files/transfer/{}",
                        params.file_id.as_deref().unwrap_or("")
                    )),
                    nat_info: None,
                }
            }
            TransferMode::Auto => {
                // 自动模式：返回需要进一步探测的响应
                P2pResponse {
                    response_type: "auto_transfer".to_string(),
                    mode: "auto".to_string(),
                    file_id: params.file_id.clone(),
                    peer: None,
                    fallback: false,
                    fallback_reason: None,
                    estimated_speed_mbps: None,
                    transfer_url: Some("auto://requires-nat-detection".to_string()),
                    nat_info: None,
                }
            }
        }
    }
}

impl Default for P2pTransferAction {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ActionHandler for P2pTransferAction {
    async fn execute(&self, _ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        let params = Self::parse_params(&target.params)?;

        tracing::info!(
            file_id = ?params.file_id,
            direction = %params.direction,
            mode = %params.mode,
            peer = ?params.peer_node_id,
            "P2P transfer action"
        );

        let mode = params.parse_mode();

        // 根据模式生成响应
        let response = match mode {
            TransferMode::Auto => {
                // 自动模式：检测 NAT 并选择最优路径
                let mut action = Self::new(); // 创建新的以避免缓存问题
                let nat_info = action.get_nat_info();

                if nat_info.nat_type == NatType::Open {
                    P2pResponse {
                        response_type: "direct_transfer".to_string(),
                        mode: "direct".to_string(),
                        file_id: params.file_id.clone(),
                        peer: params.peer_node_id.as_ref().map(|id| PeerInfo {
                            node_id: id.clone(),
                            public_ip: nat_info.public_ip.clone().unwrap_or_default(),
                            public_port: nat_info.public_port.unwrap_or(0),
                            local_ip: nat_info.local_ip.clone(),
                            local_port: nat_info.local_port,
                        }),
                        fallback: false,
                        fallback_reason: None,
                        estimated_speed_mbps: Some(1000.0),
                        transfer_url: Some("udp://direct".to_string()),
                        nat_info: Some(nat_info),
                    }
                } else if nat_info.nat_type.is_traversable() {
                    P2pResponse {
                        response_type: "p2p_transfer".to_string(),
                        mode: "p2p".to_string(),
                        file_id: params.file_id.clone(),
                        peer: params.peer_node_id.as_ref().map(|id| PeerInfo {
                            node_id: id.clone(),
                            public_ip: nat_info.public_ip.clone().unwrap_or_default(),
                            public_port: nat_info.public_port.unwrap_or(0),
                            local_ip: nat_info.local_ip.clone(),
                            local_port: nat_info.local_port,
                        }),
                        fallback: false,
                        fallback_reason: None,
                        estimated_speed_mbps: Some(100.0),
                        transfer_url: Some("p2p://hole-punching".to_string()),
                        nat_info: Some(nat_info),
                    }
                } else {
                    // 对称型 NAT，需要中继
                    P2pResponse {
                        response_type: "relay_transfer".to_string(),
                        mode: "relay".to_string(),
                        file_id: params.file_id.clone(),
                        peer: None,
                        fallback: true,
                        fallback_reason: Some("Symmetric NAT detected, using relay".to_string()),
                        estimated_speed_mbps: Some(10.0),
                        transfer_url: Some(format!(
                            "https://relay.openlink.dev/transfer/{}",
                            params.file_id.as_deref().unwrap_or("")
                        )),
                        nat_info: Some(nat_info),
                    }
                }
            }
            _ => {
                // 预定义的传输模式
                let mut action = Self::new();
                let nat_info = action.get_nat_info();
                let mut resp = self.select_transfer_path(&params);
                resp.nat_info = Some(nat_info);
                resp
            }
        };

        Ok(ActionResult::Json(serde_json::to_value(&response).unwrap()))
    }

    fn name(&self) -> &str {
        "p2p_transfer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mode() {
        let params = P2pTransferParams {
            file_id: None,
            direction: "transfer".into(),
            mode: "p2p".into(),
            peer_node_id: None,
            transfer_token: None,
            timeout_secs: 300,
        };
        assert_eq!(params.parse_mode(), TransferMode::P2p);
    }

    #[test]
    fn test_parse_mode_auto() {
        let params = P2pTransferParams {
            file_id: None,
            direction: "transfer".into(),
            mode: "auto".into(),
            peer_node_id: None,
            transfer_token: None,
            timeout_secs: 300,
        };
        assert_eq!(params.parse_mode(), TransferMode::Auto);
    }

    #[test]
    fn test_transfer_action_creation() {
        let action = P2pTransferAction::new();
        assert_eq!(action.name(), "p2p_transfer");
    }
}
