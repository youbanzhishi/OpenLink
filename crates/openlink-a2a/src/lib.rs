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
pub mod handshake;
pub mod heartbeat;
pub mod message_bus;
pub mod registry;
pub mod types;

// Phase 10 modules
pub mod bridge;
pub mod decentralized;
pub mod marketplace;
pub mod mcp;
pub mod negotiation;
pub mod trust;

// Phase 6 re-exports
pub use handshake::HandshakeEngine;
pub use heartbeat::{HeartbeatConfig, HeartbeatMonitor};
pub use message_bus::{
    BroadcastResult, MessageBus, MessageBusConfig, MessageBusError, MessageBusStats,
};
pub use registry::AgentRegistry;
pub use types::*;

// Phase 10 re-exports — MCP
pub use mcp::{
    McpClient, McpClientConfig, McpError, McpParser, McpProtocolError, McpRequest, McpResponse,
    McpServer, McpServerInfo, McpTool, McpTransport,
};

// Phase 10 re-exports — Marketplace
pub use marketplace::{
    AgentProfile, CapabilityType, MarketplaceError, MarketplaceQuery, MarketplaceRegistry,
    Recommendation,
};

// Phase 10 re-exports — Trust
pub use trust::{ListEntry, ListType, TrustConfig, TrustManager, TrustScore};

// Phase 10 re-exports — Negotiation
pub use negotiation::{
    AssignmentStatus, NegotiationConfig, NegotiationEngine, NegotiationError, ProposalStatus,
    TaskAssignment, TaskBid, TaskProposal,
};

// Phase 10 re-exports — Decentralized
pub use decentralized::{
    CapabilityAnnouncement, CapabilityGossip, CapabilityProvider, CapabilityRoute,
    DecentralizedCapabilityRouter, DhtKey, DhtValue, PartitionPolicy, PartitionStatus,
};

// Phase 10 re-exports — Bridge
pub use bridge::{
    A2ABridge, BridgeError, HttpBridge, McpBridge, NegotiatedProtocol, ProtocolBridge,
    ProtocolCapabilities, ProtocolGateway, ProtocolNegotiator, ProtocolType, UnifiedMessage,
};
