//! # 存储后端抽象与实现
//!
//! 提供统一的存储接口，支持多种后端：本地存储、R2 等。

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
            self.config.bucket,
            self.config.account_id,
            object_key
        )
    }
    
    fn sign_request(&self, method: &str, path: &str) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            method,
            path,
            timestamp,
            self.config.account_id
        );
        
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        
        type HmacSha256 = Hmac<Sha256>;
        
        let mut mac = HmacSha256::new_from_slice(self.config.secret_access_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(string_to_sign.as_bytes());
        
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());
        
        format!("HMAC-SHA256 Credential={}/{}, SignedHeaders=x-date, Signature={}", 
            self.config.access_key_id, timestamp, signature)
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
            Err(StorageError::Backend(format!(
                "Upload failed: {}", response.status()
            )))
        }
    }
    
    async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        let url = self.api_url(file_id);
        let signed_auth = self.sign_request("GET", &format!("/{}", file_id));
        
        let response = self.http_client
            .get(&url)
            .header("Authorization", signed_auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        
        if response.status().as_u16() == 404 {
            return Err(StorageError::NotFound(file_id.to_string()));
        }
        
        if !response.status().is_success() {
            return Err(StorageError::Backend(format!(
                "Download failed: {}", response.status()
            )));
        }
        
        let bytes = response.bytes().await
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
        
        let response = self.http_client
            .delete(&url)
            .header("Authorization", signed_auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        
        if response.status().is_success() || response.status().as_u16() == 404 {
            tracing::info!(file_id = %file_id, "File deleted from R2");
            Ok(())
        } else {
            Err(StorageError::Backend(format!(
                "Delete failed: {}", response.status()
            )))
        }
    }
    
    fn backend_name(&self) -> &str {
        "r2"
    }
    
    async fn exists(&self, file_id: &str) -> Result<bool, StorageError> {
        let url = self.api_url(file_id);
        let signed_auth = self.sign_request("HEAD", &format!("/{}", file_id));
        
        let response = self.http_client
            .head(&url)
            .header("Authorization", signed_auth)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        
        Ok(response.status().as_u16() == 200)
    }
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
}

impl Default for StorageRouterConfig {
    fn default() -> Self {
        Self {
            default_strategy: StorageStrategy::Auto,
            large_file_threshold: 10 * 1024 * 1024,
            r2_config: None,
            local_config: LocalStorageConfig::default(),
        }
    }
}

/// 存储路由
pub struct StorageRouter {
    config: StorageRouterConfig,
    local: Arc<LocalStorageBackend>,
    r2: Option<Arc<R2StorageBackend>>,
}

impl StorageRouter {
    pub fn new() -> Self {
        Self::with_config(StorageRouterConfig::default())
    }
    
    pub fn with_config(config: StorageRouterConfig) -> Self {
        let local = Arc::new(LocalStorageBackend::new(config.local_config.clone()));
        
        let r2 = config.r2_config.as_ref().map(|cfg| {
            Arc::new(R2StorageBackend::new(cfg.clone()).expect("Invalid R2 config"))
        });
        
        Self { config, local, r2 }
    }
    
    fn select_backend(&self, file_size: u64, strategy: &StorageStrategy) -> Arc<dyn StorageBackend> {
        match strategy {
            StorageStrategy::Local => self.local.clone() as Arc<dyn StorageBackend>,
            StorageStrategy::R2 => self.r2.clone().expect("R2 not configured") as Arc<dyn StorageBackend>,
            StorageStrategy::Auto => {
                if file_size > self.config.large_file_threshold {
                    if let Some(ref r2) = self.r2 {
                        return r2.clone() as Arc<dyn StorageBackend>;
                    }
                }
                self.local.clone() as Arc<dyn StorageBackend>
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
        let strategy = strategy.unwrap_or(self.config.default_strategy.clone());
        let backend = self.select_backend(data.len() as u64, &strategy);
        backend.upload(file_id, data, content_type).await
    }
    
    pub async fn download(&self, file_id: &str) -> Result<Vec<u8>, StorageError> {
        if let Some(ref r2) = self.r2 {
            if r2.exists(file_id).await.unwrap_or(false) {
                return r2.download(file_id).await;
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
        
        self.local.presigned_url(file_id, expires_in_secs).await
    }
    
    pub async fn delete(&self, file_id: &str) -> Result<(), StorageError> {
        let mut errors = Vec::new();
        
        if let Some(ref r2) = self.r2 {
            if let Err(e) = r2.delete(file_id).await {
                errors.push(e);
            }
        }
        
        if let Err(e) = self.local.delete(file_id).await {
            errors.push(e);
        }
        
        if errors.len() == 2 {
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
        };
        
        let router = StorageRouter::with_config(config);
        
        let small_data = b"small".to_vec();
        router.upload("small.txt", small_data, "text/plain", None).await.unwrap();
        
        let large_data = b"this is a larger file content".to_vec();
        router.upload("large.txt", large_data, "text/plain", None).await.unwrap();
        
        assert!(router.download("small.txt").await.is_ok());
        assert!(router.download("large.txt").await.is_ok());
    }
}
