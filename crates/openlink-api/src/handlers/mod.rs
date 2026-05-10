//! # 请求处理器
//!
//! 各 API 端点的处理逻辑。

pub mod link;
pub mod route;
pub mod extension;
pub mod redirect;
pub mod stats;
pub mod agent; // Phase 3: Agent API
pub mod monitoring; // Phase 5: 健康检查
pub mod plugin; // Phase 8: Plugin & Share API
pub mod p2p; // Phase 9: P2P API
pub mod edge; // Phase 9: Edge API
