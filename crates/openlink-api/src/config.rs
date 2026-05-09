//! # 应用配置
//!
//! 配置来源：default.toml + 环境变量覆盖
//! 优先级：环境变量 > 配置文件 > 默认值

use serde::Deserialize;

/// 应用配置根结构
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub store: StoreConfig,
    #[serde(default)]
    pub shortcode: ShortCodeConfig,
}

/// 服务器配置
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

/// 存储配置
#[derive(Debug, Deserialize, Clone)]
pub struct StoreConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_db_path")]
    pub path: String,
}

fn default_backend() -> String {
    "sqlite".to_string()
}

fn default_db_path() -> String {
    "./data/openlink.db".to_string()
}

impl StoreConfig {
    /// 生成 SQLite 数据库 URL
    pub fn database_url(&self) -> String {
        if self.path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{}?mode=rwc", self.path)
        }
    }
}

/// 短码配置
#[derive(Debug, Deserialize, Clone)]
pub struct ShortCodeConfig {
    #[serde(default = "default_code_length")]
    pub length: usize,
    #[serde(default = "default_charset")]
    pub charset: String,
}

impl Default for ShortCodeConfig {
    fn default() -> Self {
        Self {
            length: default_code_length(),
            charset: default_charset(),
        }
    }
}

fn default_code_length() -> usize {
    6
}

fn default_charset() -> String {
    "base62".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: default_host(),
                port: default_port(),
            },
            store: StoreConfig {
                backend: default_backend(),
                path: default_db_path(),
            },
            shortcode: ShortCodeConfig {
                length: default_code_length(),
                charset: default_charset(),
            },
        }
    }
}

impl AppConfig {
    /// 从文件加载配置
    pub fn load() -> Result<Self, String> {
        // 尝试从当前目录和项目根目录加载
        let config_paths = [
            "config/default.toml",
            "openlink/config/default.toml",
        ];

        for path in &config_paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                let config: AppConfig = toml::from_str(&content)
                    .map_err(|e| format!("Failed to parse config {}: {}", path, e))?;
                tracing::info!(path = %path, "Loaded config from file");
                return Ok(config);
            }
        }

        tracing::info!("No config file found, using defaults");
        Ok(Self::default())
    }
}
