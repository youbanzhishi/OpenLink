//! # openlink-node — 设备端守护进程入口

use chrono::Utc;
use openlink_node::discovery::DiscoveredNode;
use openlink_node::heartbeat::HeartbeatClient;
use openlink_node::{FileServer, NodeConfig, NodeDiscovery};
use std::sync::Arc;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // 加载配置
    let config = NodeConfig::load("node.toml").await.unwrap_or_else(|_| {
        tracing::info!("No config found, using defaults");
        NodeConfig::default()
    });

    tracing::info!(
        node_id = %config.node_id,
        version = %config.version,
        "OpenLink Node starting"
    );

    // 获取本机 LAN IP
    let lan_ip = get_local_ip()
        .await
        .unwrap_or_else(|| "127.0.0.1".to_string());

    // 启动 mDNS 广播
    let local_node = DiscoveredNode {
        node_id: config.node_id.clone(),
        ip: lan_ip.clone(),
        port: config.file_service_port,
        version: config.version.clone(),
        capabilities: vec![
            "file_server".to_string(),
            "heartbeat".to_string(),
            "encrypted_transfer".to_string(),
        ],
        discovered_at: Utc::now(),
        latency_ms: None,
    };

    let mut discovery = NodeDiscovery::new(&config.mdns_service_name);
    if let Err(e) = discovery.start_broadcast(local_node.clone()).await {
        tracing::warn!(error = %e, "Failed to start mDNS broadcast");
    }

    // 启动 HTTP 文件服务
    let file_server = Arc::new(FileServer::new(
        openlink_node::file_service::FileBackend::Local(config.storage_path.clone().into()),
    ));
    let file_port = config.file_service_port;
    tokio::spawn(async move {
        if let Err(e) = file_server.serve(file_port).await {
            tracing::error!(error = %e, "File server error");
        }
    });

    // 启动心跳客户端
    let heartbeat_client = Arc::new(HeartbeatClient::new(
        &config.heartbeat_server_url,
        &config.node_id,
        config.heartbeat_interval_secs,
    ));
    tokio::spawn(async move {
        heartbeat_client.start().await;
    });

    // 定期发现节点并上报
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            match discovery.discover().await {
                Ok(peers) => {
                    tracing::info!(peer_count = peers.len(), "Discovered LAN peers");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Discovery failed");
                }
            }
        }
    });

    // 等待 SIGINT
    tokio::signal::ctrl_c().await?;
    tracing::info!("OpenLink Node shutting down");
    Ok(())
}

async fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}
