//! # ext-direct-transfer — 局域网直传 Action 扩展
//!
//! 当检测到同 LAN 的 OpenLink 节点时，文件直接 P2P 传输，不走云端。
//!
//! 传输路由策略：
//! - `lan_first`（默认）：优先 LAN 直传，无节点则走云中转
//! - `force_lan`：强制 LAN，节点不可达则失败
//! - `force_cloud`：强制云中转（测试用）
//!
//! ## Action 参数格式
//! ```json
//! {
//!   "file_id": "abc123",
//!   "direction": "push",
//!   "mode": "lan_first",
//!   "encrypted": true,
//!   "target_node_id": "optional-specific-node"
//! }
//! ```

pub mod discovery;
pub mod transfer;

pub use discovery::{LanDiscovery, LanPeer};

// ─── Re-exports ─────────────────────────────────────────────

use openlink_core::{CoreError, ExtensionRegistry};
use std::sync::Arc;

/// 注册直传扩展到 Extension Registry
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    // 注册 direct_transfer action
    let action = DirectTransferAction::new();
    registry.register_action(Arc::new(action))?;

    // 注册 lan_peer condition（检测请求是否来自同 LAN）
    let condition = LanPeerCondition;
    registry.register_condition(Arc::new(condition))?;

    tracing::info!("ext-direct-transfer registered");
    Ok(())
}

use async_trait::async_trait;
use openlink_core::{ActionHandler, ActionResult, ConditionHandler, Context, Target};
use serde::{Deserialize, Serialize};

// ─── Transfer 参数 ───────────────────────────────────────────

/// 直传参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectTransferParams {
    /// 文件 ID
    #[serde(default)]
    pub file_id: Option<String>,

    /// 传输方向：push / pull / transfer
    #[serde(default = "default_direction")]
    pub direction: String,

    /// 传输模式：lan_first / force_lan / force_cloud
    #[serde(default = "default_mode")]
    pub mode: String,

    /// 是否加密传输
    #[serde(default = "default_encrypted")]
    pub encrypted: bool,

    /// 目标节点 ID（可选，指定则只用该节点）
    #[serde(default)]
    pub target_node_id: Option<String>,

    /// 分享码（用于 pull 模式）
    #[serde(default)]
    pub share_code: Option<String>,

    /// 传输超时（秒）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_direction() -> String {
    "transfer".to_string()
}

fn default_mode() -> String {
    "lan_first".to_string()
}

fn default_encrypted() -> bool {
    true
}

fn default_timeout() -> u64 {
    300
}

// ─── Transfer 响应 ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResponse {
    #[serde(rename = "type")]
    pub response_type: String,
    pub file_id: Option<String>,
    pub direction: String,
    pub mode: String,
    pub peer: Option<PeerInfo>,
    pub cloud_fallback: bool,
    pub estimated_speed_mbps: Option<f64>,
    pub transfer_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub ip: String,
    pub port: u16,
    pub latency_ms: Option<u32>,
}

// ─── DirectTransfer Action ──────────────────────────────────

/// 局域网直传 Action
pub struct DirectTransferAction {
    discovery: LanDiscovery,
}

impl DirectTransferAction {
    pub fn new() -> Self {
        Self {
            discovery: LanDiscovery::new(),
        }
    }

    fn parse_params(params: &serde_json::Value) -> Result<DirectTransferParams, CoreError> {
        serde_json::from_value(params.clone())
            .map_err(|e| CoreError::ExtensionError(format!("Invalid direct transfer params: {}", e)))
    }

    /// 执行 LAN 发现
    async fn discover_peers(&self) -> Vec<LanPeer> {
        self.discovery.discover_peers().await
    }

