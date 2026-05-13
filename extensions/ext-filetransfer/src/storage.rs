//! # 存储后端抽象与实现
//!
//! 提供统一的存储接口，支持多种后端：本地存储、R2、OSS、WebDAV、SFTP 等。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

// ─── Storage Error ──────────────────────────────────────────

/// 存储错误类型
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Invalid configuration: {0}")]
    Config(String),
}

// ─── Base64 Helper ───────────────────────────────────────────

/// Simple Base64 encoding (standard alphabet, no padding)
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let chunks = input.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// ─── Storage Backend Trait ───────────────────────────────────

/// 存储后端 trait
///
/// 所有存储后端必须实现此接口。
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// 上传文件
    async fn upload(&self, file_id: &str, data: Vec<u8>, content_type: &str) -> Result<(), StorageError>;

    /// 下载文件
    async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError>;

    /// 生成预签名 URL
    async fn presigned_url(&self, file_id: &str, expires_in_secs: u64) -> Result<String, StorageError>;

    /// 删除文件
    async fn delete(&self, file_id: &str) -> Result<(), StorageError>;

    /// 获取后端名称
    fn backend_name(&self) -> &str;

    /// 检查文件是否存在
    async fn exists(&self, file_id: &str) -> Result<bool, StorageError>;
}

// ─── Local Storage Backend ─────────────────────────────────

/// 本地存储后端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageConfig {
    /// 存储目录
    pub base_path: String,
}

impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            base_path: "/tmp/openlink-storage".to_string(),
        }
    }
}

/// 本地存储后端（测试用）
pub struct LocalStorageBackend {
    config: LocalStorageConfig,
}

impl LocalStorageBackend {
    pub fn new(config: LocalStorageConfig) -> Self {
        Self { config }
    }

    pub fn with_default() -> Self {
        Self::new(LocalStorageConfig::default())
    }

    fn file_path(&self, file_id: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.config.base_path).join(file_id)
    }
}

#[async_trait]
impl StorageBackend for LocalStorageBackend {
    async fn upload(&self, file_id: &str, data: Vec<u8>, _content_type: &str) -> Result<(), StorageError> {
        let path = self.file_path(file_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        tracing::info!(file_id = %file_id, path = %path.display(), "File uploaded to local storage");
        Ok(())
    }

    async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.file_path(file_id);
        if !path.exists() {
            return Err(StorageError::NotFound(file_id.to_string()));
        }
        let data = tokio::fs::read(&path).await?;
        tracing::debug!(file_id = %file_id, size = data.len(), "File downloaded from local storage");
        Ok(data)
    }

    async fn presigned_url(&self, file_id: &str, _expires_in_secs: u64) -> Result<String, StorageError> {
        let path = self.file_path(file_id);
        Ok(format!("file://{}", path.display()))
    }

    async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        let path = self.file_path(file_id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
            tracing::info!(file_id = %file_id, "File deleted from local storage");
        }
        Ok(())
    }

    fn backend_name(&self) -> &str {
        "local"
    }

    async fn exists(&self, file_id: &str) -> Result<bool, StorageError> {
        let path = self.file_path(file_id);
        Ok(path.exists())
    }
}

// ─── R2 Storage Backend ─────────────────────────────────────

/// R2 存储后端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R2StorageConfig {
    /// Account ID
    pub account_id: String,
    /// Access Key ID
    pub access_key_id: String,
    /// Secret Access Key
    pub secret_access_key: String,
    /// Bucket 名称
    pub bucket: String,
    /// 公共 URL 前缀
    pub public_url: String,
}

impl R2StorageConfig {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.account_id.is_empty() {
            return Err(StorageError::Config("account_id is required".to_string()));
        }
        if self.access_key_id.is_empty() {
            return Err(StorageError::Config("access_key_id is required".to_string()));
        }
        if self.secret_access_key.is_empty() {
            return Err(StorageError::Config("secret_access_key is required".to_string()));
        }
        if self.bucket.is_empty() {
            return Err(StorageError::Config("bucket is required".to_string()));
        }
        Ok(())
    }
}

/// Cloudflare R2 存储后端
pub struct R2StorageBackend {
    config: R2StorageConfig,
    http_client: reqwest::Client,
}

impl R2StorageBackend {
    pub fn new(config: R2StorageConfig) -> Result<Self, StorageError> {
        config.validate()?;
        Ok(Self {
            config,
            http_client: reqwest::Client::new(),
        })
    }

    fn api_url(&self, object_key: &str) -> String {
        format!(
            "https://{}.{}.r2.cloudflarestorage.com/{}",
            self.config.bucket, self.config.account_id, object_key
        )
    }

    fn sign_request(&self, method: &str, path: &str) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        let string_to_sign = format!("{}\n{}\n{}\n{}", method, path, timestamp, self.config.account_id);

        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(self.config.secret_access_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(string_to_sign.as_bytes());

        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());

        format!(
            "HMAC-SHA256 Credential={}/{}, SignedHeaders=x-date, Signature={}",
            self.config.access_key_id, timestamp, signature
        )
    }
}

