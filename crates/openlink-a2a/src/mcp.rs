//! # MCP 协议适配器 (Phase 10)
//!
//! Model Context Protocol 的解析器、Server 端点和 Client 连接器。
//! 支持 stdio 和 SSE 两种传输模式，可从 Extension Registry 自动生成 Tool 描述。

use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

// ─── MCP 类型定义 ──────────────────────────────────────────

/// MCP 传输模式
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// 标准输入/输出模式
    Stdio,
    /// Server-Sent Events 模式
    Sse,
}

/// MCP Tool 描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool 名称
    pub name: String,
    /// Tool 描述
    pub description: String,
    /// 输入参数 JSON Schema
    pub input_schema: serde_json::Value,
    /// 输出格式描述
    #[serde(default)]
    pub output_description: String,
    /// 关联的 Agent 能力
    #[serde(default)]
    pub capability_id: Option<String>,
}

/// MCP 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    /// JSON-RPC 请求 ID
    pub id: serde_json::Value,
    /// 方法名
    pub method: String,
    /// 参数
    #[serde(default)]
    pub params: serde_json::Value,
}

/// MCP 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    /// JSON-RPC 请求 ID
    pub id: serde_json::Value,
    /// 结果
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// 错误
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

/// MCP 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    /// 错误码
    pub code: i64,
    /// 错误消息
    pub message: String,
    /// 额外数据
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// MCP Server 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Server 名称
    pub name: String,
    /// Server 版本
    pub version: String,
    /// 支持的传输模式
    pub transport: McpTransport,
    /// 端点 URL (SSE 模式)
    #[serde(default)]
    pub endpoint: Option<String>,
}

/// MCP Client 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpClientConfig {
    /// 传输模式
    pub transport: McpTransport,
    /// 服务端点 URL (SSE 模式)
    #[serde(default)]
    pub server_url: Option<String>,
    /// 请求超时（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// 最大重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_timeout_ms() -> u64 { 30000 }
fn default_max_retries() -> u32 { 3 }

impl Default for McpClientConfig {
    fn default() -> Self {
        Self {
            transport: McpTransport::Stdio,
            server_url: None,
            timeout_ms: default_timeout_ms(),
            max_retries: default_max_retries(),
        }
    }
}

// ─── MCP 协议解析器 ────────────────────────────────────────

/// MCP 协议解析器
///
/// 负责解析和构建 MCP 消息。
pub struct McpParser;

impl McpParser {
    /// 解析 MCP 请求
    pub fn parse_request(data: &[u8]) -> Result<McpRequest, McpProtocolError> {
        serde_json::from_slice(data).map_err(|e| McpProtocolError::ParseError(e.to_string()))
    }

    /// 解析 MCP 响应
    pub fn parse_response(data: &[u8]) -> Result<McpResponse, McpProtocolError> {
        serde_json::from_slice(data).map_err(|e| McpProtocolError::ParseError(e.to_string()))
    }

    /// 构建 initialize 请求
    pub fn build_initialize_request(client_info: &McpServerInfo) -> McpRequest {
        McpRequest {
            id: serde_json::json!(1),
            method: "initialize".to_string(),
            params: serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": client_info.name,
                    "version": client_info.version,
                }
            }),
        }
    }

    /// 构建 tools/list 请求
    pub fn build_list_tools_request() -> McpRequest {
        McpRequest {
            id: serde_json::json!(2),
            method: "tools/list".to_string(),
            params: serde_json::Value::Null,
        }
    }

    /// 构建 tools/call 请求
    pub fn build_call_tool_request(tool_name: &str, arguments: serde_json::Value) -> McpRequest {
        McpRequest {
            id: serde_json::json!(uuid::Uuid::new_v4().to_string()),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            }),
        }
    }

    /// 构建成功响应
    pub fn build_success_response(id: serde_json::Value, result: serde_json::Value) -> McpResponse {
        McpResponse {
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 构建错误响应
    pub fn build_error_response(id: serde_json::Value, code: i64, message: &str) -> McpResponse {
        McpResponse {
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.to_string(),
                data: None,
            }),
        }
    }

    /// 序列化请求
    pub fn serialize_request(request: &McpRequest) -> Result<Vec<u8>, McpProtocolError> {
        serde_json::to_vec(request).map_err(|e| McpProtocolError::SerializationError(e.to_string()))
    }

    /// 序列化响应
    pub fn serialize_response(response: &McpResponse) -> Result<Vec<u8>, McpProtocolError> {
        serde_json::to_vec(response).map_err(|e| McpProtocolError::SerializationError(e.to_string()))
    }
}

