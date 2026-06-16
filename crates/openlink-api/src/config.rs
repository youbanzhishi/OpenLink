//! # 应用配置
//!
//! 配置来源：default.toml + 环境变量覆盖
//! 优先级：环境变量 > 配置文件 > 默认值
//!
//! Phase 2: 新增 auth 配置（API Token 认证）
//! Phase 3: 新增 knowledge 配置（知识体系一键接入）

use openlink_core::ApiToken;
use serde::Deserialize;

/// 应用配置根结构
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub store: StoreConfig,
    #[serde(default)]
    pub shortcode: ShortCodeConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
}

/// 服务器配置
#[derive(Debug, Deserialize, Clone, Default)]
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
#[derive(Debug, Deserialize, Clone, Default)]
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

/// 认证配置（Phase 2）
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AuthConfig {
    /// 是否启用认证
    #[serde(default = "default_auth_enabled")]
    pub enabled: bool,
    /// API Token 列表
    #[serde(default)]
    pub tokens: Vec<TokenConfig>,
}

fn default_auth_enabled() -> bool {
    false
}

/// Token 配置
#[derive(Debug, Deserialize, Clone, Default)]
pub struct TokenConfig {
    /// Token 值
    pub token: String,
    /// Token 名称
    pub name: String,
    /// 权限范围：read / write / admin
    #[serde(default = "default_token_scopes")]
    pub scopes: Vec<String>,
}

fn default_token_scopes() -> Vec<String> {
    vec!["read".to_string()]
}

/// 知识源配置
#[derive(Debug, Deserialize, Clone)]
pub struct KnowledgeSource {
    /// 源标识名（用于URL路径，如 "private"/"public"）
    pub name: String,
    /// 显示名称
    #[serde(default)]
    pub display_name: String,
    /// 短链码（极短，如 "1"/"f"，用于 y6e.cn/1 这种短链）
    #[serde(default)]
    pub short_code: String,
    /// 知识仓库本地路径
    pub repo_path: String,
    /// 该源的邀请码列表
    #[serde(default)]
    pub invite_codes: Vec<String>,
    /// 同步端点认证token（为空则不验证）
    #[serde(default)]
    pub sync_token: String,
}
impl KnowledgeSource {
    /// 显示名称，未设置时用 name
    pub fn label(&self) -> &str {
        if self.display_name.is_empty() {
            &self.name
        } else {
            &self.display_name
        }
    }
}

/// 知识体系配置（Phase 3）
#[derive(Debug, Deserialize, Clone)]
pub struct KnowledgeConfig {
    /// 是否启用知识体系
    #[serde(default = "default_knowledge_enabled")]
    pub enabled: bool,
    /// API base URL，用于生成资源 URL
    #[serde(default = "default_knowledge_base_url")]
    pub base_url: String,
    /// 多知识源列表
    #[serde(default)]
    pub sources: Vec<KnowledgeSource>,
    // ── 兼容旧配置（单源）──
    /// 旧字段：知识体系仓库本地路径（兼容，会自动转为 sources[0]）
    #[serde(default)]
    pub repo_path: String,
    /// 旧字段：邀请码列表（兼容，会自动转为 sources[0]）
    #[serde(default)]
    pub invite_codes: Vec<String>,
    /// 旧字段：同步token（兼容，会自动转为 sources[0]）
    #[serde(default)]
    pub sync_token: String,
    /// 知识体系仓库本地路径
    // MVP: 写死的邀请码列表
}

fn default_knowledge_enabled() -> bool {
    false
}

fn default_knowledge_base_url() -> String {
    "http://localhost:3000".to_string()
}