    /// 选择最优节点
    fn select_best_peer<'a>(params: &DirectTransferParams, peers: &'a [LanPeer]) -> Option<&'a LanPeer> {
        if let Some(ref target_id) = params.target_node_id {
            peers.iter().find(|p| p.node_id.as_str() == target_id)
        } else {
            peers.iter().min_by_key(|p| p.latency_ms.unwrap_or(u32::MAX))
        }
    }

    /// 构建直传响应
    #[allow(dead_code)]
    fn build_response(params: &DirectTransferParams, peer: Option<&LanPeer>, cloud_fallback: bool) -> TransferResponse {
        match peer {
            Some(p) => TransferResponse {
                response_type: "lan_direct_transfer".to_string(),
                file_id: params.file_id.clone(),
                direction: params.direction.clone(),
                mode: "lan".to_string(),
                peer: Some(PeerInfo {
                    node_id: p.node_id.clone(),
                    ip: p.ip.clone(),
                    port: p.port,
                    latency_ms: p.latency_ms,
                }),
                cloud_fallback: false,
                estimated_speed_mbps: Some(100.0), // LAN 典型速度
                transfer_url: Some(format!(
                    "http://{}:{}/openlink/files/{}",
                    p.ip,
                    p.port,
                    params.file_id.as_deref().unwrap_or("")
                )),
            },
            None => TransferResponse {
                response_type: if cloud_fallback {
                    "cloud_transfer"
                } else {
                    "transfer_unavailable"
                }
                .to_string(),
                file_id: params.file_id.clone(),
                direction: params.direction.clone(),
                mode: params.mode.clone(),
                peer: None,
                cloud_fallback,
                estimated_speed_mbps: None,
                transfer_url: None,
            },
        }
    }

    /// 执行传输
    async fn execute_transfer(
        &self,
        params: &DirectTransferParams,
        peer: Option<&LanPeer>,
    ) -> Result<ActionResult, CoreError> {
        // 对于 push 模式，向目标节点发送文件
        // 对于 pull 模式，从目标节点拉取文件
        // 这里返回传输信息，实际传输由节点间 HTTP 调用完成

        if let Some(p) = peer {
            // P2P 传输：构建节点间传输协议
            let transfer_token = generate_transfer_token();

            tracing::info!(
                peer = %p.node_id,
                ip = %p.ip,
                direction = %params.direction,
                "Initiating LAN P2P transfer"
            );

            Ok(ActionResult::Custom {
                content_type: "application/json".to_string(),
                body: serde_json::json!({
                    "type": "lan_transfer_initiated",
                    "peer": {
                        "node_id": p.node_id,
                        "ip": p.ip,
                        "port": p.port,
                    },
                    "transfer_token": transfer_token,
                    "direction": params.direction,
                    "file_id": params.file_id,
                    "encrypted": params.encrypted,
                    "estimated_speed_mbps": 100.0,
                })
                .to_string(),
            })
        } else {
            Err(CoreError::ExtensionError(
                "No LAN peer available and cloud fallback disabled".to_string(),
            ))
        }
    }
}

impl Default for DirectTransferAction {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ActionHandler for DirectTransferAction {
    async fn execute(&self, _ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        let params = Self::parse_params(&target.params)?;

        tracing::info!(
            file_id = ?params.file_id,
            direction = %params.direction,
            mode = %params.mode,
            "DirectTransfer action"
        );

        // 阶段 1：发现 LAN 节点
        let peers = self.discover_peers().await;

        // 阶段 2：选择最优传输路径
        match params.mode.as_str() {
            "force_cloud" => {
                return Ok(ActionResult::Json(
                    serde_json::to_value(TransferResponse {
                        response_type: "cloud_transfer".to_string(),
                        file_id: params.file_id.clone(),
                        direction: params.direction.clone(),
                        mode: "cloud".to_string(),
                        peer: None,
                        cloud_fallback: false,
                        estimated_speed_mbps: None,
                        transfer_url: None,
                    })
                    .unwrap(),
                ));
            }
            "force_lan" => {
                let peer = Self::select_best_peer(&params, &peers);
                if peer.is_none() && peers.is_empty() {
                    return Err(CoreError::ExtensionError(
                        "No LAN peer available (force_lan mode)".to_string(),
                    ));
                }
                self.execute_transfer(&params, peer).await
            }
            _ => {
                // lan_first：优先 LAN，无节点则云
                let peer = Self::select_best_peer(&params, &peers);
                if let Some(p) = peer {
                    self.execute_transfer(&params, Some(p)).await
                } else {
                    // 云中转
                    tracing::info!("No LAN peer, falling back to cloud transfer");
                    Ok(ActionResult::Json(
                        serde_json::to_value(TransferResponse {
                            response_type: "cloud_transfer".to_string(),
                            file_id: params.file_id.clone(),
                            direction: params.direction.clone(),
                            mode: "cloud".to_string(),
                            peer: None,
                            cloud_fallback: true,
                            estimated_speed_mbps: None,
                            transfer_url: Some(format!(
                                "https://api.openlink.dev/api/v1/files/transfer/{}",
                                params.file_id.as_deref().unwrap_or("")
                            )),
                        })
                        .unwrap(),
                    ))
                }
            }
        }
    }

    fn name(&self) -> &str {
        "direct_transfer"
    }
}

/// 生成传输令牌
fn generate_transfer_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.gen()).collect();
    hex::encode(&bytes)
}

// ─── LAN Peer Condition ─────────────────────────────────────

/// LAN Peer 条件处理器
pub struct LanPeerCondition;

