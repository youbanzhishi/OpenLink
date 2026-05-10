//! # A2A 类型定义
//!
//! Agent-to-Agent 协议的核心数据类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent 全局唯一标识
pub type AgentId = String;

/// Agent 元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent 唯一标识
    pub id: AgentId,
    /// Agent 名称
    pub name: String,
    /// Agent 描述
    #[serde(default)]
    pub description: String,
    /// Agent 版本
    pub version: String,
    /// Agent 端点地址（用于通信）
    pub endpoint: String,
    /// 能力声明列表
    pub capabilities: Vec<Capability>,
    /// 自定义元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// 注册时间
    pub registered_at: i64,
    /// 最后心跳时间
    pub last_heartbeat: i64,
    /// Agent 状态
    pub status: AgentStatus,
}

/// Agent 状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// 在线
    Online,
    /// 离线
    Offline,
    /// 忙碌（正在执行任务）
    Busy,
    /// 未知
    Unknown,
}

/// Agent 能力声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// 能力标识（如 "text-generation", "image-analysis"）
    pub id: String,
    /// 能力名称
    pub name: String,
    /// 能力描述
    #[serde(default)]
    pub description: String,
    /// 输入格式（MIME type 或自定义格式）
    #[serde(default)]
    pub input_format: String,
    /// 输出格式
    #[serde(default)]
    pub output_format: String,
    /// 能力参数（如模型配置、限制等）
    #[serde(default)]
    pub params: serde_json::Value,
}

/// 握手请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    /// 请求方 Agent ID
    pub from_agent: AgentId,
    /// 目标 Agent ID
    pub to_agent: AgentId,
    /// 请求方支持的能力
    pub offered_capabilities: Vec<String>,
    /// 期望对方提供的能力
    pub requested_capabilities: Vec<String>,
    /// 握手协议版本
    pub protocol_version: String,
    /// 随机挑战（用于信任验证）
    #[serde(default)]
    pub challenge: Option<String>,
    /// 时间戳
    pub timestamp: i64,
}

/// 握手响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    /// 是否接受握手
    pub accepted: bool,
    /// 响应方提供的能力
    pub provided_capabilities: Vec<String>,
    /// 挑战应答
    #[serde(default)]
    pub challenge_response: Option<String>,
    /// 会话 Token（握手成功后生成）
    #[serde(default)]
    pub session_token: Option<String>,
    /// 响应时间戳
    pub timestamp: i64,
    /// 拒绝原因（如果 accepted=false）
    #[serde(default)]
    pub reject_reason: Option<String>,
}

/// 信任等级
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum TrustLevel {
    /// 未验证
    Unverified = 0,
    /// 基本信任（完成握手）
    Basic = 1,
    /// 已验证（多次成功交互）
    Verified = 2,
    /// 高度信任（长期合作）
    Trusted = 3,
}

/// 信任记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustRecord {
    /// 源 Agent
    pub from_agent: AgentId,
    /// 目标 Agent
    pub to_agent: AgentId,
    /// 信任等级
    pub trust_level: TrustLevel,
    /// 成功交互次数
    pub success_count: u64,
    /// 失败交互次数
    pub failure_count: u64,
    /// 首次交互时间
    pub first_interaction: i64,
    /// 最近交互时间
    pub last_interaction: i64,
}

/// 心跳消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    /// Agent ID
    pub agent_id: AgentId,
    /// 心跳序列号
    pub seq: u64,
    /// Agent 当前状态
    pub status: AgentStatus,
    /// 当前活跃任务数
    #[serde(default)]
    pub active_tasks: u32,
    /// 时间戳
    pub timestamp: i64,
}

/// Agent 发现查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryQuery {
    /// 按能力过滤
    #[serde(default)]
    pub capability: Option<String>,
    /// 按状态过滤
    #[serde(default)]
    pub status: Option<AgentStatus>,
    /// 按信任等级过滤
    #[serde(default)]
    pub min_trust: Option<TrustLevel>,
    /// 自定义标签过滤
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A2A 通信消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AMessage {
    /// 消息 ID
    pub id: String,
    /// 发送方
    pub from: AgentId,
    /// 接收方
    pub to: AgentId,
    /// 消息类型
    pub msg_type: A2AMessageType,
    /// 消息负载
    pub payload: serde_json::Value,
    /// 时间戳
    pub timestamp: i64,
}

/// A2A 消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum A2AMessageType {
    /// 任务请求
    TaskRequest,
    /// 任务响应
    TaskResponse,
    /// 事件通知
    Event,
    /// 查询
    Query,
    /// 查询响应
    QueryResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_serialization() {
        let cap = Capability {
            id: "text-gen".to_string(),
            name: "Text Generation".to_string(),
            description: "Generate text from prompts".to_string(),
            input_format: "text/plain".to_string(),
            output_format: "text/plain".to_string(),
            params: serde_json::json!({"model": "gpt-4"}),
        };

        let json = serde_json::to_string(&cap).unwrap();
        let parsed: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "text-gen");
    }

    #[test]
    fn test_trust_level_ordering() {
        assert!(TrustLevel::Trusted > TrustLevel::Verified);
        assert!(TrustLevel::Verified > TrustLevel::Basic);
        assert!(TrustLevel::Basic > TrustLevel::Unverified);
    }

    #[test]
    fn test_handshake_request_serialization() {
        let req = HandshakeRequest {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            offered_capabilities: vec!["text-gen".to_string()],
            requested_capabilities: vec!["image-analysis".to_string()],
            protocol_version: "1.0".to_string(),
            challenge: Some("random-challenge".to_string()),
            timestamp: 1234567890,
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: HandshakeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.offered_capabilities.len(), 1);
    }

    #[test]
    fn test_agent_status() {
        let status = AgentStatus::Online;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"online\"");
    }
}
