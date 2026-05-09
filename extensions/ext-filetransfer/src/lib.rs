//! # ext-filetransfer — 文件传输 Action 扩展
//!
//! 实现 CloudRelay 模式的文件传输，支持多种存储后端：
//! - **LocalStorageBackend**: ECS 本地存储（测试用）
//! - **R2StorageBackend**: Cloudflare R2（推荐主力）
//!
//! 存储路由（Storage Router）跟 Link 路由引擎同构。

pub mod storage;
pub mod actions;

pub use storage::{StorageBackend, StorageRouter, LocalStorageBackend, R2StorageBackend, StorageError};
pub use actions::FileTransferAction;

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
