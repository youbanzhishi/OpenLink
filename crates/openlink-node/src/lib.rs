//! # openlink-node — 轻量设备端守护进程
//!
//! 设备端守护进程，实现以下功能：
//! - **mDNS 广播**：设备自动发现同 LAN 的 OpenLink 节点
//! - **文件服务**：暴露 HTTP 文件上传/下载端点
//! - **心跳上报**：定期向 OpenLink Server 上报设备状态
//!
//! 设计原则：
//! - 核心层零业务逻辑，所有功能作为扩展注册
//! - 设备节点是 OpenLink 网络的边缘节点

pub mod config;
pub mod discovery;
pub mod file_service;
pub mod heartbeat;

pub use config::NodeConfig;
pub use discovery::{DiscoveredNode, NodeDiscovery};
pub use file_service::{FileRequest, FileServer};
pub use heartbeat::{HeartbeatClient, NodeStatus};

// ─── Re-exports ──────────────────────────────────────────────

/// 注册 Node 扩展到 Extension Registry
pub fn register(registry: &mut openlink_core::ExtensionRegistry) -> Result<(), openlink_core::CoreError> {
    use std::sync::Arc;

    // 注册 direct_transfer action（节点间的 P2P 文件传输）
    let direct_transfer = ext_direct_transfer();
    registry.register_action(Arc::new(direct_transfer))?;

    // 注册 lan_peers condition（检测是否在同 LAN）
    let lan_condition = LanConditionHandler;
    registry.register_condition(Arc::new(lan_condition))?;

    Ok(())
}

// Re-export the direct transfer action (lazy to avoid circular deps)
fn ext_direct_transfer() -> impl openlink_core::ActionHandler {
    DirectTransferAction::new()
}

use async_trait::async_trait;
use openlink_core::{ActionHandler, ActionResult, Context, CoreError, Target};
use serde::{Deserialize, Serialize};

/// DirectTransfer Action — 局域网直传
///
/// 检测到同 LAN 节点时，文件直接 P2P 传输，不走云端。
/// 传输路由：直传 > 云中转
pub struct DirectTransferAction;

