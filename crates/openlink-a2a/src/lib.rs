//! # OpenLink A2A — Agent-to-Agent 协议层
//!
//! Phase 6: Agent 间发现、通信和信任建立协议。
//!
//! ## 核心组件
//! - **AgentRegistry**: Agent 注册/发现/心跳
//! - **Capability**: Agent 能力声明
//! - **Handshake**: Agent 间握手和信任建立
//! - **Heartbeat**: 心跳监测和故障检测
//! - **MessageBus**: Agent 间消息通信总线

pub mod types;
pub mod registry;
pub mod handshake;
pub mod heartbeat;
pub mod message_bus;

pub use types::*;
pub use registry::AgentRegistry;
pub use handshake::HandshakeEngine;
pub use heartbeat::{HeartbeatMonitor, HeartbeatConfig};
pub use message_bus::{MessageBus, MessageBusConfig, MessageBusStats, MessageBusError, BroadcastResult};
