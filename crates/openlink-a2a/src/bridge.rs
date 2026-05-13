//! # 协议桥接层 (Phase 10)
//!
//! 统一 MCP/A2A/HTTP 三种协议的接口，支持自动协商和消息格式转换。

use crate::mcp::{McpParser, McpRequest};
use crate::types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// 支持的协议类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolType {
    /// A2A 协议（OpenLink 原生）
    A2A,
    /// MCP 协议
    Mcp,
    /// HTTP/REST 协议
    Http,
}

/// 协议协商结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiatedProtocol {
    /// 选择的协议
    pub protocol: ProtocolType,
    /// 协议版本
    pub version: String,
    /// 选择原因
    pub reason: String,
}

/// 协议能力描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolCapabilities {
    /// 支持的协议列表（按优先级排列）
    pub supported_protocols: Vec<ProtocolType>,
    /// 各协议版本
    pub versions: HashMap<ProtocolType, String>,
    /// 延迟偏好（毫秒）
    #[serde(default)]
    pub latency_preference_ms: Option<u64>,
    /// 是否支持流式传输
    #[serde(default)]
    pub supports_streaming: bool,
}

/// 统一消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMessage {
    /// 消息 ID
    pub id: String,
    /// 来源协议
    pub source_protocol: ProtocolType,
    /// 目标协议
    pub target_protocol: ProtocolType,
    /// 发送方
    pub from: String,
    /// 接收方
    pub to: String,
    /// 消息方法/动作
    pub method: String,
    /// 消息负载
    pub payload: serde_json::Value,
    /// 元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// 时间戳
    pub timestamp: i64,
}

/// 协议转换错误
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Unsupported protocol: {0:?}")]
    UnsupportedProtocol(ProtocolType),

    #[error("Conversion error: {0}")]
    ConversionError(String),

    #[error("Negotiation failed: {0}")]
    NegotiationFailed(String),

    #[error("Protocol mismatch: expected {expected:?}, got {actual:?}")]
    ProtocolMismatch {
        expected: ProtocolType,
        actual: ProtocolType,
    },
}

/// 协议桥接 trait
///
/// 统一不同协议的接口，所有协议适配器必须实现此 trait。
#[async_trait]
pub trait ProtocolBridge: Send + Sync {
    /// 获取协议类型
    fn protocol_type(&self) -> ProtocolType;

    /// 将统一消息转换为本协议消息
    async fn encode(&self, message: &UnifiedMessage) -> Result<Vec<u8>, BridgeError>;

    /// 将本协议消息解析为统一消息
    async fn decode(&self, data: &[u8]) -> Result<UnifiedMessage, BridgeError>;

    /// 获取协议能力
    fn capabilities(&self) -> ProtocolCapabilities;
}

/// A2A 协议桥接
pub struct A2ABridge;

#[async_trait]
impl ProtocolBridge for A2ABridge {
    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::A2A
    }

    async fn encode(&self, message: &UnifiedMessage) -> Result<Vec<u8>, BridgeError> {
        let a2a_msg = A2AMessage {
            id: message.id.clone(),
            from: message.from.clone(),
            to: message.to.clone(),
            msg_type: method_to_a2a_type(&message.method),
            payload: message.payload.clone(),
            timestamp: message.timestamp,
        };
        serde_json::to_vec(&a2a_msg).map_err(|e| BridgeError::ConversionError(e.to_string()))
    }

    async fn decode(&self, data: &[u8]) -> Result<UnifiedMessage, BridgeError> {
        let a2a_msg: A2AMessage =
            serde_json::from_slice(data).map_err(|e| BridgeError::ConversionError(e.to_string()))?;

        Ok(UnifiedMessage {
            id: a2a_msg.id,
            source_protocol: ProtocolType::A2A,
            target_protocol: ProtocolType::A2A,
            from: a2a_msg.from,
            to: a2a_msg.to,
            method: a2a_type_to_method(&a2a_msg.msg_type),
            payload: a2a_msg.payload,
            metadata: HashMap::new(),
            timestamp: a2a_msg.timestamp,
        })
    }

    fn capabilities(&self) -> ProtocolCapabilities {
        let mut versions = HashMap::new();
        versions.insert(ProtocolType::A2A, "1.0".to_string());
        ProtocolCapabilities {
            supported_protocols: vec![ProtocolType::A2A],
            versions,
            latency_preference_ms: None,
            supports_streaming: true,
        }
    }
}

