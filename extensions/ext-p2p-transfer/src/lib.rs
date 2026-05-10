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
//! ## Phase 9 增强
//! - `nat_traversal`: NAT 穿透模块（UDP hole punching / TCP fallback / relay）
//! - `peer_connection`: P2P 连接管理（状态机/心跳/带宽估算/质量评分）
//! - `chunk_transfer`: 分块传输（断点续传/多源并行下载/SHA256校验）
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
pub mod nat_traversal;
pub mod peer_connection;
pub mod chunk_transfer;

pub use nat::{NatType, NatInfo};
pub use stun::StunClient;
pub use transfer::{P2pTransferAction, P2pTransferParams, TransferMode, P2pResponse};
pub use nat_traversal::{NatTraversal, TraversalStrategy, TraversalResult, TraversalSuccessRate};
pub use peer_connection::{
    PeerConnection, ConnectionState, ConnectionQuality, PeerConnectionStats,
    BandwidthEstimator,
};
pub use chunk_transfer::{
    ChunkTransferTask, ChunkInfo, ChunkState, ChunkVerifyResult,
    ParallelDownloadScheduler, DEFAULT_CHUNK_SIZE,
    calculate_chunk_count, compute_chunk_checksum,
};

use std::sync::Arc;
use openlink_core::{ExtensionRegistry, CoreError, ActionHandler, Context, ActionResult, Target};

/// 注册 P2P 传输扩展
pub fn register(registry: &mut ExtensionRegistry) -> Result<(), CoreError> {
    let action = P2pTransferAction::new();
    registry.register_action(Arc::new(action))?;
    tracing::info!("ext-p2p-transfer registered");
    Ok(())
}
