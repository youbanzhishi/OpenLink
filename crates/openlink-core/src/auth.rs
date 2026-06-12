//! # Auth 增强模块 — 认证提供者
//!
//! Phase 7: 安全加固
//!
//! - `AuthProvider` trait: 认证提供者接口
//! - `ApiKeyAuth`: API Key 认证
//! - `JwtAuth`: JWT 令牌认证（验证签名 + 过期检查）
//! - `AuthMiddleware`: HTTP 中间件，未认证 → 401

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ─── AuthProvider Trait ─────────────────────────────────────

/// 认证提供者接口
///
/// 所有认证方式（API Key、JWT、OAuth2 等）都实现此 trait。
/// 多个 Provider 可以组合使用（OR 逻辑：任一通过即可）。
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// 验证凭证，返回认证结果
    async fn authenticate(&self, credentials: &Credentials) -> AuthResult;

    /// 认证提供者名称
    fn provider_name(&self) -> &str;

    /// 是否启用
    fn is_enabled(&self) -> bool;
}

/// 认证凭证 — 统一的凭证表示
#[derive(Debug, Clone)]
pub enum Credentials {
    /// Bearer Token（API Key 或 JWT）
    BearerToken(String),
    /// API Key（Header 或 Query 参数）
    ApiKey(String),
    /// Basic Auth（username:password）
    Basic { username: String, password: String },
    /// 自定义凭证
    Custom(HashMap<String, String>),
}

/// 认证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    /// 是否认证成功
    pub authenticated: bool,
    /// 认证主体标识（用户 ID / Key 名称等）
    pub identity: Option<String>,
    /// 权限范围
    pub scopes: Vec<String>,
    /// 认证失败原因
    pub reason: Option<String>,
    /// 凭证过期时间（Unix 时间戳），仅 JWT 有效
    pub expires_at: Option<i64>,
    /// 额外元数据
    pub metadata: HashMap<String, String>,
}