impl KnowledgeConfig {
    /// 获取解析后的知识源列表（自动兼容旧配置）
    pub fn resolved_sources(&self) -> Vec<KnowledgeSource> {
        if !self.sources.is_empty() {
            return self.sources.clone();
        }
        // 兼容：旧配置只有 repo_path/invite_codes/sync_token，包装为 "private" 源
        if !self.repo_path.is_empty() {
            return vec![KnowledgeSource {
                name: "private".to_string(),
                display_name: "OpenClaw知识体系".to_string(),
                short_code: String::new(),
                repo_path: self.repo_path.clone(),
                invite_codes: self.invite_codes.clone(),
                sync_token: self.sync_token.clone(),
            }];
        }
        vec![]
    }
    /// 根据 invite_code 查找对应的知识源
    pub fn find_source_by_code(&self, code: &str) -> Option<KnowledgeSource> {
        for source in self.resolved_sources() {
            if source.invite_codes.contains(&code.to_string()) {
                return Some(source);
            }
        }
        None
    }
    /// 根据短链码查找知识源（优先匹配 short_code，其次匹配 invite_code）
    pub fn find_source_by_short_code(&self, code: &str) -> Option<KnowledgeSource> {
        for source in self.resolved_sources() {
            if !source.short_code.is_empty() && source.short_code == code {
                return Some(source);
            }
        }
        // fallback: 邀请码也能当短链用
        self.find_source_by_code(code)
    }
    /// 根据 name 查找知识源
    pub fn find_source_by_name(&self, name: &str) -> Option<KnowledgeSource> {
        self.resolved_sources().into_iter().find(|s| s.name == name)
    }
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_knowledge_base_url(),
            sources: vec![],
            repo_path: String::new(),
            invite_codes: vec![],
            sync_token: String::new(),

        }
    }
}

impl AuthConfig {
    /// 将配置转换为 ApiToken 列表
    pub fn to_api_tokens(&self) -> Vec<ApiToken> {
        use openlink_core::TokenScope;

        self.tokens
            .iter()
            .map(|tc| {
                let scopes: Vec<TokenScope> = tc
                    .scopes
                    .iter()
                    .filter_map(|s| match s.as_str() {
                        "read" => Some(TokenScope::Read),
                        "write" => Some(TokenScope::Write),
                        "admin" => Some(TokenScope::Admin),
                        _ => None,
                    })
                    .collect();

                ApiToken {
                    token: tc.token.clone(),
                    name: tc.name.clone(),
                    scopes,
                }
            })
            .collect()
    }

    /// 验证 Token 是否有效，返回权限范围
    pub fn validate_token(&self, token: &str) -> Option<Vec<openlink_core::TokenScope>> {
        if !self.enabled {
            // 认证未启用，返回 admin 权限
            return Some(vec![openlink_core::TokenScope::Admin]);
        }

        self.to_api_tokens()
            .iter()
            .find(|t| t.token == token)
            .map(|t| t.scopes.clone())
    }
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
            auth: AuthConfig::default(),
            knowledge: KnowledgeConfig::default(),
        }
    }
}

