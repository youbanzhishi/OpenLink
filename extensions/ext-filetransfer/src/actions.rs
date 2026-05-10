//! # FileTransfer Action 实现

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openlink_core::{ActionHandler, ActionResult, Context, CoreError, Target};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::storage::StorageRouter;

/// 文件传输操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileTransferOperation {
    /// 上传文件
    Upload,
    /// 下载文件
    Download,
    /// 生成分享链接
    Share,
    /// 获取文件信息
    Info,
}

/// 存储策略
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageStrategy {
    /// 自动选择
    #[default]
    Auto,
    /// 本地存储
    Local,
    /// R2 存储
    R2,
}

/// 文件传输请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferParams {
    /// 操作类型
    pub operation: FileTransferOperation,
    /// 文件 ID（下载/分享时使用）
    #[serde(default)]
    pub file_id: Option<String>,
    /// 文件名
    #[serde(default)]
    pub filename: Option<String>,
    /// 内容类型
    #[serde(default)]
    pub content_type: Option<String>,
    /// 文件大小
    #[serde(default)]
    pub size: Option<u64>,
    /// 存储后端策略
    #[serde(default)]
    pub storage: Option<StorageStrategy>,
    /// 分享链接过期时间（秒）
    #[serde(default)]
    pub share_ttl: Option<u64>,
    /// 分享码
    #[serde(default)]
    pub share_code: Option<String>,
    /// 上传 URL（预签名 URL，用于实际上传）
    #[serde(default)]
    pub upload_url: Option<bool>,
}

/// 文件元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub file_id: String,
    pub filename: Option<String>,
    pub content_type: String,
    pub size: u64,
    pub storage: String,
    pub created_at: DateTime<Utc>,
    pub access_count: u64,
}

/// 文件传输 Action 处理器
pub struct FileTransferAction {
    storage_router: Arc<StorageRouter>,
}

impl FileTransferAction {
    pub fn new(storage_router: Arc<StorageRouter>) -> Self {
        Self { storage_router }
    }

    /// 解析参数
    fn parse_params(params: &serde_json::Value) -> Result<FileTransferParams, CoreError> {
        serde_json::from_value(params.clone())
            .map_err(|e| CoreError::ExtensionError(format!("Invalid file transfer params: {}", e)))
    }