impl AuthResult {
    /// 认证成功
    pub fn success(identity: &str, scopes: Vec<String>) -> Self {
        Self {
            authenticated: true,
            identity: Some(identity.to_string()),
            scopes,
            reason: None,
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    /// 认证成功（带过期时间）
    pub fn success_with_expiry(identity: &str, scopes: Vec<String>, expires_at: i64) -> Self {
        Self {
            authenticated: true,
            identity: Some(identity.to_string()),
            scopes,
            reason: None,
            expires_at: Some(expires_at),
            metadata: HashMap::new(),
        }
    }

    /// 认证失败
    pub fn failure(reason: &str) -> Self {
        Self {
            authenticated: false,
            identity: None,
            scopes: vec![],
            reason: Some(reason.to_string()),
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    /// 检查是否有指定权限
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope || s == "admin")
    }
}

// ─── ApiKeyAuth ─────────────────────────────────────────────

/// API Key 认证提供者
///
/// 从请求的 Header 或 Bearer Token 中提取 API Key，
/// 与配置的 Key 列表进行匹配。
pub struct ApiKeyAuth {
    /// 已配置的 API Key 列表
    keys: HashMap<String, ApiKeyEntry>,
    /// 是否启用
    enabled: bool,
}

/// API Key 条目
#[derive(Debug, Clone)]
struct ApiKeyEntry {
    /// Key 名称
    name: String,
    /// 权限范围
    scopes: Vec<String>,
    /// 是否启用
    active: bool,
}

impl ApiKeyAuth {
    /// 创建 API Key 认证提供者
    pub fn new(enabled: bool) -> Self {
        Self {
            keys: HashMap::new(),
            enabled,
        }
    }

    /// 添加一个 API Key
    pub fn add_key(&mut self, key: &str, name: &str, scopes: Vec<String>) {
        self.keys.insert(
            key.to_string(),
            ApiKeyEntry {
                name: name.to_string(),
                scopes,
                active: true,
            },
        );
    }

    /// 从配置条目批量添加 Key
    pub fn add_keys(&mut self, entries: Vec<ApiKeyConfig>) {
        for entry in entries {
            self.keys.insert(
                entry.key,
                ApiKeyEntry {
                    name: entry.name,
                    scopes: entry.scopes,
                    active: entry.active,
                },
            );
        }
    }

    /// 禁用一个 Key
    pub fn revoke_key(&mut self, key: &str) {
        if let Some(entry) = self.keys.get_mut(key) {
            entry.active = false;
        }
    }

    /// 列出所有 Key（不暴露 Key 值）
    pub fn list_keys(&self) -> Vec<(String, Vec<String>, bool)> {
        self.keys
            .values()
            .map(|e| (e.name.clone(), e.scopes.clone(), e.active))
            .collect()
    }
}

/// API Key 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// Key 值
    pub key: String,
    /// Key 名称
    pub name: String,
    /// 权限范围
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 是否启用
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

#[async_trait]
impl AuthProvider for ApiKeyAuth {
    async fn authenticate(&self, credentials: &Credentials) -> AuthResult {
        if !self.enabled {
            // 未启用时，返回 admin 权限
            return AuthResult::success("anonymous", vec!["admin".to_string()]);
        }

        let key_str = match credentials {
            Credentials::BearerToken(token) => token.as_str(),
            Credentials::ApiKey(key) => key.as_str(),
            _ => {
                return AuthResult::failure("Unsupported credential type for API Key auth");
            }
        };

        match self.keys.get(key_str) {
            Some(entry) if entry.active => AuthResult::success(&entry.name, entry.scopes.clone()),
            Some(_) => AuthResult::failure("API Key has been revoked"),
            None => AuthResult::failure("Invalid API Key"),
        }
    }

    fn provider_name(&self) -> &str {
        "api_key"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

// ─── JwtAuth ────────────────────────────────────────────────

/// JWT 认证提供者
///
/// 验证 JWT 签名 + 过期检查。
/// 支持 HMAC-SHA256 对称签名验证。
///
/// 注意：此实现为无外部 JWT 依赖的轻量版本。
/// 生产环境建议使用 `jsonwebtoken` crate。
pub struct JwtAuth {
    /// HMAC 签名密钥
    secret: String,
    /// 签名算法
    #[allow(dead_code)]
    algorithm: JwtAlgorithm,
    /// 是否启用
    enabled: bool,
    /// 允许的发行者
    allowed_issuers: Vec<String>,
    /// 允许的受众
    allowed_audiences: Vec<String>,
}

/// JWT 签名算法
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
#[derive(Default)]
pub enum JwtAlgorithm {
    /// HMAC SHA-256
    HS256,
    /// HMAC SHA-384
    HS384,
    /// HMAC SHA-512
    HS512,
}

/// JWT 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// 签名密钥
    pub secret: String,
    /// 签名算法
    #[serde(default)]
    pub algorithm: JwtAlgorithm,
    /// 是否启用
    #[serde(default)]
    pub enabled: bool,
    /// 允许的发行者
    #[serde(default)]
    pub allowed_issuers: Vec<String>,
    /// 允许的受众
    #[serde(default)]
    pub allowed_audiences: Vec<String>,
}

impl JwtAuth {
    /// 创建 JWT 认证提供者
    pub fn new(secret: &str, algorithm: JwtAlgorithm, enabled: bool) -> Self {
        Self {
            secret: secret.to_string(),
            algorithm,
            enabled,
            allowed_issuers: vec![],
            allowed_audiences: vec![],
        }
    }

    /// 从配置创建
    pub fn from_config(config: &JwtConfig) -> Self {
        Self {
            secret: config.secret.clone(),
            algorithm: config.algorithm.clone(),
            enabled: config.enabled,
            allowed_issuers: config.allowed_issuers.clone(),
            allowed_audiences: config.allowed_audiences.clone(),
        }
    }

    /// 解码 JWT 的 payload 部分（不验证签名）
    fn decode_payload(&self, token: &str) -> Option<JwtPayload> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        // Base64url decode payload
        let payload_b64 = parts[1];
        let payload_bytes = base64url_decode(payload_b64)?;
        let payload_str = String::from_utf8(payload_bytes).ok()?;
        serde_json::from_str(&payload_str).ok()
    }

    /// 验证 JWT 签名
    fn verify_signature(&self, token: &str) -> bool {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return false;
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let signature_b64 = parts[2];

        // Compute expected signature
        let expected = self.compute_signature(&signing_input);
        let expected_b64 = base64url_encode(&expected);

        // Constant-time comparison
        constant_time_eq(&expected_b64, signature_b64)
    }

    /// 计算签名
    fn compute_signature(&self, input: &str) -> Vec<u8> {
        // Simple HMAC computation using sha2 crate or fallback
        // For now, use a basic HMAC-SHA256 implementation
        let key_bytes = self.secret.as_bytes();
        let input_bytes = input.as_bytes();

        // HMAC-SHA256
        let mut hmac_input = Vec::new();
        hmac_input.extend_from_slice(input_bytes);
        hmac_simple_sha256(key_bytes, &hmac_input)
    }

    /// 验证 JWT 完整性（签名 + 过期 + 发行者 + 受众）
    fn validate_token(&self, token: &str) -> AuthResult {
        // Step 1: Verify signature
        if !self.verify_signature(token) {
            return AuthResult::failure("Invalid JWT signature");
        }

        // Step 2: Decode payload
        let payload = match self.decode_payload(token) {
            Some(p) => p,
            None => return AuthResult::failure("Invalid JWT payload"),
        };

        // Step 3: Check expiration
        if let Some(exp) = payload.exp {
            let now = chrono::Utc::now().timestamp();
            if now > exp {
                return AuthResult::failure("JWT token has expired");
            }
        } else {
            // Tokens without expiration are rejected
            return AuthResult::failure("JWT token missing expiration");
        }

        // Step 4: Check not-before
        if let Some(nbf) = payload.nbf {
            let now = chrono::Utc::now().timestamp();
            if now < nbf {
                return AuthResult::failure("JWT token not yet valid");
            }
        }

        // Step 5: Check issuer
        if !self.allowed_issuers.is_empty() {
            if let Some(ref iss) = payload.iss {
                if !self.allowed_issuers.contains(iss) {
                    return AuthResult::failure("JWT issuer not allowed");
                }
            } else {
                return AuthResult::failure("JWT missing required issuer");
            }
        }

        // Step 6: Check audience
        if !self.allowed_audiences.is_empty() {
            if let Some(ref aud) = payload.aud {
                if !self.allowed_audiences.contains(aud) {
                    return AuthResult::failure("JWT audience not allowed");
                }
            } else {
                return AuthResult::failure("JWT missing required audience");
            }
        }

        // Success
        let identity = payload.sub.unwrap_or_else(|| "unknown".to_string());
        let scopes = payload.scopes.clone().unwrap_or_else(|| vec!["read".to_string()]);
        let expires_at = payload.exp.unwrap_or(0);

        AuthResult::success_with_expiry(&identity, scopes, expires_at)
    }
}

/// JWT Payload 结构
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JwtPayload {
    /// Subject（主体标识）
    #[serde(default)]
    sub: Option<String>,
    /// Issuer（发行者）
    #[serde(default)]
    iss: Option<String>,
    /// Audience（受众）
    #[serde(default)]
    aud: Option<String>,
    /// Expiration（过期时间，Unix 时间戳）
    #[serde(default)]
    exp: Option<i64>,
    /// Not Before（生效时间，Unix 时间戳）
    #[serde(default)]
    nbf: Option<i64>,
    /// Issued At（签发时间，Unix 时间戳）
    #[serde(default)]
    iat: Option<i64>,
    /// JWT ID（唯一标识）
    #[serde(default)]
    jti: Option<String>,
    /// 自定义权限范围
    #[serde(default)]
    scopes: Option<Vec<String>>,
}

#[async_trait]
impl AuthProvider for JwtAuth {
    async fn authenticate(&self, credentials: &Credentials) -> AuthResult {
        if !self.enabled {
            return AuthResult::success("anonymous", vec!["admin".to_string()]);
        }

        let token = match credentials {
            Credentials::BearerToken(token) => token.as_str(),
            Credentials::ApiKey(key) => key.as_str(), // Also accept raw key as JWT
            _ => {
                return AuthResult::failure("Unsupported credential type for JWT auth");
            }
        };

        self.validate_token(token)
    }

    fn provider_name(&self) -> &str {
        "jwt"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

// ─── AuthMiddleware ─────────────────────────────────────────

/// 认证中间件 — HTTP 请求认证
///
/// 支持多个 AuthProvider 组合（OR 逻辑：任一通过即可）。
/// 未认证请求返回 401。
pub struct AuthMiddleware {
    providers: Vec<Arc<dyn AuthProvider>>,
    /// 是否全局启用（至少一个 Provider 启用）
    enabled: bool,
}

impl AuthMiddleware {
    /// 创建认证中间件
    pub fn new(providers: Vec<Arc<dyn AuthProvider>>) -> Self {
        let enabled = providers.iter().any(|p| p.is_enabled());
        Self { providers, enabled }
    }

    /// 从 Header 中提取凭证
    pub fn extract_credentials(auth_header: Option<&str>, api_key_header: Option<&str>) -> Option<Credentials> {
        // 1. Try Authorization header
        if let Some(auth) = auth_header {
            let auth = auth.trim();
            if auth.starts_with("Bearer ") {
                return auth
                    .strip_prefix("Bearer ")
                    .map(|t| Credentials::BearerToken(t.to_string()));
            }
            if auth.starts_with("bearer ") {
                return auth
                    .strip_prefix("Bearer ")
                    .map(|t| Credentials::BearerToken(t.to_string()));
            }
            if auth.starts_with("Basic ") {
                let decoded = auth
                    .strip_prefix("Basic ")
                    .map(|s| base64url_decode(s.trim()))
                    .unwrap_or_else(|| base64url_decode(auth[6..].trim()));
                if let Some(bytes) = decoded {
                    if let Ok(s) = String::from_utf8(bytes) {
                        let parts: Vec<&str> = s.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            return Some(Credentials::Basic {
                                username: parts[0].to_string(),
                                password: parts[1].to_string(),
                            });
                        }
                    }
                }
            }
        }

        // 2. Try X-API-Key header
        if let Some(key) = api_key_header {
            return Some(Credentials::ApiKey(key.trim().to_string()));
        }

        None
    }

    /// 验证请求
    pub async fn authenticate(&self, credentials: &Credentials) -> AuthResult {
        if !self.enabled {
            return AuthResult::success("anonymous", vec!["admin".to_string()]);
        }

        // Try each provider (OR logic)
        let mut last_failure = AuthResult::failure("No auth providers configured");

        for provider in &self.providers {
            if !provider.is_enabled() {
                continue;
            }
            let result = provider.authenticate(credentials).await;
            if result.authenticated {
                return result;
            }
            last_failure = result;
        }

        last_failure
    }

    /// 检查是否需要认证（是否有 Provider 启用）
    pub fn requires_auth(&self) -> bool {
        self.enabled
    }

    /// 添加 Provider
    pub fn add_provider(&mut self, provider: Arc<dyn AuthProvider>) {
        if provider.is_enabled() {
            self.enabled = true;
        }
        self.providers.push(provider);
    }
}

// ─── Helper Functions ───────────────────────────────────────

/// Base64url 解码
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    // Replace URL-safe characters and add padding
    let mut s = input.replace('-', "+").replace('_', "/");
    let padding = (4 - s.len() % 4) % 4;
    for _ in 0..padding {
        s.push('=');
    }
    base64_decode(&s)
}

/// Base64url 编码
fn base64url_encode(input: &[u8]) -> String {
    let mut s = base64_encode(input);
    s = s.replace('+', "-").replace('/', "_");
    // Remove trailing padding
    while s.ends_with('=') {
        s.pop();
    }
    s
}

/// Simple base64 decode (no external dependency)
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let input = input.trim_end_matches('=');
    let mut result = Vec::with_capacity(input.len() * 3 / 4);

    let mut buffer: u32 = 0;
    let mut bits = 0;

    for ch in input.chars() {
        let val = TABLE.iter().position(|&b| b == ch as u8)?;
        buffer = (buffer << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buffer >> bits) as u8);
        }
    }

    Some(result)
}