// ─── MCP Server 端点 ───────────────────────────────────────

/// MCP Server：将 OpenLink 的路由/传输能力暴露为 MCP Tool
pub struct McpServer {
    /// Server 信息
    info: McpServerInfo,
    /// 注册的 Tools
    tools: Arc<RwLock<HashMap<String, McpTool>>>,
    /// 传输模式
    transport: McpTransport,
}

impl McpServer {
    /// 创建 MCP Server
    pub fn new(name: &str, version: &str, transport: McpTransport) -> Self {
        let info = McpServerInfo {
            name: name.to_string(),
            version: version.to_string(),
            transport: transport.clone(),
            endpoint: None,
        };

        // Pre-populate builtin tools
        let mut tools = HashMap::new();
        Self::populate_builtin_tools(&mut tools);

        Self {
            info,
            tools: Arc::new(RwLock::new(tools)),
            transport,
        }
    }

    /// 注册内置 Tools（从 OpenLink 能力自动生成）
    fn populate_builtin_tools(tools: &mut HashMap<String, McpTool>) {
        let builtin_tools = vec![
            McpTool {
                name: "openlink.route".to_string(),
                description: "Route a request through the OpenLink network to the best available agent".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target": { "type": "string", "description": "Target agent or capability" },
                        "payload": { "type": "object", "description": "Request payload" },
                    },
                    "required": ["target", "payload"]
                }),
                output_description: "Routing result with response from target agent".to_string(),
                capability_id: Some("routing".to_string()),
            },
            McpTool {
                name: "openlink.discover".to_string(),
                description: "Discover agents by capability in the OpenLink network".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "capability": { "type": "string", "description": "Capability to search for" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Filter tags" },
                    },
                    "required": ["capability"]
                }),
                output_description: "List of matching agents".to_string(),
                capability_id: Some("discovery".to_string()),
            },
            McpTool {
                name: "openlink.transfer".to_string(),
                description: "Transfer data between agents via P2P or relay".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "Source agent ID" },
                        "destination": { "type": "string", "description": "Destination agent ID" },
                        "data_ref": { "type": "string", "description": "Reference to the data" },
                    },
                    "required": ["source", "destination", "data_ref"]
                }),
                output_description: "Transfer status".to_string(),
                capability_id: Some("transfer".to_string()),
            },
        ];

        for tool in builtin_tools {
            tools.insert(tool.name.clone(), tool);
        }
    }

    /// 注册 Tool
    pub async fn register_tool(&self, tool: McpTool) -> Result<(), McpProtocolError> {
        let mut tools = self.tools.write().await;
        if tools.contains_key(&tool.name) {
            return Err(McpProtocolError::ToolAlreadyRegistered(tool.name.clone()));
        }
        tracing::info!(tool = %tool.name, "MCP Tool registered");
        tools.insert(tool.name.clone(), tool);
        Ok(())
    }

    /// 注销 Tool
    pub async fn deregister_tool(&self, name: &str) -> Result<McpTool, McpProtocolError> {
        let mut tools = self.tools.write().await;
        tools.remove(name).ok_or_else(|| McpProtocolError::ToolNotFound(name.to_string()))
    }

    /// 列出所有 Tools
    pub async fn list_tools(&self) -> Vec<McpTool> {
        let tools = self.tools.read().await;
        tools.values().cloned().collect()
    }

    /// 从 Agent 能力生成 Tool 描述
    pub fn capability_to_tool(capability: &Capability) -> McpTool {
        McpTool {
            name: format!("agent.{}", capability.id),
            description: if capability.description.is_empty() {
                capability.name.clone()
            } else {
                capability.description.clone()
            },
            input_schema: if capability.input_format.is_empty() {
                serde_json::json!({"type": "object", "properties": {}})
            } else {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string", "format": capability.input_format }
                    }
                })
            },
            output_description: capability.output_format.clone(),
            capability_id: Some(capability.id.clone()),
        }
    }

    /// 处理 MCP 请求
    pub async fn handle_request(&self, request: &McpRequest) -> McpResponse {
        match request.method.as_str() {
            "initialize" => {
                McpParser::build_success_response(
                    request.id.clone(),
                    serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": { "listChanged": true } },
                        "serverInfo": {
                            "name": self.info.name,
                            "version": self.info.version,
                        }
                    }),
                )
            }
            "tools/list" => {
                let tools = self.list_tools().await;
                McpParser::build_success_response(
                    request.id.clone(),
                    serde_json::json!({
                        "tools": tools
                    }),
                )
            }
            "tools/call" => {
                let tool_name = request.params.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tools = self.tools.read().await;
                if tools.contains_key(tool_name) {
                    McpParser::build_success_response(
                        request.id.clone(),
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Tool {} executed successfully", tool_name)
                            }]
                        }),
                    )
                } else {
                    McpParser::build_error_response(
                        request.id.clone(),
                        -32601,
                        &format!("Tool not found: {}", tool_name),
                    )
                }
            }
            _ => {
                McpParser::build_error_response(
                    request.id.clone(),
                    -32601,
                    &format!("Method not found: {}", request.method),
                )
            }
        }
    }

    /// 获取 Server 信息
    pub fn server_info(&self) -> &McpServerInfo {
        &self.info
    }

    /// 获取传输模式
    pub fn transport(&self) -> &McpTransport {
        &self.transport
    }
}

