//! # Store Trait 定义

use crate::error::StoreError;
use async_trait::async_trait;
use openlink_core::{AccessLog, Extension, Link, LinkStats, OverviewStats, Route};

/// Store Trait — 所有存储实现必须实现此接口
#[async_trait]
pub trait Store: Send + Sync {
    // ─── Link 操作 ────────────────────────────────────────────

    async fn create_link(&self, link: &Link) -> Result<Link, StoreError>;
    async fn get_link(&self, id: &str) -> Result<Option<Link>, StoreError>;
    async fn get_link_by_code(&self, code: &str) -> Result<Option<Link>, StoreError>;
    async fn update_link(&self, link: &Link) -> Result<Link, StoreError>;
    async fn delete_link(&self, id: &str) -> Result<(), StoreError>;
    async fn list_links(&self, owner: Option<&str>, limit: usize) -> Result<Vec<Link>, StoreError>;

    // ─── Route 操作 ────────────────────────────────────────────

    async fn create_route(&self, route: &Route) -> Result<Route, StoreError>;
    async fn get_route(&self, id: &str) -> Result<Option<Route>, StoreError>;
    async fn get_route_by_link_id(&self, link_id: &str) -> Result<Option<Route>, StoreError>;
    async fn update_route(&self, route: &Route) -> Result<Route, StoreError>;
    async fn delete_route(&self, id: &str) -> Result<(), StoreError>;
    async fn list_routes(&self, link_id: Option<&str>) -> Result<Vec<Route>, StoreError>;

    // ─── Access Log 操作 ──────────────────────────────────────

    async fn log_access(&self, log: &AccessLog) -> Result<(), StoreError>;
    async fn get_access_logs(&self, link_id: &str, limit: usize) -> Result<Vec<AccessLog>, StoreError>;

    // ─── Stats 操作 ───────────────────────────────────────────

    async fn get_link_stats(&self, link_id: &str) -> Result<LinkStats, StoreError>;
    async fn get_overview_stats(&self) -> Result<OverviewStats, StoreError>;

    // ─── Extension 操作 ────────────────────────────────────────

    async fn list_extensions(&self) -> Result<Vec<Extension>, StoreError>;
    async fn save_extension(&self, ext: &Extension) -> Result<(), StoreError>;

    // ─── Health Check (Phase 5) ──────────────────────────────

    async fn health_check(&self) -> Result<(), StoreError>;
}