/// Simple base64 encode (no external dependency)
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i + 2 < input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        result.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        result.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        result.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }

    if i + 1 < input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        result.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        result.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        result.push('=');
    } else if i < input.len() {
        let n = (input[i] as u32) << 16;
        result.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        result.push_str("==");
    }

    result
}

/// Constant-time string comparison (to prevent timing attacks)
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// Simple HMAC-SHA256 (minimal implementation for JWT verification)
/// In production, use the `hmac` + `sha2` crates.
fn hmac_simple_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    // HMAC(K, m) = H((K' ⊕ opad) || H((K' ⊕ ipad) || m))
    // K' = H(K) if len(K) > block_size, else K padded with zeros
    const BLOCK_SIZE: usize = 64; // SHA-256 block size

    let key_padded = if key.len() > BLOCK_SIZE {
        let mut padded = sha256_simple(key);
        padded.resize(BLOCK_SIZE, 0);
        padded
    } else {
        let mut padded = key.to_vec();
        padded.resize(BLOCK_SIZE, 0);
        padded
    };

    // Inner: (K' ⊕ ipad) || m
    let mut inner_data = Vec::with_capacity(BLOCK_SIZE + message.len());
    for (i, &k) in key_padded.iter().enumerate() {
        inner_data.push(k ^ 0x36);
    }
    inner_data.extend_from_slice(message);
    let inner_hash = sha256_simple(&inner_data);

    // Outer: (K' ⊕ opad) || inner_hash
    let mut outer_data = Vec::with_capacity(BLOCK_SIZE + 32);
    for (i, &k) in key_padded.iter().enumerate() {
        outer_data.push(k ^ 0x5C);
    }
    outer_data.extend_from_slice(&inner_hash);

    sha256_simple(&outer_data)
}

