//! # 节点配置
//!
//! 从配置文件加载节点配置，支持 TOML 格式。

use serde::{Deserialize, Serialize};

/// 节点配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// 节点唯一 ID（自动生成，首次启动时写入）
    pub node_id: String,
    /// 节点显示名称
    pub display_name: String,
    /// 软件版本
    pub version: String,
    /// HTTP 文件服务端口
    pub file_service_port: u16,
    /// 心跳服务器 URL
    pub heartbeat_server_url: String,
    /// 心跳间隔（秒）
    pub heartbeat_interval_secs: u64,
    /// 本地存储路径
    pub storage_path: String,
    /// 传输加密
    pub encryption_enabled: bool,
    /// mDNS 服务名
    pub mdns_service_name: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            display_name: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "openlink-node".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
            file_service_port: 8080,
            heartbeat_server_url: "http://localhost:3000".to_string(),
            heartbeat_interval_secs: 30,
            storage_path: "./node_storage".to_string(),
            encryption_enabled: true,
            mdns_service_name: "_openlink._tcp.local.".to_string(),
        }
    }
}

impl NodeConfig {
    /// 从 TOML 文件加载配置
    pub async fn load(path: &str) -> Result<Self, ConfigError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ConfigError::IoError(e.to_string()))?;

        toml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// 保存配置到 TOML 文件
    pub async fn save(&self, path: &str) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self).map_err(|e| ConfigError::SerializeError(e.to_string()))?;

        tokio::fs::write(path, content)
            .await
            .map_err(|e| ConfigError::IoError(e.to_string()))
    }
}

/// 配置错误
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Serialization error: {0}")]
    SerializeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = NodeConfig::default();
        assert!(!config.node_id.is_empty());
        assert_eq!(config.file_service_port, 8080);
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert!(config.encryption_enabled);
    }

    #[test]
    fn test_config_serialization() {
        let config = NodeConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: NodeConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.node_id, config.node_id);
    }
}
