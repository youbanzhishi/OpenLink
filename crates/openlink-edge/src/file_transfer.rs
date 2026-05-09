//! # 文件传输服务（边缘版）
//!
//! 轻量文件传输服务，支持基本的推送和拉取。

use std::path::{Path, PathBuf};
use tokio::fs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub file_id: String,
    pub filename: String,
    pub size: u64,
    pub created_at: i64,
}

/// 文件传输服务
pub struct FileTransferService {
    storage_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum TransferError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("File not found: {0}")]
    NotFound(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
}

impl FileTransferService {
    /// 创建新的文件传输服务
    pub fn new(storage_path: impl Into<PathBuf>) -> Self {
        Self {
            storage_path: storage_path.into(),
        }
    }
    
    /// 初始化存储目录
    pub async fn init(&self) -> Result<(), TransferError> {
        fs::create_dir_all(&self.storage_path).await?;
        Ok(())
    }
    
    /// 存储文件
    pub async fn store(&self, file_id: &str, data: Vec<u8>) -> Result<FileInfo, TransferError> {
        let path = self.storage_path.join(file_id);
        
        fs::write(&path, data).await?;
        
        let metadata = fs::metadata(&path).await?;
        
        Ok(FileInfo {
            file_id: file_id.to_string(),
            filename: file_id.to_string(),
            size: metadata.len(),
            created_at: chrono::Utc::now().timestamp(),
        })
    }
    
    /// 获取文件
    pub async fn get(&self, file_id: &str) -> Result<Vec<u8>, TransferError> {
        let path = self.storage_path.join(file_id);
        
        if !path.exists() {
            return Err(TransferError::NotFound(file_id.to_string()));
        }
        
        Ok(fs::read(&path).await?)
    }
    
    /// 删除文件
    pub async fn delete(&self, file_id: &str) -> Result<(), TransferError> {
        let path = self.storage_path.join(file_id);
        
        if !path.exists() {
            return Err(TransferError::NotFound(file_id.to_string()));
        }
        
        fs::remove_file(&path).await?;
        Ok(())
    }
    
    /// 列出所有文件
    pub async fn list(&self) -> Result<Vec<FileInfo>, TransferError> {
        let mut files = Vec::new();
        
        let mut entries = fs::read_dir(&self.storage_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                let metadata = entry.metadata().await?;
                let file_id = entry.file_name().to_string_lossy().to_string();
                
                files.push(FileInfo {
                    file_id: file_id.clone(),
                    filename: file_id,
                    size: metadata.len(),
                    created_at: metadata.created()
                        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
                        .unwrap_or(0),
                });
            }
        }
        
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_store_and_get() {
        let temp = TempDir::new().unwrap();
        let service = FileTransferService::new(temp.path());
        service.init().await.unwrap();
        
        let data = b"Hello, Edge!".to_vec();
        let info = service.store("test.txt", data.clone()).await.unwrap();
        
        assert_eq!(info.file_id, "test.txt");
        assert_eq!(info.size, 12);
        
        let retrieved = service.get("test.txt").await.unwrap();
        assert_eq!(retrieved, data);
    }
    
    #[tokio::test]
    async fn test_not_found() {
        let temp = TempDir::new().unwrap();
        let service = FileTransferService::new(temp.path());
        service.init().await.unwrap();
        
        let result = service.get("nonexistent").await;
        assert!(result.is_err());
    }
}
