//! # 文件服务模块
//!
//! 在设备端暴露 HTTP 文件上传/下载端点，供其他 LAN 节点使用。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

/// 文件存储后端
#[derive(Debug, Clone)]
pub enum FileBackend {
    /// 本地文件系统
    Local(PathBuf),
    /// 内存存储（测试用）
    Memory,
}

/// 文件元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub file_id: String,
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    pub uploaded_at: chrono::DateTime<chrono::Utc>,
}

/// 文件请求
#[derive(Debug, Clone, Deserialize)]
pub struct FileRequest {
    pub file_id: Option<String>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
}

/// 文件服务
pub struct FileServer {
    backend: Arc<RwLock<FileBackend>>,
    /// 内存存储（当 backend 为 Memory 时使用）
    memory_store: Arc<RwLock<std::collections::HashMap<String, (String, Vec<u8>)>>>,
}

impl FileServer {
    /// 创建文件服务器
    pub fn new(backend: FileBackend) -> Self {
        Self {
            backend: Arc::new(RwLock::new(backend)),
            memory_store: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// 创建内存存储的文件服务器（测试用）
    pub fn memory() -> Self {
        Self::new(FileBackend::Memory)
    }

    /// 启动 HTTP 文件服务
    pub async fn serve(self: Arc<Self>, port: u16) -> Result<(), FileServiceError> {
        let file_server_upload = self.clone();
        let file_server_download = self.clone();
        let file_server_head = self.clone();

        // 文件上传端点
        let upload = warp::path("openlink")
            .and(warp::path("files"))
            .and(warp::path("upload"))
            .and(warp::post())
            .and(warp::body::content_length_limit(500 * 1024 * 1024))
            .and(warp::body::bytes())
            .and_then(move |body: bytes::Bytes| {
                let server = file_server_upload.clone();
                async move {
                    server.handle_upload(body).await
                }
            });

        // 文件下载端点
        let download = warp::path("openlink")
            .and(warp::path("files"))
            .and(warp::path::param::<String>())
            .and(warp::get())
            .and_then(move |file_id: String| {
                let server = file_server_download.clone();
                let file_id = file_id.clone();
                async move {
                    server.handle_download(&file_id).await
                }
            });

        // 文件存在检查端点
        let head = warp::path("openlink")
            .and(warp::path("files"))
            .and(warp::path::param::<String>())
            .and(warp::head())
            .and_then(move |file_id: String| {
                let server = file_server_head.clone();
                let file_id = file_id.clone();
                async move {
                    server.handle_exists(&file_id).await
                }
            });

        let routes = upload.or(download).or(head);
        let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();

        tracing::info!(port = port, "Starting file server");
        warp::serve(routes).run(addr).await;
        Ok(())
    }

    async fn handle_upload(&self, body: bytes::Bytes) -> Result<impl warp::Reply, warp::Rejection> {
        let file_id = uuid::Uuid::new_v4().to_string();
        let body_len = body.len();

        let backend = self.backend.read().await;
        match &*backend {
            FileBackend::Memory => {
                let mut store = self.memory_store.write().await;
                store.insert(file_id.clone(), (file_id.clone(), body.to_vec()));
            }
            FileBackend::Local(path) => {
                let file_path = path.join(&file_id);
                tokio::fs::write(&file_path, &body)
                    .await
                    .map_err(|e| warp::reject::custom(FileServiceError::IoError(e.to_string())))?;
            }
        }

        Ok(warp::reply::json(&serde_json::json!({
            "type": "file_uploaded",
            "file_id": file_id,
            "size": body_len,
        })))
    }

    async fn handle_download(&self, file_id: &str) -> Result<impl warp::Reply, warp::Rejection> {
        let backend = self.backend.read().await;
        match &*backend {
            FileBackend::Memory => {
                let store = self.memory_store.read().await;
                if let Some((_, body)) = store.get(file_id) {
                    let body_vec = body.clone();
                    Ok(warp::http::Response::builder()
                        .status(200)
                        .header("Content-Type", "application/octet-stream")
                        .body(warp::hyper::Body::from(body_vec))
                        .unwrap())
                } else {
                    Err(warp::reject::not_found())
                }
            }
            FileBackend::Local(path) => {
                let file_path = path.join(file_id);
                if !file_path.exists() {
                    return Err(warp::reject::not_found());
                }
                let body = tokio::fs::read(&file_path)
                    .await
                    .map_err(|e| warp::reject::custom(FileServiceError::IoError(e.to_string())))?;
                Ok(warp::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "application/octet-stream")
                    .body(warp::hyper::Body::from(body))
                    .unwrap())
            }
        }
    }

    async fn handle_exists(&self, file_id: &str) -> Result<impl warp::Reply, warp::Rejection> {
        let backend = self.backend.read().await;
        let exists = match &*backend {
            FileBackend::Memory => {
                let store = self.memory_store.read().await;
                store.contains_key(file_id)
            }
            FileBackend::Local(path) => path.join(file_id).exists(),
        };

        if exists {
            Ok(warp::http::Response::builder()
                .status(200)
                .body(warp::hyper::Body::empty())
                .unwrap())
        } else {
            Err(warp::reject::not_found())
        }
    }

    /// 内存存储：添加文件（用于测试）
    #[cfg(test)]
    pub async fn put_memory(&self, file_id: &str, filename: &str, body: Vec<u8>) {
        let mut store = self.memory_store.write().await;
        store.insert(file_id.to_string(), (filename.to_string(), body));
    }

    /// 内存存储：获取文件
    #[cfg(test)]
    pub async fn get_memory(&self, file_id: &str) -> Option<Vec<u8>> {
        let store = self.memory_store.read().await;
        store.get(file_id).map(|(_, b)| b.clone())
    }
}

/// 文件服务错误
#[derive(Debug, thiserror::Error)]
pub enum FileServiceError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Not found")]
    NotFound,
    #[error("File too large")]
    FileTooLarge,
}

impl warp::reject::Reject for FileServiceError {}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_backend() {
        let server = FileServer::memory();
        let file_id = "test-file-1";
        let body = b"hello world".to_vec();
        server.put_memory(file_id, "test.txt", body.clone()).await;

        let retrieved = server.get_memory(file_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), body);
    }

    #[tokio::test]
    async fn test_memory_not_found() {
        let server = FileServer::memory();
        let retrieved = server.get_memory("non-existent").await;
        assert!(retrieved.is_none());
    }
}
