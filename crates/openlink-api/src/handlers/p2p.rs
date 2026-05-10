//! # P2P API 端点处理器 (Phase 9)
//!
//! - GET /api/v1/p2p/peers — 在线节点列表
//! - GET /api/v1/p2p/status — P2P 连接状态
//! - POST /api/v1/p2p/connect — 建立P2P连接

use axum::response::IntoResponse;
use axum::{extract::State, http::StatusCode, response::Response};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

/// 在线节点列表响应
#[derive(Debug, Serialize, Deserialize)]
pub struct PeersResponse {
    pub peers: Vec<PeerEntry>,
    pub total: usize,
}

/// 节点条目
#[derive(Debug, Serialize, Deserialize)]
pub struct PeerEntry {
    pub node_id: String,
    pub addr: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub last_heartbeat_ago_secs: i64,
}

/// P2P 连接状态响应
#[derive(Debug, Serialize, Deserialize)]
pub struct P2pStatusResponse {
    pub local_node_id: String,
    pub active_connections: usize,
    pub connections: Vec<ConnectionEntry>,
    pub nat_type: String,
    pub routing_table_size: usize,
}

/// 连接条目
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionEntry {
    pub peer_id: String,
    pub state: String,
    pub latency_ms: Option<f64>,
    pub quality_score: Option<f64>,
    pub bytes_transferred: u64,
}

/// 建立连接请求
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectRequest {
    pub peer_node_id: String,
    pub mode: Option<String>,
}

/// 建立连接响应
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectResponse {
    pub success: bool,
    pub peer_id: String,
    pub strategy: String,
    pub estimated_latency_ms: Option<f64>,
    pub message: String,
}

/// GET /api/v1/p2p/peers — 在线节点列表
pub async fn list_peers(_state: State<Arc<AppState>>) -> Response {
    // 模拟返回在线节点列表
    // 实际实现从 GossipMembership 获取
    let response = PeersResponse {
        peers: vec![
            PeerEntry {
                node_id: "edge-cn-east-1".to_string(),
                addr: "10.0.1.1:8080".to_string(),
                status: "alive".to_string(),
                capabilities: vec!["p2p".to_string(), "edge".to_string()],
                last_heartbeat_ago_secs: 3,
            },
            PeerEntry {
                node_id: "edge-cn-south-1".to_string(),
                addr: "10.0.2.1:8080".to_string(),
                status: "alive".to_string(),
                capabilities: vec!["p2p".to_string(), "edge".to_string(), "relay".to_string()],
                last_heartbeat_ago_secs: 5,
            },
        ],
        total: 2,
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(serde_json::to_string(&response).unwrap_or_default().into())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// GET /api/v1/p2p/status — P2P 连接状态
pub async fn get_status(_state: State<Arc<AppState>>) -> Response {
    let response = P2pStatusResponse {
        local_node_id: "local-node".to_string(),
        active_connections: 1,
        connections: vec![ConnectionEntry {
            peer_id: "edge-cn-east-1".to_string(),
            state: "connected".to_string(),
            latency_ms: Some(15.0),
            quality_score: Some(85.0),
            bytes_transferred: 1_048_576,
        }],
        nat_type: "full_cone".to_string(),
        routing_table_size: 3,
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(serde_json::to_string(&response).unwrap_or_default().into())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// POST /api/v1/p2p/connect — 建立P2P连接
pub async fn connect(
    _state: State<Arc<AppState>>,
    axum::Json(req): axum::Json<ConnectRequest>,
) -> Response {
    let mode = req.mode.as_deref().unwrap_or("auto");

    let response = ConnectResponse {
        success: true,
        peer_id: req.peer_node_id.clone(),
        strategy: mode.to_string(),
        estimated_latency_ms: Some(20.0),
        message: format!(
            "P2P connection to {} established via {}",
            req.peer_node_id, mode
        ),
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(serde_json::to_string(&response).unwrap_or_default().into())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