impl AppConfig {
    /// 从文件加载配置
    pub fn load() -> Result<Self, String> {
        // 尝试从当前目录和项目根目录加载
        let config_paths = ["config/default.toml", "openlink/config/default.toml"];

        for path in &config_paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                let config: AppConfig =
                    toml::from_str(&content).map_err(|e| format!("Failed to parse config {}: {}", path, e))?;
                tracing::info!(path = %path, "Loaded config from file");
                return Ok(config);
            }
        }

        tracing::info!("No config file found, using defaults");
        Ok(Self::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server.port, 3000);
        assert!(!config.auth.enabled);
    }

    #[test]
    fn test_auth_config_validate_token_disabled() {
        let config = AuthConfig::default();
        let result = config.validate_token("any-token");
        assert!(result.is_some());
    }

    #[test]
    fn test_auth_config_validate_token_enabled() {
        let config = AuthConfig {
            enabled: true,
            tokens: vec![TokenConfig {
                token: "test-secret".to_string(),
                name: "test".to_string(),
                scopes: vec!["read".to_string(), "write".to_string()],
            }],
        };

        // 有效 token
        let result = config.validate_token("test-secret");
        assert!(result.is_some());
        let scopes = result.unwrap();
        assert!(scopes.contains(&openlink_core::TokenScope::Read));
        assert!(scopes.contains(&openlink_core::TokenScope::Write));

        // 无效 token
        let result = config.validate_token("invalid-token");
        assert!(result.is_none());
    }

    #[test]
    fn test_to_api_tokens() {
        let config = AuthConfig {
            enabled: true,
            tokens: vec![TokenConfig {
                token: "admin-token".to_string(),
                name: "admin".to_string(),
                scopes: vec!["read".to_string(), "write".to_string(), "admin".to_string()],
            }],
        };

        let tokens = config.to_api_tokens();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, "admin-token");
        assert_eq!(tokens[0].scopes.len(), 3);
    }

    #[test]
    fn test_knowledge_config_defaults() {
        let config = KnowledgeConfig::default();
        assert!(!config.enabled);
        assert!(config.sources.is_empty());

        assert!(config.repo_path.is_empty());
        assert!(config.invite_codes.is_empty());
        assert_eq!(config.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_knowledge_config_compat_mode() {
        // 旧配置自动转为单源
        let config = KnowledgeConfig {
            enabled: true,
            repo_path: "/opt/knowledge".to_string(),
            invite_codes: vec!["test-code".to_string()],
            base_url: "https://api.example.com".to_string(),
            sync_token: "my-sync-token".to_string(),
            sources: vec![],
        };
        let sources = config.resolved_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "private");
        assert_eq!(sources[0].repo_path, "/opt/knowledge");
        assert_eq!(sources[0].invite_codes.len(), 1);
    }
    #[test]
    fn test_knowledge_config_multi_source() {
        let config = KnowledgeConfig {
            enabled: true,
            base_url: "https://link.opendev.dev".to_string(),
            sources: vec![
                KnowledgeSource {
                    name: "private".to_string(),
                    display_name: "OpenClaw私有".to_string(),
                    short_code: "0".to_string(),
                    repo_path: "/opt/ks-private".to_string(),
                    invite_codes: vec!["private-code".to_string()],
                    sync_token: "sync-priv".to_string(),
                },
                KnowledgeSource {
                    name: "public".to_string(),
                    display_name: "OpenClaw公开".to_string(),
                    short_code: "1".to_string(),
                    repo_path: "/opt/ks-public".to_string(),
                    invite_codes: vec!["public-code".to_string()],
                    sync_token: String::new(),
                },
            ],
            repo_path: String::new(),
            invite_codes: vec![],
            sync_token: String::new(),
        };
        let sources = config.resolved_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].name, "private");
        assert_eq!(sources[1].name, "public");
    }
    #[test]
    fn test_find_source_by_code() {
        let config = KnowledgeConfig {
            enabled: true,
            base_url: "http://localhost:3000".to_string(),
            sources: vec![
                KnowledgeSource {
                    name: "private".to_string(),
                    display_name: String::new(),
                    short_code: "0".to_string(),
                    repo_path: "/priv".to_string(),
                    invite_codes: vec!["code-priv".to_string()],
                    sync_token: String::new(),
                },
                KnowledgeSource {
                    name: "public".to_string(),
                    display_name: String::new(),
                    short_code: "1".to_string(),
                    repo_path: "/pub".to_string(),
                    invite_codes: vec!["code-pub".to_string()],
                    sync_token: String::new(),
                },
            ],
            repo_path: String::new(),
            invite_codes: vec![],
            sync_token: String::new(),
        };
        let found = config.find_source_by_code("code-pub").unwrap();
        assert_eq!(found.name, "public");
        assert_eq!(found.repo_path, "/pub");
        assert!(config.find_source_by_code("nope").is_none());
    }

    #[test]
    fn test_knowledge_config_custom() {
        let config = KnowledgeConfig {
            enabled: true,
            base_url: "https://api.example.com".to_string(),
            sources: vec![],
            repo_path: "/opt/knowledge".to_string(),
            invite_codes: vec!["test-code-1".to_string(), "test-code-2".to_string()],
            sync_token: String::new(),
        };
        assert!(config.enabled);
        assert_eq!(config.repo_path, "/opt/knowledge");
        assert_eq!(config.invite_codes.len(), 2);
        assert_eq!(config.base_url, "https://api.example.com");
    }

    #[test]
    fn test_app_config_with_knowledge() {
        let mut config = AppConfig::default();
        config.knowledge = KnowledgeConfig {
            enabled: true,
            base_url: "https://api.example.com".to_string(),
            sources: vec![
                KnowledgeSource {
                    name: "test".to_string(),
                    display_name: String::new(),
                    short_code: "t".to_string(),
                    repo_path: "/opt/test".to_string(),
                    invite_codes: vec!["test-code".to_string()],
                    sync_token: String::new(),
                },
            ],
            repo_path: String::new(),
            invite_codes: vec![],
            sync_token: String::new(),
        };
        assert!(config.knowledge.enabled);
        assert_eq!(config.knowledge.invite_codes.len(), 1);
    }
}