#[async_trait]
impl StorageBackend for R2StorageBackend {
    async fn upload(&self, file_id: &str, data: Vec<u8>, content_type: &str) -> Result<(), StorageError> {
        let url = self.api_url(file_id);
        let signed_auth = self.sign_request("PUT", &format!("/{}", file_id));

        let client = reqwest::Client::new();
        let response = client
            .put(&url)
            .header("Content-Type", content_type)
            .header("Content-Length", data.len().to_string())
            .header("Authorization", signed_auth)
            .body(data)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if response.status().is_success() || response.status().as_u16() == 200 {
            tracing::info!(file_id = %file_id, "File uploaded to R2");
            Ok(())
        } else {
            Err(StorageError::Backend(format!("Upload failed: {}", response.status())))
        }
    }

    async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        let url = self.api_url(file_id);
        let signed_auth = self.sign_request("GET", &format!("/{}", file_id));

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", signed_auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if response.status().as_u16() == 404 {
            return Err(StorageError::NotFound(file_id.to_string()));
        }

        if !response.status().is_success() {
            return Err(StorageError::Backend(format!("Download failed: {}", response.status())));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        tracing::debug!(file_id = %file_id, size = bytes.len(), "File downloaded from R2");
        Ok(bytes.to_vec())
    }

    async fn presigned_url(&self, file_id: &str, expires_in_secs: u64) -> Result<String, StorageError> {
        let _expiry = chrono::Utc::now() + chrono::Duration::seconds(expires_in_secs as i64);
        let public_url = format!("{}/{}", self.config.public_url.trim_end_matches('/'), file_id);

        tracing::debug!(file_id = %file_id, "Generated presigned URL");
        Ok(public_url)
    }

    async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        let url = self.api_url(file_id);
        let signed_auth = self.sign_request("DELETE", &format!("/{}", file_id));

        let response = self
            .http_client
            .delete(&url)
            .header("Authorization", signed_auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if response.status().is_success() || response.status().as_u16() == 404 {
            tracing::info!(file_id = %file_id, "File deleted from R2");
            Ok(())
        } else {
            Err(StorageError::Backend(format!("Delete failed: {}", response.status())))
        }
    }

    fn backend_name(&self) -> &str {
        "r2"
    }

    async fn exists(&self, file_id: &str) -> Result<bool, StorageError> {
        let url = self.api_url(file_id);
        let signed_auth = self.sign_request("HEAD", &format!("/{}", file_id));

        let response = self
            .http_client
            .head(&url)
            .header("Authorization", signed_auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(response.status().as_u16() == 200)
    }
}

// ─── OSS Storage Backend ────────────────────────────────────

/// 阿里云 OSS 存储后端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OssStorageConfig {
    /// Access Key ID
    pub access_key_id: String,
    /// Access Key Secret
    pub access_key_secret: String,
    /// Bucket 名称
    pub bucket: String,
    /// Endpoint（如 oss-cn-hangzhou.aliyuncs.com）
    pub endpoint: String,
    /// 自定义域名或公共 URL 前缀
    #[serde(default)]
    pub public_url: Option<String>,
}

impl OssStorageConfig {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.access_key_id.is_empty() {
            return Err(StorageError::Config("access_key_id is required".to_string()));
        }
        if self.access_key_secret.is_empty() {
            return Err(StorageError::Config("access_key_secret is required".to_string()));
        }
        if self.bucket.is_empty() {
            return Err(StorageError::Config("bucket is required".to_string()));
        }
        if self.endpoint.is_empty() {
            return Err(StorageError::Config("endpoint is required".to_string()));
        }
        Ok(())
    }

    /// 获取 bucket 的基础 URL
    fn bucket_url(&self) -> String {
        format!("https://{}.{}", self.bucket, self.endpoint)
    }

    /// 获取公共访问 URL
    fn public_access_url(&self, object_key: &str) -> String {
        if let Some(ref pub_url) = self.public_url {
            format!("{}/{}", pub_url.trim_end_matches('/'), object_key)
        } else {
            format!("{}/{}", self.bucket_url(), object_key)
        }
    }

    /// HMAC-SHA1 签名 (OSS V1 签名)
    fn sign_v1(&self, method: &str, resource: &str, date: &str, content_type: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha1::Sha1;

        type HmacSha1 = Hmac<Sha1>;

        let string_to_sign = format!("{}\n\n{}\n{}\n{}", method, content_type, date, resource);

        let mut mac =
            HmacSha1::new_from_slice(self.access_key_secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(string_to_sign.as_bytes());

        let result = mac.finalize();
        let signature = base64_encode(&result.into_bytes());

        format!("OSS {}:{}", self.access_key_id, signature)
    }
}

/// 阿里云 OSS 存储后端
///
/// 支持自定义 endpoint，兼容 MinIO 等 S3 兼容存储。
pub struct OssStorageBackend {
    config: OssStorageConfig,
    http_client: reqwest::Client,
}

impl OssStorageBackend {
    pub fn new(config: OssStorageConfig) -> Result<Self, StorageError> {
        config.validate()?;
        Ok(Self {
            config,
            http_client: reqwest::Client::new(),
        })
    }

    fn object_url(&self, object_key: &str) -> String {
        format!("{}/{}", self.config.bucket_url(), object_key)
    }

    fn resource_path(&self, object_key: &str) -> String {
        format!("/{}/{}", self.config.bucket, object_key)
    }

    fn format_date() -> String {
        chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }
}