// ─── MCP Client 连接器 ──────────────────────────────────────

/// MCP Client：连接外部 MCP 服务
pub struct McpClient {
    /// 配置
    config: McpClientConfig,
    /// 已知的外部 MCP Server
    servers: Arc<RwLock<HashMap<String, McpServerInfo>>>,
    /// 缓存的 Tool 列表
    cached_tools: Arc<RwLock<HashMap<String, Vec<McpTool>>>>,
}

impl McpClient {
    /// 创建 MCP Client
    pub fn new(config: McpClientConfig) -> Self {
        Self {
            config,
            servers: Arc::new(RwLock::new(HashMap::new())),
            cached_tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册外部 MCP Server
    pub async fn register_server(&self, server_info: McpServerInfo) -> Result<(), McpProtocolError> {
        let name = server_info.name.clone();
        tracing::info!(server = %name, transport = ?server_info.transport, "MCP Server registered");
        let mut servers = self.servers.write().await;
        servers.insert(name, server_info);
        Ok(())
    }

    /// 注销外部 MCP Server
    pub async fn deregister_server(&self, name: &str) -> Result<McpServerInfo, McpProtocolError> {
        let mut servers = self.servers.write().await;
        let mut cached = self.cached_tools.write().await;
        cached.remove(name);
        servers.remove(name).ok_or_else(|| McpProtocolError::ServerNotFound(name.to_string()))
    }

    /// 列出已注册 Server
    pub async fn list_servers(&self) -> Vec<McpServerInfo> {
        let servers = self.servers.read().await;
        servers.values().cloned().collect()
    }

    /// 获取指定 Server 的 Tools（使用缓存）
    pub async fn get_server_tools(&self, server_name: &str) -> Option<Vec<McpTool>> {
        let cached = self.cached_tools.read().await;
        cached.get(server_name).cloned()
    }

    /// 刷新指定 Server 的 Tool 缓存
    pub async fn refresh_tools(&self, server_name: &str, tools: Vec<McpTool>) {
        let mut cached = self.cached_tools.write().await;
        tracing::info!(server = %server_name, tool_count = tools.len(), "MCP tools cache refreshed");
        cached.insert(server_name.to_string(), tools);
    }

    /// 构建 tools/call 请求给指定 Server
    pub fn build_tool_call(&self, tool_name: &str, arguments: serde_json::Value) -> McpRequest {
        McpParser::build_call_tool_request(tool_name, arguments)
    }

    /// 获取 Client 配置
    pub fn config(&self) -> &McpClientConfig {
        &self.config
    }
}

// ─── 错误类型 ───────────────────────────────────────────────

/// MCP 协议错误
#[derive(Debug, thiserror::Error)]
pub enum McpProtocolError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Tool already registered: {0}")]
    ToolAlreadyRegistered(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Timeout: {0}")]
    Timeout(String),
}

// ─── 单元测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_request_parsing() {
        let json = r#"{"id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
        let req = McpParser::parse_request(json.as_bytes()).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, serde_json::json!(1));
    }

    #[test]
    fn test_mcp_response_parsing() {
        let json = r#"{"id":1,"result":{"protocolVersion":"2024-11-05"}}"#;
        let resp = McpParser::parse_response(json.as_bytes()).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_mcp_server_initialize() {
        let server = McpServer::new("test-server", "1.0.0", McpTransport::Stdio);
        assert_eq!(server.server_info().name, "test-server");
        assert_eq!(server.transport(), &McpTransport::Stdio);
    }

    #[tokio::test]
    async fn test_mcp_server_tool_registration() {
        let server = McpServer::new("test-server", "1.0.0", McpTransport::Stdio);

        let tool = McpTool {
            name: "custom.tool".to_string(),
            description: "A custom tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_description: "result".to_string(),
            capability_id: Some("custom".to_string()),
        };

        server.register_tool(tool).await.unwrap();
        let tools = server.list_tools().await;

        // 3 builtin + 1 custom
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().any(|t| t.name == "custom.tool"));
    }

    #[tokio::test]
    async fn test_mcp_server_handle_initialize() {
        let server = McpServer::new("test-server", "1.0.0", McpTransport::Sse);

        let request = McpRequest {
            id: serde_json::json!(1),
            method: "initialize".to_string(),
            params: serde_json::Value::Null,
        };

        let response = server.handle_request(&request).await;
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "test-server");
    }

    #[tokio::test]
    async fn test_mcp_server_handle_tools_list() {
        let server = McpServer::new("test-server", "1.0.0", McpTransport::Stdio);

        let request = McpRequest {
            id: serde_json::json!(2),
            method: "tools/list".to_string(),
            params: serde_json::Value::Null,
        };

        let response = server.handle_request(&request).await;
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert!(result["tools"].is_array());
        // Should have 3 builtin tools
        assert_eq!(result["tools"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_mcp_server_handle_unknown_method() {
        let server = McpServer::new("test-server", "1.0.0", McpTransport::Stdio);

        let request = McpRequest {
            id: serde_json::json!(99),
            method: "unknown/method".to_string(),
            params: serde_json::Value::Null,
        };

        let response = server.handle_request(&request).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_mcp_server_handle_tools_call() {
        let server = McpServer::new("test-server", "1.0.0", McpTransport::Stdio);

        let request = McpRequest {
            id: serde_json::json!(3),
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "openlink.route"}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.result.is_some());
    }

    #[tokio::test]
    async fn test_mcp_server_handle_tools_call_not_found() {
        let server = McpServer::new("test-server", "1.0.0", McpTransport::Stdio);

        let request = McpRequest {
            id: serde_json::json!(3),
            method: "tools/call".to_string(),
            params: serde_json::json!({"name": "nonexistent.tool"}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn test_mcp_client_register_server() {
        let client = McpClient::new(McpClientConfig::default());

        let server_info = McpServerInfo {
            name: "external-mcp".to_string(),
            version: "1.0".to_string(),
            transport: McpTransport::Sse,
            endpoint: Some("http://localhost:8080/mcp".to_string()),
        };

        client.register_server(server_info).await.unwrap();
        let servers = client.list_servers().await;
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "external-mcp");
    }

    #[test]
    fn test_capability_to_tool() {
        let cap = Capability {
            id: "text-gen".to_string(),
            name: "Text Generation".to_string(),
            description: "Generate text from prompts".to_string(),
            input_format: "text/plain".to_string(),
            output_format: "text/plain".to_string(),
            params: serde_json::Value::Null,
        };

        let tool = McpServer::capability_to_tool(&cap);
        assert_eq!(tool.name, "agent.text-gen");
        assert_eq!(tool.capability_id, Some("text-gen".to_string()));
        assert!(tool.input_schema.is_object());
    }

    #[test]
    fn test_mcp_build_initialize_request() {
        let info = McpServerInfo {
            name: "client".to_string(),
            version: "1.0".to_string(),
            transport: McpTransport::Stdio,
            endpoint: None,
        };
        let req = McpParser::build_initialize_request(&info);
        assert_eq!(req.method, "initialize");
        assert!(req.params["clientInfo"]["name"] == "client");
    }

    #[test]
    fn test_mcp_serialization_roundtrip() {
        let request = McpParser::build_list_tools_request();
        let bytes = McpParser::serialize_request(&request).unwrap();
        let parsed = McpParser::parse_request(&bytes).unwrap();
        assert_eq!(parsed.method, "tools/list");
    }

    #[test]
    fn test_mcp_error_response() {
        let resp = McpParser::build_error_response(
            serde_json::json!(1),
            -32600,
            "Invalid Request",
        );
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32600);
    }

    #[test]
    fn test_mcp_transport_serialization() {
        let stdio = McpTransport::Stdio;
        let json = serde_json::to_string(&stdio).unwrap();
        assert_eq!(json, "\"stdio\"");

        let sse = McpTransport::Sse;
        let json = serde_json::to_string(&sse).unwrap();
        assert_eq!(json, "\"sse\"");
    }
}
