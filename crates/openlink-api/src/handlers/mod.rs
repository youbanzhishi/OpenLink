//! # 请求处理器
//!
//! 各 API 端点的处理逻辑。

pub mod agent; // Phase 3: Agent API
pub mod card; // Phase 3.5: Identity Card 名片
pub mod edge;
pub mod extension;
pub mod link;
pub mod monitoring; // Phase 5: 健康检查
pub mod p2p; // Phase 9: P2P API
pub mod plugin; // Phase 8: Plugin & Share API
pub mod redirect;
pub mod route;
pub mod stats; // Phase 9: Edge API
