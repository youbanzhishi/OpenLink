//! # Store Trait — 存储抽象接口
//!
//! 核心逻辑通过此 trait 访问数据，不绑定具体数据库实现。
//! SQLite 和 PostgreSQL 分别实现此 trait。
//!
//! Phase 2: 新增 OverviewStats 和增强版 LinkStats

use async_trait::async_trait;
use openlink_core::{
    Link, Route, Extension, AccessLog, LinkStats, OverviewStats,
};

/// 统一存储接口 — 所有存储操作都通过此 trait
///
/// 设计原则：
/// - 核心逻辑只依赖此 trait，不知道底层是什么数据库
/// - 新增存储后端只需实现此 trait
#[async_trait]
pub trait Store: Send + Sync {
    // ─── Link 操作 ───────────────────────────────────────────

    /// 创建链接
    async fn create_link(&self, link: &Link) -> Result<Link, crate::error::StoreError>;

    /// 通过短码查询链接
    async fn get_link_by_code(&self, code: &str) -> Result<Option<Link>, crate::error::StoreError>;

    /// 更新链接
    async fn update_link(&self, code: &str, payload: &serde_json::Value, metadata: &serde_json::Value) -> Result<Link, crate::error::StoreError>;

    /// 删除链接（软删除：设 is_active = false）
    async fn delete_link(&self, code: &str) -> Result<(), crate::error::StoreError>;

    /// 列出链接（Phase 2: 新增，支持分页）
    async fn list_links(&self, offset: i64, limit: i64) -> Result<Vec<Link>, crate::error::StoreError>;

    /// 统计总链接数（Phase 2: 新增）
    async fn count_links(&self) -> Result<i64, crate::error::StoreError>;

    /// 统计活跃链接数（Phase 2: 新增）
    async fn count_active_links(&self) -> Result<i64, crate::error::StoreError>;

    // ─── Route 操作 ──────────────────────────────────────────

    /// 创建路由规则
    async fn create_route(&self, route: &Route) -> Result<Route, crate::error::StoreError>;

    /// 通过 Link ID 查询路由
    async fn get_route_by_link_id(&self, link_id: &str) -> Result<Option<Route>, crate::error::StoreError>;

    /// 更新路由规则
    async fn update_route(&self, id: &str, route: &Route) -> Result<Route, crate::error::StoreError>;

    /// 删除路由规则
    async fn delete_route(&self, id: &str) -> Result<(), crate::error::StoreError>;

    // ─── Extension 操作 ─────────────────────────────────────

    /// 注册扩展
    async fn register_extension(&self, ext: &Extension) -> Result<Extension, crate::error::StoreError>;

    /// 列出所有扩展
    async fn list_extensions(&self) -> Result<Vec<Extension>, crate::error::StoreError>;

    /// 通过名称查询扩展
    async fn get_extension_by_name(&self, name: &str) -> Result<Option<Extension>, crate::error::StoreError>;

    /// 卸载扩展（软删除）
    async fn delete_extension(&self, name: &str) -> Result<(), crate::error::StoreError>;

    // ─── Access Log 操作 ────────────────────────────────────

    /// 记录访问日志
    async fn log_access(&self, log: &AccessLog) -> Result<(), crate::error::StoreError>;

    /// 获取链接统计（Phase 2: 增强版，包含设备/身份分布）
    async fn get_link_stats(&self, link_id: &str) -> Result<LinkStats, crate::error::StoreError>;

    /// 获取全局统计概览（Phase 2: 新增）
    async fn get_overview_stats(&self) -> Result<OverviewStats, crate::error::StoreError>;
}
