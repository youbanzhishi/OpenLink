//! # ext-filetransfer — 文件传输 Action 扩展
//!
//! 实现 CloudRelay 模式的文件传输，支持多种存储后端：
//! - **LocalStorageBackend**: 本地存储（测试用）
//! - **R2StorageBackend**: Cloudflare R2
//! - **OssStorageBackend**: 阿里云 OSS（兼容 MinIO 等 S3 兼容存储）
//! - **WebdavStorageBackend**: WebDAV
//! - **SftpStorageBackend**: SFTP（ssh2）
//!
//! 存储路由（Storage Router）跟 Link 路由引擎同构。

pub mod actions;
pub mod storage;

pub use actions::FileTransferAction;
pub use storage::{
    LocalStorageBackend, LocalStorageConfig, OssStorageBackend, OssStorageConfig, R2StorageBackend, R2StorageConfig,
    SftpFileInfo, SftpStorageBackend, SftpStorageConfig, StorageBackend, StorageError, StorageRouter, StorageStrategy,
    WebdavFileInfo, WebdavStorageBackend, WebdavStorageConfig,
};

// ─── Re-exports for convenience ─────────────────────────────

/// 注册所有文件传输扩展到 Extension Registry
pub fn register(registry: &mut openlink_core::ExtensionRegistry) -> Result<(), openlink_core::CoreError> {
    use std::sync::Arc;

    // 创建存储路由
    let storage = Arc::new(StorageRouter::new());

    // 注册文件传输 Action
    let action = FileTransferAction::new(storage);
    registry.register_action(Arc::new(action))?;

    Ok(())
}