/// Minimal SHA-256 implementation for JWT signature verification
fn sha256_simple(data: &[u8]) -> Vec<u8> {
    // SHA-256 constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    // Initial hash values
    let mut h0: u32 = 0x6a09e667;
    let mut h1: u32 = 0xbb67ae85;
    let mut h2: u32 = 0x3c6ef372;
    let mut h3: u32 = 0xa54ff53a;
    let mut h4: u32 = 0x510e527f;
    let mut h5: u32 = 0x9b05688c;
    let mut h6: u32 = 0x1f83d9ab;
    let mut h7: u32 = 0x5be0cd19;

    // Pre-processing: adding padding bits
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit block
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;
        let mut f = h5;
        let mut g = h6;
        let mut hh = h7;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
        h5 = h5.wrapping_add(f);
        h6 = h6.wrapping_add(g);
        h7 = h7.wrapping_add(hh);
    }

    let mut result = Vec::with_capacity(32);
    for h in &[h0, h1, h2, h3, h4, h5, h6, h7] {
        result.extend_from_slice(&h.to_be_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64url_decode_encode() {
        let original = b"hello world";
        let encoded = base64url_encode(original);
        let decoded = base64url_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_base64_decode() {
        // "hello" in base64 is "aGVsbG8="
        let decoded = base64_decode("aGVsbG8=").unwrap();
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn test_base64_encode() {
        let encoded = base64_encode(b"hello");
        assert_eq!(encoded, "aGVsbG8=");
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
    }

    #[tokio::test]
    async fn test_api_key_auth() {
        let mut auth = ApiKeyAuth::new(true);
        auth.add_key(
            "secret-key-123",
            "test-user",
            vec!["read".to_string(), "write".to_string()],
        );

        let creds = Credentials::BearerToken("secret-key-123".to_string());
        let result = auth.authenticate(&creds).await;
        assert!(result.authenticated);
        assert_eq!(result.identity, Some("test-user".to_string()));
        assert!(result.has_scope("read"));
    }

    #[tokio::test]
    async fn test_api_key_auth_invalid() {
        let mut auth = ApiKeyAuth::new(true);
        auth.add_key("secret-key-123", "test-user", vec!["read".to_string()]);

        let creds = Credentials::BearerToken("wrong-key".to_string());
        let result = auth.authenticate(&creds).await;
        assert!(!result.authenticated);
    }

    #[tokio::test]
    async fn test_api_key_auth_disabled() {
        let auth = ApiKeyAuth::new(false);
        let creds = Credentials::BearerToken("any-key".to_string());
        let result = auth.authenticate(&creds).await;
        assert!(result.authenticated); // Disabled = anonymous admin
    }

    #[tokio::test]
    async fn test_api_key_auth_revoked() {
        let mut auth = ApiKeyAuth::new(true);
        auth.add_key("secret-key-123", "test-user", vec!["read".to_string()]);
        auth.revoke_key("secret-key-123");

        let creds = Credentials::BearerToken("secret-key-123".to_string());
        let result = auth.authenticate(&creds).await;
        assert!(!result.authenticated);
    }

    #[tokio::test]
    async fn test_auth_middleware_extract_bearer() {
        let creds = AuthMiddleware::extract_credentials(Some("Bearer my-token"), None);
        assert!(matches!(creds, Some(Credentials::BearerToken(t)) if t == "my-token"));
    }

    #[tokio::test]
    async fn test_auth_middleware_extract_api_key() {
        let creds = AuthMiddleware::extract_credentials(None, Some("my-api-key"));
        assert!(matches!(creds, Some(Credentials::ApiKey(k)) if k == "my-api-key"));
    }

    #[tokio::test]
    async fn test_auth_middleware_extract_none() {
        let creds = AuthMiddleware::extract_credentials(None, None);
        assert!(creds.is_none());
    }

    #[test]
    fn test_auth_result_has_scope() {
        let result = AuthResult::success("user", vec!["read".to_string(), "write".to_string()]);
        assert!(result.has_scope("read"));
        assert!(result.has_scope("write"));
        assert!(!result.has_scope("admin"));
    }

    #[test]
    fn test_auth_result_admin_scope() {
        let result = AuthResult::success("admin", vec!["admin".to_string()]);
        assert!(result.has_scope("read")); // admin implies all
        assert!(result.has_scope("write"));
        assert!(result.has_scope("admin"));
    }

    #[test]
    fn test_jwt_config_deserialize() {
        let json = r#"{"secret":"my-secret","algorithm":"HS256","enabled":true}"#;
        let config: JwtConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.secret, "my-secret");
        assert_eq!(config.algorithm, JwtAlgorithm::HS256);
        assert!(config.enabled);
    }
}