/// MCP 协议桥接
pub struct McpBridge;

#[async_trait]
impl ProtocolBridge for McpBridge {
    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Mcp
    }

    async fn encode(&self, message: &UnifiedMessage) -> Result<Vec<u8>, BridgeError> {
        let mcp_request = McpRequest {
            id: serde_json::json!(message.id),
            method: message.method.clone(),
            params: message.payload.clone(),
        };
        McpParser::serialize_request(&mcp_request).map_err(|e| BridgeError::ConversionError(e.to_string()))
    }

    async fn decode(&self, data: &[u8]) -> Result<UnifiedMessage, BridgeError> {
        let mcp_request = McpParser::parse_request(data).map_err(|e| BridgeError::ConversionError(e.to_string()))?;

        Ok(UnifiedMessage {
            id: match mcp_request.id {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                other => other.to_string(),
            },
            source_protocol: ProtocolType::Mcp,
            target_protocol: ProtocolType::Mcp,
            from: String::new(),
            to: String::new(),
            method: mcp_request.method,
            payload: mcp_request.params,
            metadata: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    fn capabilities(&self) -> ProtocolCapabilities {
        let mut versions = HashMap::new();
        versions.insert(ProtocolType::Mcp, "2024-11-05".to_string());
        ProtocolCapabilities {
            supported_protocols: vec![ProtocolType::Mcp],
            versions,
            latency_preference_ms: None,
            supports_streaming: true,
        }
    }
}

/// HTTP 协议桥接
pub struct HttpBridge;

#[async_trait]
impl ProtocolBridge for HttpBridge {
    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Http
    }

    async fn encode(&self, message: &UnifiedMessage) -> Result<Vec<u8>, BridgeError> {
        // HTTP REST 风格: { method, path, body }
        let http_msg = serde_json::json!({
            "method": "POST",
            "path": format!("/api/{}", message.method),
            "body": {
                "id": message.id,
                "from": message.from,
                "to": message.to,
                "payload": message.payload,
                "timestamp": message.timestamp,
            },
            "headers": message.metadata,
        });
        serde_json::to_vec(&http_msg).map_err(|e| BridgeError::ConversionError(e.to_string()))
    }

    async fn decode(&self, data: &[u8]) -> Result<UnifiedMessage, BridgeError> {
        let http_msg: serde_json::Value =
            serde_json::from_slice(data).map_err(|e| BridgeError::ConversionError(e.to_string()))?;

        let body = http_msg
            .get("body")
            .ok_or_else(|| BridgeError::ConversionError("Missing body".to_string()))?;

        Ok(UnifiedMessage {
            id: body.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            source_protocol: ProtocolType::Http,
            target_protocol: ProtocolType::Http,
            from: body.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            to: body.get("to").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            method: http_msg
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_start_matches("/api/")
                .to_string(),
            payload: body.get("payload").cloned().unwrap_or(serde_json::Value::Null),
            metadata: http_msg
                .get("headers")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
            timestamp: body.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0),
        })
    }

    fn capabilities(&self) -> ProtocolCapabilities {
        let mut versions = HashMap::new();
        versions.insert(ProtocolType::Http, "1.1".to_string());
        ProtocolCapabilities {
            supported_protocols: vec![ProtocolType::Http],
            versions,
            latency_preference_ms: None,
            supports_streaming: false,
        }
    }
}

// ─── 协议自动协商 ──────────────────────────────────────────

/// 协议协商器
pub struct ProtocolNegotiator {
    /// 各协议优先级权重
    priority: Vec<(ProtocolType, f64)>,
}

impl ProtocolNegotiator {
    /// 创建协商器
    pub fn new() -> Self {
        Self {
            priority: vec![
                (ProtocolType::A2A, 1.0),  // 原生协议最高优先
                (ProtocolType::Mcp, 0.8),  // MCP 次之
                (ProtocolType::Http, 0.5), // HTTP 兜底
            ],
        }
    }