#[async_trait]
impl ConditionHandler for LanPeerCondition {
    async fn evaluate(&self, ctx: &Context, params: &serde_json::Value) -> Result<bool, CoreError> {
        // 检查请求是否标记为来自 LAN peer
        let lan_peer_id = ctx.custom.get("lan_peer_node_id").and_then(|v| v.as_str());

        let require_encrypted = params
            .get("require_encrypted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let has_lan_peer = lan_peer_id.is_some();

        // 如果要求加密但节点不支持，跳过
        if has_lan_peer && require_encrypted {
            let supports_encryption = ctx
                .custom
                .get("lan_peer_encrypted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            return Ok(supports_encryption);
        }

        Ok(has_lan_peer)
    }

    fn name(&self) -> &str {
        "lan_peer"
    }
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_transfer_params() {
        let params = serde_json::json!({
            "file_id": "file-abc",
            "direction": "push",
            "mode": "lan_first",
            "encrypted": true,
            "timeout_secs": 300
        });
        let parsed: DirectTransferParams = serde_json::from_value(params).unwrap();
        assert_eq!(parsed.file_id.as_deref(), Some("file-abc"));
        assert_eq!(parsed.direction, "push");
        assert_eq!(parsed.mode, "lan_first");
        assert!(parsed.encrypted);
        assert_eq!(parsed.timeout_secs, 300);
    }

    #[test]
    fn test_transfer_params_defaults() {
        let params = serde_json::json!({});
        let parsed: DirectTransferParams = serde_json::from_value(params).unwrap();
        assert_eq!(parsed.direction, "transfer");
        assert_eq!(parsed.mode, "lan_first");
        assert!(parsed.encrypted);
        assert_eq!(parsed.timeout_secs, 300);
    }

    #[test]
    fn test_select_best_peer_by_latency() {
        let peers = vec![
            LanPeer {
                node_id: "a".into(),
                ip: "192.168.1.10".into(),
                port: 8080,
                latency_ms: Some(10),
                supports_encryption: true,
            },
            LanPeer {
                node_id: "b".into(),
                ip: "192.168.1.11".into(),
                port: 8080,
                latency_ms: Some(5),
                supports_encryption: true,
            },
            LanPeer {
                node_id: "c".into(),
                ip: "192.168.1.12".into(),
                port: 8080,
                latency_ms: Some(20),
                supports_encryption: false,
            },
        ];

        let params = DirectTransferParams {
            file_id: None,
            direction: "transfer".into(),
            mode: "lan_first".into(),
            encrypted: true,
            target_node_id: None,
            share_code: None,
            timeout_secs: 300,
        };

        let best = DirectTransferAction::select_best_peer(&params, &peers);
        assert!(best.is_some());
        assert_eq!(best.unwrap().node_id, "b");
    }

    #[test]
    fn test_select_specific_peer() {
        let peers = vec![
            LanPeer {
                node_id: "a".into(),
                ip: "192.168.1.10".into(),
                port: 8080,
                latency_ms: Some(10),
                supports_encryption: true,
            },
            LanPeer {
                node_id: "b".into(),
                ip: "192.168.1.11".into(),
                port: 8080,
                latency_ms: Some(5),
                supports_encryption: true,
            },
        ];

        let params = DirectTransferParams {
            file_id: None,
            direction: "transfer".into(),
            mode: "lan_first".into(),
            encrypted: true,
            target_node_id: Some("a".into()),
            share_code: None,
            timeout_secs: 300,
        };

        let best = DirectTransferAction::select_best_peer(&params, &peers);
        assert!(best.is_some());
        assert_eq!(best.unwrap().node_id, "a");
    }

    #[test]
    fn test_build_lan_response() {
        let params = DirectTransferParams {
            file_id: Some("file-123".into()),
            direction: "push".into(),
            mode: "lan_first".into(),
            encrypted: true,
            target_node_id: None,
            share_code: None,
            timeout_secs: 300,
        };

        let peer = LanPeer {
            node_id: "node-x".into(),
            ip: "192.168.1.50".into(),
            port: 9090,
            latency_ms: Some(3),
            supports_encryption: true,
        };

        let resp = DirectTransferAction::build_response(&params, Some(&peer), false);
        assert_eq!(resp.response_type, "lan_direct_transfer");
        assert!(resp.peer.is_some());
        assert_eq!(resp.peer.as_ref().unwrap().node_id, "node-x");
        assert_eq!(resp.estimated_speed_mbps, Some(100.0));
    }

    #[test]
    fn test_generate_transfer_token() {
        let token1 = generate_transfer_token();
        let token2 = generate_transfer_token();
        assert_eq!(token1.len(), 32); // 16 bytes = 32 hex chars
        assert_ne!(token1, token2);
    }
}