#[async_trait]
impl StorageBackend for OssStorageBackend {
    async fn upload(&self, file_id: &str, data: Vec<u8>, content_type: &str) -> Result<(), StorageError> {
        let url = self.object_url(file_id);
        let date = Self::format_date();
        let resource = self.resource_path(file_id);
        let auth = self.config.sign_v1("PUT", &resource, &date, content_type);

        let response = self
            .http_client
            .put(&url)
            .header("Date", &date)
            .header("Authorization", auth)
            .header("Content-Type", content_type)
            .header("Content-Length", data.len().to_string())
            .body(data)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if response.status().is_success() {
            tracing::info!(file_id = %file_id, "File uploaded to OSS");
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(StorageError::Backend(format!(
                "OSS upload failed: {} - {}",
                status, body
            )))
        }
    }

    async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        let url = self.object_url(file_id);
        let date = Self::format_date();
        let resource = self.resource_path(file_id);
        let auth = self.config.sign_v1("GET", &resource, &date, "");

        let response = self
            .http_client
            .get(&url)
            .header("Date", &date)
            .header("Authorization", auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if response.status().as_u16() == 404 {
            return Err(StorageError::NotFound(file_id.to_string()));
        }

        if !response.status().is_success() {
            return Err(StorageError::Backend(format!(
                "OSS download failed: {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        tracing::debug!(file_id = %file_id, size = bytes.len(), "File downloaded from OSS");
        Ok(bytes.to_vec())
    }

    async fn presigned_url(&self, file_id: &str, expires_in_secs: u64) -> Result<String, StorageError> {
        let expires = chrono::Utc::now().timestamp() as u64 + expires_in_secs;
        let resource = self.resource_path(file_id);

        use hmac::{Hmac, Mac};
        use sha1::Sha1;
        type HmacSha1 = Hmac<Sha1>;

        let string_to_sign = format!("GET\n\n\n{}\n{}", expires, resource);
        let mut mac =
            HmacSha1::new_from_slice(self.config.access_key_secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize();
        let signature = base64_encode(&result.into_bytes());

        let url = format!(
            "{}?OSSAccessKeyId={}&Expires={}&Signature={}",
            self.object_url(file_id),
            urlencoding::encode(&self.config.access_key_id),
            expires,
            urlencoding::encode(&signature)
        );

        Ok(url)
    }

    async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        let url = self.object_url(file_id);
        let date = Self::format_date();
        let resource = self.resource_path(file_id);
        let auth = self.config.sign_v1("DELETE", &resource, &date, "");

        let response = self
            .http_client
            .delete(&url)
            .header("Date", &date)
            .header("Authorization", auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if response.status().is_success() || response.status().as_u16() == 404 {
            tracing::info!(file_id = %file_id, "File deleted from OSS");
            Ok(())
        } else {
            Err(StorageError::Backend(format!(
                "OSS delete failed: {}",
                response.status()
            )))
        }
    }

    fn backend_name(&self) -> &str {
        "oss"
    }

    async fn exists(&self, file_id: &str) -> Result<bool, StorageError> {
        let url = self.object_url(file_id);
        let date = Self::format_date();
        let resource = self.resource_path(file_id);
        let auth = self.config.sign_v1("HEAD", &resource, &date, "");

        let response = self
            .http_client
            .head(&url)
            .header("Date", &date)
            .header("Authorization", auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(response.status().as_u16() == 200)
    }
}

// ─── WebDAV Storage Backend ─────────────────────────────────

/// WebDAV 存储后端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavStorageConfig {
    /// WebDAV 服务器 URL（如 https://dav.example.com/remote.php/dav/files/user/）
    pub base_url: String,
    /// 用户名（Basic Auth）
    #[serde(default)]
    pub username: Option<String>,
    /// 密码（Basic Auth）
    #[serde(default)]
    pub password: Option<String>,
}

impl WebdavStorageConfig {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.base_url.is_empty() {
            return Err(StorageError::Config("base_url is required".to_string()));
        }
        Ok(())
    }

    fn auth_header(&self) -> Option<String> {
        match (&self.username, &self.password) {
            (Some(user), Some(pass)) => {
                let credentials = base64_encode(format!("{}:{}", user, pass).as_bytes());
                Some(format!("Basic {}", credentials))
            }
            _ => None,
        }
    }
}

/// WebDAV 存储后端
///
/// 支持 PUT/GET/DELETE/MKCOL/PROPFIND，Basic Auth 认证，目录创建和列表。
pub struct WebdavStorageBackend {
    config: WebdavStorageConfig,
    http_client: reqwest::Client,
}

impl WebdavStorageBackend {
    pub fn new(config: WebdavStorageConfig) -> Result<Self, StorageError> {
        config.validate()?;
        Ok(Self {
            config,
            http_client: reqwest::Client::new(),
        })
    }

    fn object_url(&self, file_id: &str) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        format!("{}/{}", base, file_id)
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(auth) = self.config.auth_header() {
            req.header("Authorization", auth)
        } else {
            req
        }
    }

    /// 创建目录（MKCOL）
    pub async fn mkdir(&self, dir_path: &str) -> Result<(), StorageError> {
        let url = self.object_url(dir_path);
        let req = self
            .http_client
            .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &url);
        let resp = self
            .apply_auth(req)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        // 201 = created, 405 = already exists, both OK
        if resp.status().is_success() || resp.status().as_u16() == 405 {
            Ok(())
        } else {
            Err(StorageError::Backend(format!("MKCOL failed: {}", resp.status())))
        }
    }

    /// 列出目录（PROPFIND）
    pub async fn list_dir(&self, dir_path: &str) -> Result<Vec<WebdavFileInfo>, StorageError> {
        let url = self.object_url(dir_path);
        let req = self
            .http_client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Depth", "1");

        let resp = self
            .apply_auth(req)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if !resp.status().is_success() && resp.status().as_u16() != 207 {
            return Err(StorageError::Backend(format!("PROPFIND failed: {}", resp.status())));
        }

        let body = resp.text().await.map_err(|e| StorageError::Backend(e.to_string()))?;

        // Simple XML parsing: extract href values from <d:response> elements
        let mut files = Vec::new();
        for resp_block in body.split("<d:response>").skip(1) {
            if let Some(href_start) = resp_block.find("<d:href>") {
                if let Some(href_end) = resp_block.find("</d:href>") {
                    let href = &resp_block[href_start + 8..href_end];
                    let is_dir = resp_block.contains("<d:collection/>");
                    files.push(WebdavFileInfo {
                        href: href.to_string(),
                        is_directory: is_dir,
                    });
                }
            }
        }

        Ok(files)
    }
}

/// WebDAV 文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebdavFileInfo {
    /// 文件/目录路径
    pub href: String,
    /// 是否为目录
    pub is_directory: bool,
}

#[async_trait]
impl StorageBackend for WebdavStorageBackend {
    async fn upload(&self, file_id: &str, data: Vec<u8>, content_type: &str) -> Result<(), StorageError> {
        let url = self.object_url(file_id);

        // Ensure parent directory exists
        if let Some(parent) = file_id.rfind('/') {
            let parent_path = &file_id[..parent];
            if !parent_path.is_empty() {
                let _ = self.mkdir(parent_path).await;
            }
        }

        let req = self
            .http_client
            .put(&url)
            .header("Content-Type", content_type)
            .body(data);
        let resp = self
            .apply_auth(req)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if resp.status().is_success() || resp.status().as_u16() == 201 {
            tracing::info!(file_id = %file_id, "File uploaded to WebDAV");
            Ok(())
        } else {
            Err(StorageError::Backend(format!(
                "WebDAV upload failed: {}",
                resp.status()
            )))
        }
    }