    /// 协商最优协议
    pub fn negotiate(&self, local: &ProtocolCapabilities, remote: &ProtocolCapabilities) -> NegotiatedProtocol {
        // 找到双方都支持的协议
        let common: Vec<&ProtocolType> = local
            .supported_protocols
            .iter()
            .filter(|p| remote.supported_protocols.contains(p))
            .collect();

        if common.is_empty() {
            return NegotiatedProtocol {
                protocol: ProtocolType::Http,
                version: "1.1".to_string(),
                reason: "No common protocol, falling back to HTTP".to_string(),
            };
        }

        // 按优先级排序，选择最高优先级的公共协议
        let mut best: Option<(ProtocolType, f64, String)> = None;

        for (proto, weight) in &self.priority {
            if common.iter().any(|p| *p == proto) {
                let version = local
                    .versions
                    .get(proto)
                    .or_else(|| remote.versions.get(proto))
                    .cloned()
                    .unwrap_or_else(|| "1.0".to_string());

                if best.as_ref().map_or(true, |b| weight > &b.1) {
                    best = Some((proto.clone(), *weight, version));
                }
            }
        }

        match best {
            Some((proto, _, version)) => NegotiatedProtocol {
                protocol: proto,
                version,
                reason: "Best common protocol with priority weight".to_string(),
            },
            None => NegotiatedProtocol {
                protocol: ProtocolType::Http,
                version: "1.1".to_string(),
                reason: "Fallback to HTTP".to_string(),
            },
        }
    }
}

impl Default for ProtocolNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 协议转换网关 ──────────────────────────────────────────

/// 协议转换网关
pub struct ProtocolGateway {
    /// 注册的桥接器
    bridges: Arc<RwLock<HashMap<ProtocolType, Arc<dyn ProtocolBridge>>>>,
    /// 协商器
    negotiator: ProtocolNegotiator,
}

impl ProtocolGateway {
    /// 创建网关
    pub fn new() -> Self {
        let mut bridges: HashMap<ProtocolType, Arc<dyn ProtocolBridge>> = HashMap::new();
        bridges.insert(ProtocolType::A2A, Arc::new(A2ABridge));
        bridges.insert(ProtocolType::Mcp, Arc::new(McpBridge));
        bridges.insert(ProtocolType::Http, Arc::new(HttpBridge));

        Self {
            bridges: Arc::new(RwLock::new(bridges)),
            negotiator: ProtocolNegotiator::new(),
        }
    }

    /// 注册自定义桥接器
    pub async fn register_bridge(&self, bridge: Arc<dyn ProtocolBridge>) {
        let proto = bridge.protocol_type();
        let mut bridges = self.bridges.write().await;
        tracing::info!(protocol = ?proto, "Custom bridge registered");
        bridges.insert(proto, bridge);
    }

    /// 转换消息：从源协议到目标协议
    pub async fn convert(
        &self,
        source_protocol: &ProtocolType,
        target_protocol: &ProtocolType,
        data: &[u8],
    ) -> Result<Vec<u8>, BridgeError> {
        if source_protocol == target_protocol {
            return Ok(data.to_vec());
        }

        let bridges = self.bridges.read().await;

        let source_bridge = bridges
            .get(source_protocol)
            .ok_or_else(|| BridgeError::UnsupportedProtocol(source_protocol.clone()))?;
        let target_bridge = bridges
            .get(target_protocol)
            .ok_or_else(|| BridgeError::UnsupportedProtocol(target_protocol.clone()))?;

        // 源协议解码为统一消息
        let unified = source_bridge.decode(data).await?;

        // 设置目标协议
        let mut unified = unified;
        unified.target_protocol = target_protocol.clone();

        // 编码为目标协议
        target_bridge.encode(&unified).await
    }

    /// 协商协议
    pub fn negotiate(&self, local: &ProtocolCapabilities, remote: &ProtocolCapabilities) -> NegotiatedProtocol {
        self.negotiator.negotiate(local, remote)
    }

    /// 获取指定协议的桥接器
    pub async fn get_bridge(&self, protocol: &ProtocolType) -> Option<Arc<dyn ProtocolBridge>> {
        let bridges = self.bridges.read().await;
        bridges.get(protocol).cloned()
    }
}

