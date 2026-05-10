//! # OpenLink A2A — Agent-to-Agent 协议层
//!
//! Phase 6: Agent 间发现、通信和信任建立协议。
//! Phase 10: 协议层 — MCP 适配器、Agent 市场、信任系统、去中心化路由、协议桥接。
//!
//! ## 核心组件 (Phase 6)
//! - **AgentRegistry**: Agent 注册/发现/心跳
//! - **Capability**: Agent 能力声明
//! - **Handshake**: Agent 间握手和信任建立
//! - **Heartbeat**: 心跳监测和故障检测
//! - **MessageBus**: Agent 间消息通信总线
//!
//! ## 协议层组件 (Phase 10)
//! - **MCP**: Model Context Protocol 适配器（Server/Client/Parser）
//! - **Marketplace**: Agent 发现市场（注册/搜索/推荐）
//! - **Trust**: 信任与声誉系统（评分/衰减/黑白名单）
//! - **Negotiation**: 任务协商协议（发布/竞标/分配）
//! - **Decentralized**: 去中心化路由增强（DHT/能力路由/分区容错）
//! - **Bridge**: 协议桥接层（MCP/A2A/HTTP 统一接口/自动协商/格式转换）

// Phase 6 modules
pub mod types;
pub mod registry;
pub mod handshake;
pub mod heartbeat;
pub mod message_bus;

// Phase 10 modules
pub mod mcp;
pub mod marketplace;
pub mod trust;
pub mod negotiation;
pub mod decentralized;
pub mod bridge;

// Phase 6 re-exports
pub use types::*;
pub use registry::AgentRegistry;
pub use handshake::HandshakeEngine;
pub use heartbeat::{HeartbeatMonitor, HeartbeatConfig};
pub use message_bus::{MessageBus, MessageBusConfig, MessageBusStats, MessageBusError, BroadcastResult};

// Phase 10 re-exports — MCP
pub use mcp::{
    McpTransport, McpTool, McpRequest, McpResponse, McpError,
    McpServerInfo, McpClientConfig, McpParser,
    McpServer, McpClient,
    McpProtocolError,
};

// Phase 10 re-exports — Marketplace
pub use marketplace::{
    AgentProfile, MarketplaceQuery, CapabilityType,
    Recommendation, MarketplaceRegistry, MarketplaceError,
};

// Phase 10 re-exports — Trust
pub use trust::{
    TrustScore, TrustConfig, TrustManager,
    ListType, ListEntry,
};

// Phase 10 re-exports — Negotiation
pub use negotiation::{
    TaskProposal, ProposalStatus, TaskBid, TaskAssignment, AssignmentStatus,
    NegotiationConfig, NegotiationEngine, NegotiationError,
};

// Phase 10 re-exports — Decentralized
pub use decentralized::{
    DhtKey, DhtValue, CapabilityRoute, CapabilityProvider,
    CapabilityGossip, CapabilityAnnouncement,
    PartitionStatus, PartitionPolicy,
    DecentralizedCapabilityRouter,
};

// Phase 10 re-exports — Bridge
pub use bridge::{
    ProtocolType, NegotiatedProtocol, ProtocolCapabilities, UnifiedMessage,
    ProtocolBridge, A2ABridge, McpBridge, HttpBridge,
    ProtocolNegotiator, ProtocolGateway, BridgeError,
};