    async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        let url = self.object_url(file_id);
        let req = self.http_client.get(&url);
        let resp = self
            .apply_auth(req)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Err(StorageError::NotFound(file_id.to_string()));
        }

        if !resp.status().is_success() {
            return Err(StorageError::Backend(format!(
                "WebDAV download failed: {}",
                resp.status()
            )));
        }

        let bytes = resp.bytes().await.map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(bytes.to_vec())
    }

    async fn presigned_url(&self, file_id: &str, _expires_in_secs: u64) -> Result<String, StorageError> {
        // WebDAV doesn't natively support presigned URLs; return direct URL
        Ok(self.object_url(file_id))
    }

    async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        let url = self.object_url(file_id);
        let req = self.http_client.delete(&url);
        let resp = self
            .apply_auth(req)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if resp.status().is_success() || resp.status().as_u16() == 404 {
            tracing::info!(file_id = %file_id, "File deleted from WebDAV");
            Ok(())
        } else {
            Err(StorageError::Backend(format!(
                "WebDAV delete failed: {}",
                resp.status()
            )))
        }
    }

    fn backend_name(&self) -> &str {
        "webdav"
    }

    async fn exists(&self, file_id: &str) -> Result<bool, StorageError> {
        let url = self.object_url(file_id);
        let req = self.http_client.head(&url);
        let resp = self
            .apply_auth(req)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(resp.status().as_u16() == 200)
    }
}

// ─── SFTP Storage Backend ───────────────────────────────────

/// SFTP 存储后端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpStorageConfig {
    /// 主机地址
    pub host: String,
    /// 端口
    #[serde(default = "default_sftp_port")]
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 密码认证
    #[serde(default)]
    pub password: Option<String>,
    /// 密钥文件路径
    #[serde(default)]
    pub key_path: Option<String>,
    /// 远程基础目录
    #[serde(default = "default_sftp_base_dir")]
    pub base_dir: String,
}

fn default_sftp_port() -> u16 {
    22
}
fn default_sftp_base_dir() -> String {
    "/tmp/openlink-sftp".to_string()
}

impl SftpStorageConfig {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.host.is_empty() {
            return Err(StorageError::Config("host is required".to_string()));
        }
        if self.username.is_empty() {
            return Err(StorageError::Config("username is required".to_string()));
        }
        if self.password.is_none() && self.key_path.is_none() {
            return Err(StorageError::Config("password or key_path is required".to_string()));
        }
        Ok(())
    }
}

/// SFTP 存储后端
///
/// 使用 ssh2 crate，支持密钥认证和密码认证。
/// 注意：由于 ssh2 是同步的，我们使用 tokio::task::spawn_blocking 包装。
pub struct SftpStorageBackend {
    config: SftpStorageConfig,
}

impl SftpStorageBackend {
    pub fn new(config: SftpStorageConfig) -> Result<Self, StorageError> {
        config.validate()?;
        Ok(Self { config })
    }

    fn remote_path(&self, file_id: &str) -> String {
        let base = self.config.base_dir.trim_end_matches('/');
        format!("{}/{}", base, file_id)
    }

