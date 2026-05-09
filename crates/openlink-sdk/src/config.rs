//! # SDK 配置

use serde::{Deserialize, Serialize};

/// SDK 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// OpenLink API 基础 URL
    pub base_url: String,
    /// API Token（用于认证）
    pub api_token: Option<String>,
    /// Agent ID（自动注入）
    pub agent_id: Option<String>,
    /// Agent 类型
    pub agent_type: Option<String>,
    /// Device ID（自动注入）
    pub device_id: Option<String>,
    /// 请求超时（秒）
    pub timeout_secs: u64,
    /// 是否启用 TLS
    pub tls_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            api_token: None,
            agent_id: None,
            agent_type: None,
            device_id: None,
            timeout_secs: 30,
            tls_enabled: false,
        }
    }
}

impl Config {
    /// 创建新配置
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    /// 设置 API Token
    pub fn api_token(mut self, token: impl Into<String>) -> Self {
        self.api_token = Some(token.into());
        self
    }

    /// 设置 Agent ID
    pub fn agent_id(mut self, id: impl Into<String>) -> Self {
        self.agent_id = Some(id.into());
        self
    }

    /// 设置 Agent 类型
    pub fn agent_type(mut self, type_: impl Into<String>) -> Self {
        self.agent_type = Some(type_.into());
        self
    }

    /// 设置 Device ID
    pub fn device_id(mut self, id: impl Into<String>) -> Self {
        self.device_id = Some(id.into());
        self
    }

    /// 设置超时
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 获取完整 API URL
    pub fn api_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }
}
