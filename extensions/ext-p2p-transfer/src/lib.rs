//! # ext-p2p-transfer — P2P 传输扩展
//!
//! 实现 NAT 穿透让异地设备直连，无需中继服务器。
//!
//! ## 传输路由策略
//! - `direct`: 直连（仅限同一LAN）
//! - `p2p`: STUN穿透尝试 UDP 打洞
//! - `relay`: TURN 中继降级（需要 TURN 服务器）
//! - `auto`（默认）: 直连 > P2P > 中继 > 云存储
//!
//! ## Action 参数格式
//! ```json
//! {
//!   "file_id": "abc123",
//!   "direction": "push|pull",
//!   "mode": "auto",
//!   "peer_node_id": "target-node-id",
//!   "transfer_token": "optional-token"
//! }
//! ```

pub mod stun;
pub mod nat;
pub mod transfer;

pub use nat::{NatType, NatInfo};
pub use stun::StunClient;
pub use transfer::{P2pTransferAction, P2pTransferParams, TransferMode, P2pResponse};

use std::sync::Arc;
use openlink_core::{ExtensionRegistry, CoreError, ActionHandler, Context, ActionResult, Target};

/// 注册 P2P 传输扩展
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    let action = P2pTransferAction::new();
    registry.register_action(Arc::new(action))?;
    tracing::info!("ext-p2p-transfer registered");
    Ok(())
}