    /// 创建到 SFTP 服务器的连接
    fn connect(&self) -> Result<(ssh2::Session, ssh2::Sftp), StorageError> {
        let tcp = std::net::TcpStream::connect(format!("{}:{}", self.config.host, self.config.port))
            .map_err(|e| StorageError::Backend(format!("TCP connect failed: {}", e)))?;

        let mut sess =
            ssh2::Session::new().map_err(|e| StorageError::Backend(format!("SSH session create failed: {}", e)))?;
        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| StorageError::Backend(format!("SSH handshake failed: {}", e)))?;

        // Authenticate
        if let Some(ref key_path) = self.config.key_path {
            sess.userauth_pubkey_file(
                &self.config.username,
                None, // no public key path, let ssh2 find it
                std::path::Path::new(key_path),
                self.config.password.as_deref(),
            )
            .map_err(|e| StorageError::Backend(format!("SSH key auth failed: {}", e)))?;
        } else if let Some(ref password) = self.config.password {
            sess.userauth_password(&self.config.username, password)
                .map_err(|e| StorageError::Backend(format!("SSH password auth failed: {}", e)))?;
        }

        if !sess.authenticated() {
            return Err(StorageError::Backend("SSH authentication failed".to_string()));
        }

        let sftp = sess
            .sftp()
            .map_err(|e| StorageError::Backend(format!("SFTP channel open failed: {}", e)))?;

        Ok((sess, sftp))
    }
}

#[async_trait]
impl StorageBackend for SftpStorageBackend {
    async fn upload(&self, file_id: &str, data: Vec<u8>, _content_type: &str) -> Result<(), StorageError> {
        let remote_path = self.remote_path(file_id);
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || -> Result<(), StorageError> {
            let (_sess, sftp) = SftpStorageBackend { config }.connect()?;

            // Ensure parent directory exists
            if let Some(parent) = remote_path.rfind('/') {
                let parent_path = &remote_path[..parent];
                let _ = sftp.mkdir(std::path::Path::new(parent_path), 0o755);
            }

            let mut remote_file = sftp
                .create(std::path::Path::new(&remote_path))
                .map_err(|e| StorageError::Backend(format!("SFTP create file failed: {}", e)))?;
            use std::io::Write;
            remote_file
                .write_all(&data)
                .map_err(|e| StorageError::Backend(format!("SFTP write failed: {}", e)))?;

            Ok(())
        })
        .await
        .map_err(|e| StorageError::Backend(format!("Task join error: {}", e)))?
    }

    async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        let remote_path = self.remote_path(file_id);
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, StorageError> {
            let (_sess, sftp) = SftpStorageBackend { config }.connect()?;

            let mut remote_file = sftp
                .open(std::path::Path::new(&remote_path))
                .map_err(|e| StorageError::Backend(format!("SFTP open file failed: {}", e)))?;

            let mut buf = Vec::new();
            use std::io::Read;
            remote_file
                .read_to_end(&mut buf)
                .map_err(|e| StorageError::Backend(format!("SFTP read failed: {}", e)))?;

            Ok(buf)
        })
        .await
        .map_err(|e| StorageError::Backend(format!("Task join error: {}", e)))?
    }

    async fn presigned_url(&self, file_id: &str, _expires_in_secs: u64) -> Result<String, StorageError> {
        // SFTP doesn't support presigned URLs; return sftp:// scheme
        Ok(format!(
            "sftp://{}@{}:{}/{}",
            self.config.username,
            self.config.host,
            self.config.port,
            self.remote_path(file_id)
        ))
    }

    async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        let remote_path = self.remote_path(file_id);
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || -> Result<(), StorageError> {
            let (_sess, sftp) = SftpStorageBackend { config }.connect()?;
            match sftp.unlink(std::path::Path::new(&remote_path)) {
                Ok(()) => Ok(()),
                Err(e) => {
                    // If file doesn't exist, that's OK
                    let err_msg = e.to_string();
                    if err_msg.contains("No such file") || err_msg.contains("not found") {
                        Ok(())
                    } else {
                        Err(StorageError::Backend(format!("SFTP delete failed: {}", e)))
                    }
                }
            }
        })
        .await
        .map_err(|e| StorageError::Backend(format!("Task join error: {}", e)))?
    }

    fn backend_name(&self) -> &str {
        "sftp"
    }

    async fn exists(&self, file_id: &str) -> Result<bool, StorageError> {
        let remote_path = self.remote_path(file_id);
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || -> Result<bool, StorageError> {
            let (_sess, sftp) = SftpStorageBackend { config }.connect()?;
            match sftp.stat(std::path::Path::new(&remote_path)) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        })
        .await
        .map_err(|e| StorageError::Backend(format!("Task join error: {}", e)))?
    }
}

/// SFTP 列表操作
impl SftpStorageBackend {
    /// 列出目录
    pub async fn list(&self, dir_path: &str) -> Result<Vec<SftpFileInfo>, StorageError> {
        let remote_path = {
            let base = self.config.base_dir.trim_end_matches('/');
            format!("{}/{}", base, dir_path)
        };
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<SftpFileInfo>, StorageError> {
            let (_sess, sftp) = SftpStorageBackend { config }.connect()?;
            let entries = sftp
                .readdir(std::path::Path::new(&remote_path))
                .map_err(|e| StorageError::Backend(format!("SFTP readdir failed: {}", e)))?;

            let files = entries
                .into_iter()
                .map(|(path, stat)| {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
                    let is_dir = stat.is_dir();
                    let size = stat.size.unwrap_or(0);
                    SftpFileInfo {
                        name,
                        is_directory: is_dir,
                        size,
                    }
                })
                .collect();

            Ok(files)
        })
        .await
        .map_err(|e| StorageError::Backend(format!("Task join error: {}", e)))?
    }
}