    /// 生成文件 ID
    fn generate_file_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// 生成分享码
    fn generate_share_code() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let code: String = (0..8)
            .map(|_| {
                let idx = rng.gen_range(0..62);
                let chars = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
                chars[idx] as char
            })
            .collect();
        code
    }

    /// 处理上传操作
    async fn handle_upload(&self, params: &FileTransferParams) -> Result<ActionResult, CoreError> {
        let file_id = params
            .file_id
            .clone()
            .unwrap_or_else(Self::generate_file_id);

        // 如果需要返回上传 URL
        if params.upload_url.unwrap_or(false) {
            // 在实际实现中，这里应该先生成预签名上传 URL
            // 但由于我们需要先调用后端 API，这里简化处理
            let upload_url = format!("https://api.openlink.dev/api/v1/files/{}/upload", file_id);

            return Ok(ActionResult::Json(serde_json::json!({
                "type": "file_upload_initiated",
                "file_id": file_id,
                "upload_url": upload_url,
                "expires_in": 3600
            })));
        }

        Ok(ActionResult::Json(serde_json::json!({
            "type": "file_upload_ready",
            "file_id": file_id,
            "filename": params.filename,
            "content_type": params.content_type,
            "size": params.size
        })))
    }

    /// 处理下载操作
    async fn handle_download(
        &self,
        params: &FileTransferParams,
    ) -> Result<ActionResult, CoreError> {
        let file_id = params.file_id.as_ref().ok_or_else(|| {
            CoreError::ExtensionError("file_id required for download".to_string())
        })?;

        // 生成预签名下载 URL
        let ttl = params.share_ttl.unwrap_or(3600);
        let download_url = self
            .storage_router
            .presigned_url(file_id, ttl)
            .await
            .map_err(|e| CoreError::ExtensionError(format!("Storage error: {}", e)))?;

        Ok(ActionResult::Json(serde_json::json!({
            "type": "file_download",
            "file_id": file_id,
            "download_url": download_url,
            "expires_in": ttl
        })))
    }

    /// 处理分享操作
    async fn handle_share(&self, params: &FileTransferParams) -> Result<ActionResult, CoreError> {
        let file_id = params
            .file_id
            .as_ref()
            .ok_or_else(|| CoreError::ExtensionError("file_id required for share".to_string()))?;

        let share_code = params
            .share_code
            .clone()
            .unwrap_or_else(Self::generate_share_code);
        let ttl = params.share_ttl.unwrap_or(3600 * 24 * 7); // 默认 7 天

        // 生成分享 URL
        let share_url = format!("https://openlink.dev/s/{}", share_code);

        Ok(ActionResult::Json(serde_json::json!({
            "type": "file_share",
            "file_id": file_id,
            "share_code": share_code,
            "share_url": share_url,
            "expires_at": (Utc::now() + chrono::Duration::seconds(ttl as i64)).to_rfc3339()
        })))
    }

    /// 处理信息查询
    async fn handle_info(&self, params: &FileTransferParams) -> Result<ActionResult, CoreError> {
        let file_id = params
            .file_id
            .as_ref()
            .ok_or_else(|| CoreError::ExtensionError("file_id required for info".to_string()))?;

        // 检查文件是否存在
        let exists = self.storage_router.download(file_id).await.is_ok();

        if !exists {
            return Err(CoreError::ExtensionError(format!(
                "File not found: {}",
                file_id
            )));
        }

        Ok(ActionResult::Json(serde_json::json!({
            "type": "file_info",
            "file_id": file_id,
            "filename": params.filename,
            "content_type": params.content_type,
            "exists": true
        })))
    }
}

#[async_trait]
impl ActionHandler for FileTransferAction {
    async fn execute(&self, _ctx: &Context, target: &Target) -> Result<ActionResult, CoreError> {
        let params = Self::parse_params(&target.params)?;

        tracing::info!(operation = ?params.operation, "FileTransfer action");

        match params.operation {
            FileTransferOperation::Upload => self.handle_upload(&params).await,
            FileTransferOperation::Download => self.handle_download(&params).await,
            FileTransferOperation::Share => self.handle_share(&params).await,
            FileTransferOperation::Info => self.handle_info(&params).await,
        }
    }

    fn name(&self) -> &str {
        "file_transfer"
    }
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_upload_params() {
        let params = serde_json::json!({
            "operation": "upload",
            "filename": "test.pdf",
            "content_type": "application/pdf",
            "size": 1024
        });

        let parsed: FileTransferParams = serde_json::from_value(params).unwrap();
        assert_eq!(parsed.operation, FileTransferOperation::Upload);
        assert_eq!(parsed.filename.as_deref(), Some("test.pdf"));
    }

    #[test]
    fn test_parse_download_params() {
        let params = serde_json::json!({
            "operation": "download",
            "file_id": "abc123"
        });

        let parsed: FileTransferParams = serde_json::from_value(params).unwrap();
        assert_eq!(parsed.operation, FileTransferOperation::Download);
        assert_eq!(parsed.file_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_generate_share_code() {
        let code1 = FileTransferAction::generate_share_code();
        let code2 = FileTransferAction::generate_share_code();

        assert_eq!(code1.len(), 8);
        assert_eq!(code2.len(), 8);
        assert_ne!(code1, code2);
    }

    #[test]
    fn test_file_transfer_params_serialization() {
        let params = FileTransferParams {
            operation: FileTransferOperation::Upload,
            file_id: None,
            filename: Some("test.txt".to_string()),
            content_type: Some("text/plain".to_string()),
            size: Some(100),
            storage: Some(StorageStrategy::Auto),
            share_ttl: Some(3600),
            share_code: None,
            upload_url: Some(true),
        };

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"operation\":\"upload\""));

        let parsed: FileTransferParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.operation, FileTransferOperation::Upload);
    }
}
