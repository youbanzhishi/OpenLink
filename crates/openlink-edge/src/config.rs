//! # 边缘配置
//!
//! 精简配置，从文件读取，不依赖数据库。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 边缘节点配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// 节点 ID
    pub node_id: String,
    
    /// HTTP 服务地址
    pub listen_addr: String,
    
    /// 文件存储目录
    pub storage_path: String,
    
    /// 缓存配置
    pub cache: CacheConfig,
    
    /// 日志级别
    #[serde(default = "default_log_level")]
    pub log_level: String,
    
    /// 设备密钥（用于节点认证）
    pub device_key: Option<String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl EdgeConfig {
    /// 从 TOML 文件加载配置
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        
        toml::from_str(&content)
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }
    
    /// 创建默认配置
    pub fn default_config() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            listen_addr: "0.0.0.0:8080".to_string(),
            storage_path: "./edge-storage".to_string(),
            cache: CacheConfig::default(),
            log_level: "info".to_string(),
            device_key: None,
        }
    }
}

/// 缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// 是否启用缓存
    pub enabled: bool,
    
    /// 最大缓存条目数
    #[serde(default = "default_cache_size")]
    pub max_entries: usize,
    
    /// 缓存 TTL（秒）
    #[serde(default = "default_cache_ttl")]
    pub ttl_secs: u64,
}

fn default_cache_size() -> usize {
    1000
}

fn default_cache_ttl() -> u64 {
    3600
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: default_cache_size(),
            ttl_secs: default_cache_ttl(),
        }
    }
}

/// 配置错误
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
}