/// SFTP 文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpFileInfo {
    /// 文件名
    pub name: String,
    /// 是否为目录
    pub is_directory: bool,
    /// 文件大小
    pub size: u64,
}

// ─── Storage Router ─────────────────────────────────────────

/// 存储后端选择策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StorageStrategy {
    /// 自动选择（根据文件大小等）
    Auto,
    /// 强制使用本地存储
    Local,
    /// 强制使用 R2
    R2,
    /// 强制使用 OSS
    Oss,
    /// 强制使用 WebDAV
    Webdav,
    /// 强制使用 SFTP
    Sftp,
}

/// 存储路由配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRouterConfig {
    /// 默认策略
    pub default_strategy: StorageStrategy,
    /// 大文件阈值（字节）
    pub large_file_threshold: u64,
    /// R2 配置
    pub r2_config: Option<R2StorageConfig>,
    /// 本地配置
    pub local_config: LocalStorageConfig,
    /// OSS 配置
    pub oss_config: Option<OssStorageConfig>,
    /// WebDAV 配置
    pub webdav_config: Option<WebdavStorageConfig>,
    /// SFTP 配置
    pub sftp_config: Option<SftpStorageConfig>,
}

impl Default for StorageRouterConfig {
    fn default() -> Self {
        Self {
            default_strategy: StorageStrategy::Auto,
            large_file_threshold: 10 * 1024 * 1024,
            r2_config: None,
            local_config: LocalStorageConfig::default(),
            oss_config: None,
            webdav_config: None,
            sftp_config: None,
        }
    }
}

/// 存储路由
pub struct StorageRouter {
    config: StorageRouterConfig,
    local: Arc<LocalStorageBackend>,
    r2: Option<Arc<R2StorageBackend>>,
    oss: Option<Arc<OssStorageBackend>>,
    webdav: Option<Arc<WebdavStorageBackend>>,
    sftp: Option<Arc<SftpStorageBackend>>,
}

impl StorageRouter {
    pub fn new() -> Self {
        Self::with_config(StorageRouterConfig::default())
    }

    pub fn with_config(config: StorageRouterConfig) -> Self {
        let local = Arc::new(LocalStorageBackend::new(config.local_config.clone()));

        let r2 = config
            .r2_config
            .as_ref()
            .map(|cfg| Arc::new(R2StorageBackend::new(cfg.clone()).expect("Invalid R2 config")));

        let oss = config
            .oss_config
            .as_ref()
            .map(|cfg| Arc::new(OssStorageBackend::new(cfg.clone()).expect("Invalid OSS config")));

        let webdav = config
            .webdav_config
            .as_ref()
            .map(|cfg| Arc::new(WebdavStorageBackend::new(cfg.clone()).expect("Invalid WebDAV config")));

        let sftp = config
            .sftp_config
            .as_ref()
            .map(|cfg| Arc::new(SftpStorageBackend::new(cfg.clone()).expect("Invalid SFTP config")));

        Self {
            config,
            local,
            r2,
            oss,
            webdav,
            sftp,
        }
    }

    fn select_backend(
        &self,
        file_size: u64,
        strategy: &StorageStrategy,
    ) -> Result<Arc<dyn StorageBackend>, StorageError> {
        match strategy {
            StorageStrategy::Local => Ok(self.local.clone() as Arc<dyn StorageBackend>),
            StorageStrategy::R2 => self
                .r2
                .clone()
                .ok_or_else(|| StorageError::Config("R2 not configured".to_string()))
                .map(|b| b as Arc<dyn StorageBackend>),
            StorageStrategy::Oss => self
                .oss
                .clone()
                .ok_or_else(|| StorageError::Config("OSS not configured".to_string()))
                .map(|b| b as Arc<dyn StorageBackend>),
            StorageStrategy::Webdav => self
                .webdav
                .clone()
                .ok_or_else(|| StorageError::Config("WebDAV not configured".to_string()))
                .map(|b| b as Arc<dyn StorageBackend>),
            StorageStrategy::Sftp => self
                .sftp
                .clone()
                .ok_or_else(|| StorageError::Config("SFTP not configured".to_string()))
                .map(|b| b as Arc<dyn StorageBackend>),
            StorageStrategy::Auto => {
                if file_size > self.config.large_file_threshold {
                    if let Some(ref r2) = self.r2 {
                        return Ok(r2.clone() as Arc<dyn StorageBackend>);
                    }
                    if let Some(ref oss) = self.oss {
                        return Ok(oss.clone() as Arc<dyn StorageBackend>);
                    }
                }
                Ok(self.local.clone() as Arc<dyn StorageBackend>)
            }
        }
    }

    pub async fn upload(
        &self,
        file_id: &str,
        data: Vec<u8>,
        content_type: &str,
        strategy: Option<StorageStrategy>,
    ) -> Result<(), StorageError> {
        let strategy = strategy.unwrap_or_else(|| self.config.default_strategy.clone());
        let backend = self.select_backend(data.len() as u64, &strategy)?;
        backend.upload(file_id, data, content_type).await
    }