#[allow(clippy::new_without_default)]
impl DirectTransferAction {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectTransferParams {
    /// 文件 ID
    #[serde(default)]
    pub file_id: Option<String>,
    /// 目标节点 ID
    #[serde(default)]
    pub target_node_id: Option<String>,
    /// 操作：push / pull / transfer
    #[serde(default = "default_direction")]
    pub direction: String,
    /// 传输模式：lan_first / force_lan / force_cloud
    #[serde(default = "default_mode")]
    pub mode: String,
    /// 是否启用加密
    #[serde(default = "default_encrypted")]
    pub encrypted: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPeer {
    pub node_id: String,
    pub ip: String,
    pub port: u16,
    pub latency_ms: Option<u32>,
    pub supports_encryption: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRoute {
    pub mode: String,
    pub peer: Option<TransferPeer>,
    pub cloud_fallback: bool,
    pub estimated_speed_mbps: Option<f64>,
}

#[async_trait]
impl ActionHandler for DirectTransferAction {
    async fn execute(&self, _ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        let params: DirectTransferParams = serde_json::from_value(target.params.clone())
            .map_err(|e| CoreError::ExtensionError(format!("Invalid direct transfer params: {}", e)))?;

        tracing::info!(
            file_id = ?params.file_id,
            direction = %params.direction,
            mode = %params.mode,
            "DirectTransfer action"
        );

        // 步骤 1：发现同 LAN 节点
        let peers = discover_lan_peers().await;

        // 步骤 2：选择最优传输路径
        let route = select_transfer_route(&params, &peers).await;

        // 步骤 3：执行传输
        match route {
            TransferRoute { peer: Some(p), .. } => {
                // LAN 直传
                tracing::info!(peer = %p.node_id, ip = %p.ip, "LAN direct transfer");
                Ok(ActionResult::Json(serde_json::json!({
                    "type": "lan_direct_transfer",
                    "peer": {
                        "node_id": p.node_id,
                        "ip": p.ip,
                        "port": p.port,
                    },
                    "file_id": params.file_id,
                    "direction": params.direction,
                    "encrypted": params.encrypted,
                })))
            }
            _ => {
                // 云中转
                tracing::info!("No LAN peer available, falling back to cloud");
                Ok(ActionResult::Json(serde_json::json!({
                    "type": "cloud_transfer",
                    "file_id": params.file_id,
                    "cloud_fallback": true,
                    "reason": "no_lan_peer",
                })))
            }
        }
    }

    fn name(&self) -> &str {
        "direct_transfer"
    }
}

/// 发现同 LAN 的 OpenLink 节点
async fn discover_lan_peers() -> Vec<TransferPeer> {
    // 实际实现：使用 dns_sd 库进行 mDNS 发现
    // 这里返回模拟数据，生产环境替换为真实 mDNS 查询
    Vec::new()
}

/// 根据参数和可用节点选择最优传输路径
async fn select_transfer_route(params: &DirectTransferParams, peers: &[TransferPeer]) -> TransferRoute {
    match params.mode.as_str() {
        "force_lan" => {
            // 强制 LAN，查找最近节点
            let best = peers.iter().min_by_key(|p| p.latency_ms.unwrap_or(u32::MAX));
            TransferRoute {
                mode: "lan".to_string(),
                peer: best.cloned(),
                cloud_fallback: false,
                estimated_speed_mbps: best.map(|_| 100.0),
            }
        }
        "force_cloud" => TransferRoute {
            mode: "cloud".to_string(),
            peer: None,
            cloud_fallback: false,
            estimated_speed_mbps: None,
        },
        _ => {
            // lan_first: 优先 LAN，无节点则云
            let best = peers.iter().min_by_key(|p| p.latency_ms.unwrap_or(u32::MAX));
            TransferRoute {
                mode: "lan_first".to_string(),
                peer: best.cloned(),
                cloud_fallback: best.is_none(),
                estimated_speed_mbps: best.map(|_| 100.0),
            }
        }
    }
}

/// LAN peers condition handler — 检测请求是否来自同 LAN
pub struct LanConditionHandler;

#[async_trait]
impl openlink_core::ConditionHandler for LanConditionHandler {
    async fn evaluate(&self, ctx: &Context, params: &serde_json::Value) -> Result<bool, CoreError> {
        // 检查请求是否来自已知的 LAN 节点
        let _require_encrypted = params
            .get("require_encrypted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 从 context.custom 中读取 LAN peer 信息
        let has_lan_peer = ctx.custom.get("lan_peer_node_id").and_then(|v| v.as_str()).is_some();

        Ok(has_lan_peer)
    }

    fn name(&self) -> &str {
        "lan_peer"
    }
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_direct_transfer_params() {
        let params = serde_json::json!({
            "file_id": "file-123",
            "direction": "push",
            "mode": "lan_first",
            "encrypted": true
        });
        let parsed: DirectTransferParams = serde_json::from_value(params).unwrap();
        assert_eq!(parsed.file_id.as_deref(), Some("file-123"));
        assert_eq!(parsed.direction, "push");
        assert_eq!(parsed.mode, "lan_first");
        assert!(parsed.encrypted);
    }

    #[test]
    fn test_direct_transfer_params_defaults() {
        let params = serde_json::json!({});
        let parsed: DirectTransferParams = serde_json::from_value(params).unwrap();
        assert_eq!(parsed.direction, "transfer");
        assert_eq!(parsed.mode, "lan_first");
        assert!(parsed.encrypted);
    }

    #[tokio::test]
    async fn test_select_route_lan_first() {
        let params = DirectTransferParams {
            file_id: Some("file-123".to_string()),
            target_node_id: None,
            direction: "transfer".to_string(),
            mode: "lan_first".to_string(),
            encrypted: true,
        };
        let peers = vec![
            TransferPeer {
                node_id: "node-a".to_string(),
                ip: "192.168.1.10".to_string(),
                port: 8080,
                latency_ms: Some(5),
                supports_encryption: true,
            },
            TransferPeer {
                node_id: "node-b".to_string(),
                ip: "192.168.1.11".to_string(),
                port: 8080,
                latency_ms: Some(10),
                supports_encryption: true,
            },
        ];
        let route = select_transfer_route(&params, &peers).await;
        assert_eq!(route.mode, "lan_first");
        assert!(route.peer.is_some());
        assert_eq!(route.peer.as_ref().unwrap().node_id, "node-a");
        assert!(!route.cloud_fallback);
    }

    #[tokio::test]
    async fn test_select_route_no_peers() {
        let params = DirectTransferParams {
            file_id: Some("file-123".to_string()),
            target_node_id: None,
            direction: "transfer".to_string(),
            mode: "lan_first".to_string(),
            encrypted: true,
        };
        let route = select_transfer_route(&params, &[]).await;
        assert_eq!(route.mode, "lan_first");
        assert!(route.peer.is_none());
        assert!(route.cloud_fallback);
    }

    #[tokio::test]
    async fn test_select_route_force_cloud() {
        let params = DirectTransferParams {
            file_id: Some("file-123".to_string()),
            target_node_id: None,
            direction: "transfer".to_string(),
            mode: "force_cloud".to_string(),
            encrypted: false,
        };
        let peers = vec![TransferPeer {
            node_id: "node-a".to_string(),
            ip: "192.168.1.10".to_string(),
            port: 8080,
            latency_ms: Some(5),
            supports_encryption: true,
        }];
        let route = select_transfer_route(&params, &peers).await;
        assert_eq!(route.mode, "cloud");
        assert!(route.peer.is_none());
        assert!(!route.cloud_fallback);
    }
}