impl Default for ProtocolGateway {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 辅助函数 ───────────────────────────────────────────────

/// 将方法名映射到 A2A 消息类型
fn method_to_a2a_type(method: &str) -> A2AMessageType {
    match method {
        "task_request" | "tools/call" => A2AMessageType::TaskRequest,
        "task_response" => A2AMessageType::TaskResponse,
        "event" | "notify" => A2AMessageType::Event,
        "query" | "tools/list" => A2AMessageType::Query,
        "query_response" => A2AMessageType::QueryResponse,
        _ => A2AMessageType::TaskRequest,
    }
}

/// 将 A2A 消息类型映射到方法名
fn a2a_type_to_method(msg_type: &A2AMessageType) -> String {
    match msg_type {
        A2AMessageType::TaskRequest => "task_request".to_string(),
        A2AMessageType::TaskResponse => "task_response".to_string(),
        A2AMessageType::Event => "event".to_string(),
        A2AMessageType::Query => "query".to_string(),
        A2AMessageType::QueryResponse => "query_response".to_string(),
    }
}

// ─── 单元测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_a2a_bridge_roundtrip() {
        let bridge = A2ABridge;
        let msg = UnifiedMessage {
            id: "msg-1".to_string(),
            source_protocol: ProtocolType::A2A,
            target_protocol: ProtocolType::A2A,
            from: "agent-a".to_string(),
            to: "agent-b".to_string(),
            method: "task_request".to_string(),
            payload: serde_json::json!({"task": "analyze"}),
            metadata: HashMap::new(),
            timestamp: 1234567890,
        };

        let encoded = bridge.encode(&msg).await.unwrap();
        let decoded = bridge.decode(&encoded).await.unwrap();
        assert_eq!(decoded.id, "msg-1");
        assert_eq!(decoded.from, "agent-a");
        assert_eq!(decoded.method, "task_request");
    }

    #[tokio::test]
    async fn test_mcp_bridge_roundtrip() {
        let bridge = McpBridge;
        let msg = UnifiedMessage {
            id: "msg-2".to_string(),
            source_protocol: ProtocolType::Mcp,
            target_protocol: ProtocolType::Mcp,
            from: "".to_string(),
            to: "".to_string(),
            method: "tools/call".to_string(),
            payload: serde_json::json!({"name": "test"}),
            metadata: HashMap::new(),
            timestamp: 1234567890,
        };

        let encoded = bridge.encode(&msg).await.unwrap();
        let decoded = bridge.decode(&encoded).await.unwrap();
        assert_eq!(decoded.method, "tools/call");
    }

    #[tokio::test]
    async fn test_protocol_negotiation() {
        let negotiator = ProtocolNegotiator::new();

        let local = ProtocolCapabilities {
            supported_protocols: vec![ProtocolType::A2A, ProtocolType::Mcp, ProtocolType::Http],
            versions: {
                let mut m = HashMap::new();
                m.insert(ProtocolType::A2A, "1.0".to_string());
                m.insert(ProtocolType::Mcp, "2024-11-05".to_string());
                m.insert(ProtocolType::Http, "1.1".to_string());
                m
            },
            latency_preference_ms: None,
            supports_streaming: true,
        };

        let remote = ProtocolCapabilities {
            supported_protocols: vec![ProtocolType::Mcp, ProtocolType::Http],
            versions: {
                let mut m = HashMap::new();
                m.insert(ProtocolType::Mcp, "2024-11-05".to_string());
                m.insert(ProtocolType::Http, "1.1".to_string());
                m
            },
            latency_preference_ms: None,
            supports_streaming: false,
        };

        let result = negotiator.negotiate(&local, &remote);
        assert_eq!(result.protocol, ProtocolType::Mcp);
    }

    #[tokio::test]
    async fn test_protocol_gateway_convert() {
        let gateway = ProtocolGateway::new();

        // Create an A2A message
        let a2a_msg = A2AMessage {
            id: "msg-1".to_string(),
            from: "agent-a".to_string(),
            to: "agent-b".to_string(),
            msg_type: A2AMessageType::TaskRequest,
            payload: serde_json::json!({"task": "analyze"}),
            timestamp: 1234567890,
        };

        let a2a_data = serde_json::to_vec(&a2a_msg).unwrap();

        // Convert A2A -> MCP
        let mcp_data = gateway
            .convert(&ProtocolType::A2A, &ProtocolType::Mcp, &a2a_data)
            .await
            .unwrap();

        // Verify it's valid MCP
        let mcp_req = McpParser::parse_request(&mcp_data).unwrap();
        assert_eq!(mcp_req.method, "task_request");
    }

    #[tokio::test]
    async fn test_protocol_gateway_same_protocol_passthrough() {
        let gateway = ProtocolGateway::new();
        let data = b"test data".to_vec();

        let result = gateway
            .convert(&ProtocolType::A2A, &ProtocolType::A2A, &data)
            .await
            .unwrap();
        assert_eq!(result, data);
    }
}