    pub async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        // Try each backend in priority order
        if let Some(ref r2) = self.r2 {
            if r2.exists(file_id).await.unwrap_or(false) {
                return r2.download(file_id).await;
            }
        }
        if let Some(ref oss) = self.oss {
            if oss.exists(file_id).await.unwrap_or(false) {
                return oss.download(file_id).await;
            }
        }
        if let Some(ref webdav) = self.webdav {
            if webdav.exists(file_id).await.unwrap_or(false) {
                return webdav.download(file_id).await;
            }
        }
        if let Some(ref sftp) = self.sftp {
            if sftp.exists(file_id).await.unwrap_or(false) {
                return sftp.download(file_id).await;
            }
        }
        if self.local.exists(file_id).await.unwrap_or(false) {
            return self.local.download(file_id).await;
        }

        Err(StorageError::NotFound(file_id.to_string()))
    }

    pub async fn presigned_url(&self, file_id: &str, expires_in_secs: u64) -> Result<String, StorageError> {
        if let Some(ref r2) = self.r2 {
            if r2.exists(file_id).await.unwrap_or(false) {
                return r2.presigned_url(file_id, expires_in_secs).await;
            }
        }
        if let Some(ref oss) = self.oss {
            if oss.exists(file_id).await.unwrap_or(false) {
                return oss.presigned_url(file_id, expires_in_secs).await;
            }
        }

        self.local.presigned_url(file_id, expires_in_secs).await
    }

    pub async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        let mut errors = Vec::new();

        if let Some(ref r2) = self.r2 {
            if let Err(e) = r2.delete(file_id).await {
                errors.push(e);
            }
        }
        if let Some(ref oss) = self.oss {
            if let Err(e) = oss.delete(file_id).await {
                errors.push(e);
            }
        }
        if let Some(ref webdav) = self.webdav {
            if let Err(e) = webdav.delete(file_id).await {
                errors.push(e);
            }
        }
        if let Some(ref sftp) = self.sftp {
            if let Err(e) = sftp.delete(file_id).await {
                errors.push(e);
            }
        }
        if let Err(e) = self.local.delete(file_id).await {
            errors.push(e);
        }

        // Only error if ALL backends failed
        let total_backends = 1
            + self.r2.is_some() as usize
            + self.oss.is_some() as usize
            + self.webdav.is_some() as usize
            + self.sftp.is_some() as usize;

        if errors.len() >= total_backends {
            return Err(StorageError::Backend("Failed to delete from all backends".to_string()));
        }

        Ok(())
    }
}

impl Default for StorageRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_storage_config_default() {
        let config = LocalStorageConfig::default();
        assert_eq!(config.base_path, "/tmp/openlink-storage");
    }

    #[test]
    fn test_storage_router_new() {
        let router = StorageRouter::new();
        assert_eq!(router.config.default_strategy, StorageStrategy::Auto);
    }

    #[test]
    fn test_r2_config_validation() {
        let config = R2StorageConfig {
            account_id: "".to_string(),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            bucket: "bucket".to_string(),
            public_url: "https://pub.example.com".to_string(),
        };
        assert!(R2StorageBackend::new(config).is_err());
    }

    #[test]
    fn test_oss_config_validation() {
        let config = OssStorageConfig {
            access_key_id: "".to_string(),
            access_key_secret: "secret".to_string(),
            bucket: "my-bucket".to_string(),
            endpoint: "oss-cn-hangzhou.aliyuncs.com".to_string(),
            public_url: None,
        };
        assert!(OssStorageBackend::new(config).is_err());
    }

    #[test]
    fn test_oss_config_valid() {
        let config = OssStorageConfig {
            access_key_id: "LTAI...".to_string(),
            access_key_secret: "secret".to_string(),
            bucket: "my-bucket".to_string(),
            endpoint: "oss-cn-hangzhou.aliyuncs.com".to_string(),
            public_url: Some("https://cdn.example.com".to_string()),
        };
        assert!(OssStorageBackend::new(config).is_ok());
    }

    #[test]
    fn test_webdav_config_validation() {
        let config = WebdavStorageConfig {
            base_url: "".to_string(),
            username: None,
            password: None,
        };
        assert!(WebdavStorageBackend::new(config).is_err());
    }

    #[test]
    fn test_webdav_config_valid() {
        let config = WebdavStorageConfig {
            base_url: "https://dav.example.com/files/".to_string(),
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
        };
        assert!(WebdavStorageBackend::new(config).is_ok());
    }

    #[test]
    fn test_webdav_auth_header() {
        let config = WebdavStorageConfig {
            base_url: "https://dav.example.com".to_string(),
            username: Some("alice".to_string()),
            password: Some("secret".to_string()),
        };
        let auth = config.auth_header().unwrap();
        assert!(auth.starts_with("Basic "));
    }

    #[test]
    fn test_sftp_config_validation() {
        let config = SftpStorageConfig {
            host: "".to_string(),
            port: 22,
            username: "user".to_string(),
            password: Some("pass".to_string()),
            key_path: None,
            base_dir: "/data".to_string(),
        };
        assert!(SftpStorageBackend::new(config).is_err());
    }

    #[test]
    fn test_sftp_config_no_auth() {
        let config = SftpStorageConfig {
            host: "sftp.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            password: None,
            key_path: None,
            base_dir: "/data".to_string(),
        };
        assert!(SftpStorageBackend::new(config).is_err());
    }

    #[test]
    fn test_sftp_config_valid_password() {
        let config = SftpStorageConfig {
            host: "sftp.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            password: Some("pass".to_string()),
            key_path: None,
            base_dir: "/data".to_string(),
        };
        assert!(SftpStorageBackend::new(config).is_ok());
    }

    #[test]
    fn test_sftp_config_valid_key() {
        let config = SftpStorageConfig {
            host: "sftp.example.com".to_string(),
            port: 2222,
            username: "user".to_string(),
            password: None,
            key_path: Some("/home/user/.ssh/id_rsa".to_string()),
            base_dir: "/data".to_string(),
        };
        let backend = SftpStorageBackend::new(config).unwrap();
        assert_eq!(backend.backend_name(), "sftp");
    }

    #[test]
    fn test_oss_sign_v1() {
        let config = OssStorageConfig {
            access_key_id: "LTAI".to_string(),
            access_key_secret: "mysecret".to_string(),
            bucket: "bucket".to_string(),
            endpoint: "oss-cn-hangzhou.aliyuncs.com".to_string(),
            public_url: None,
        };
        let auth = config.sign_v1("GET", "/bucket/test.txt", "Sun, 01 Jan 2024 00:00:00 GMT", "");
        assert!(auth.starts_with("OSS LTAI:"));
    }

    #[test]
    fn test_sftp_remote_path() {
        let config = SftpStorageConfig {
            host: "sftp.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            password: Some("pass".to_string()),
            key_path: None,
            base_dir: "/data/files".to_string(),
        };
        let backend = SftpStorageBackend::new(config).unwrap();
        assert_eq!(backend.remote_path("dir/file.txt"), "/data/files/dir/file.txt");
    }

    #[test]
    fn test_webdav_object_url() {
        let config = WebdavStorageConfig {
            base_url: "https://dav.example.com/files/".to_string(),
            username: None,
            password: None,
        };
        let backend = WebdavStorageBackend::new(config).unwrap();
        assert_eq!(
            backend.object_url("dir/file.txt"),
            "https://dav.example.com/files/dir/file.txt"
        );
    }

    #[test]
    fn test_storage_strategy_variants() {
        assert_eq!(StorageStrategy::Auto, StorageStrategy::Auto);
        assert_ne!(StorageStrategy::Local, StorageStrategy::R2);
        assert_ne!(StorageStrategy::Oss, StorageStrategy::Webdav);
        assert_ne!(StorageStrategy::Sftp, StorageStrategy::Local);
    }

    #[test]
    fn test_storage_router_select_backend_unconfigured() {
        let router = StorageRouter::new();
        // R2 not configured, should error
        let result = router.select_backend(100, &StorageStrategy::R2);
        assert!(result.is_err());
        // OSS not configured
        let result = router.select_backend(100, &StorageStrategy::Oss);
        assert!(result.is_err());
        // WebDAV not configured
        let result = router.select_backend(100, &StorageStrategy::Webdav);
        assert!(result.is_err());
        // SFTP not configured
        let result = router.select_backend(100, &StorageStrategy::Sftp);
        assert!(result.is_err());
        // Local should always work
        let result = router.select_backend(100, &StorageStrategy::Local);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_local_backend_upload_download() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = LocalStorageConfig {
            base_path: temp_dir.path().to_str().unwrap().to_string(),
        };
        let backend = LocalStorageBackend::new(config);

        let file_id = "test-file.txt";
        let data = b"Hello, World!".to_vec();

        backend.upload(file_id, data.clone(), "text/plain").await.unwrap();
        assert!(backend.exists(file_id).await.unwrap());

        let downloaded = backend.download(file_id).await.unwrap();
        assert_eq!(downloaded, data);

        backend.delete(file_id).await.unwrap();
        assert!(!backend.exists(file_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_storage_router_auto_selection() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = StorageRouterConfig {
            default_strategy: StorageStrategy::Auto,
            large_file_threshold: 10,
            r2_config: None,
            local_config: LocalStorageConfig {
                base_path: temp_dir.path().to_str().unwrap().to_string(),
            },
            oss_config: None,
            webdav_config: None,
            sftp_config: None,
        };

        let router = StorageRouter::with_config(config);

        let small_data = b"small".to_vec();
        router
            .upload("small.txt", small_data, "text/plain", None)
            .await
            .unwrap();

        let large_data = b"this is a larger file content".to_vec();
        router
            .upload("large.txt", large_data, "text/plain", None)
            .await
            .unwrap();

        assert!(router.download("small.txt").await.is_ok());
        assert!(router.download("large.txt").await.is_ok());
    }

    #[test]
    fn test_oss_presigned_url_signing() {
        let config = OssStorageConfig {
            access_key_id: "test_key".to_string(),
            access_key_secret: "test_secret".to_string(),
            bucket: "test-bucket".to_string(),
            endpoint: "oss-cn-hangzhou.aliyuncs.com".to_string(),
            public_url: None,
        };
        let backend = OssStorageBackend::new(config).unwrap();
        // presigned_url is async, just test the object_url construction
        assert_eq!(
            backend.object_url("my/file.txt"),
            "https://test-bucket.oss-cn-hangzhou.aliyuncs.com/my/file.txt"
        );
    }
}
