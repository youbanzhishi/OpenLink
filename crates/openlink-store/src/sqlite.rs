//! # SQLite 存储实现
//!
//! OpenLink 初期使用 SQLite 作为存储后端。
//! 表结构参考项目规划文档，适配 SQLite 类型系统：
//! - UUID → TEXT
//! - JSONB → TEXT (存储 JSON 字符串，查询时解析)
//! - TIMESTAMPTZ → TEXT (ISO 8601 格式)
//! - BOOLEAN → INTEGER (0/1)
//!
//! Phase 2 增强：
//! - access_logs 表扩展字段（code, visitor_ip, identity_type, device_type）
//! - 新增 list_links / count_links / count_active_links
//! - 新增 get_overview_stats
//! - 增强 get_link_stats（设备/身份分布）

use async_trait::async_trait;
use openlink_core::{
    Link, Route, Extension, AccessLog, LinkStats, OverviewStats, TopLink,
    Target, Action,
};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use chrono::Utc;
use crate::traits::Store;
use crate::error::StoreError;

/// SQLite 存储实现
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// 创建 SQLite 存储，自动初始化表结构
    pub async fn new(database_url: &str) -> Result<Self, StoreError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| StoreError::DatabaseError(format!("Failed to connect: {}", e)))?;

        let store = Self { pool };
        store.init_tables().await?;
        Ok(store)
    }

    /// 初始化表结构（如果不存在则创建）
    async fn init_tables(&self) -> Result<(), StoreError> {
        // 启用 WAL 模式提升并发性能
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::DatabaseError(format!("Failed to set WAL mode: {}", e)))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS links (
                id          TEXT PRIMARY KEY,
                code        TEXT UNIQUE NOT NULL,
                payload     TEXT DEFAULT '{}',
                owner_id    TEXT NOT NULL,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                metadata    TEXT DEFAULT '{}',
                is_active   INTEGER DEFAULT 1
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS routes (
                id              TEXT PRIMARY KEY,
                link_id         TEXT NOT NULL REFERENCES links(id),
                rules           TEXT DEFAULT '[]',
                default_target  TEXT NOT NULL,
                version         INTEGER NOT NULL DEFAULT 1,
                created_at      TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS extensions (
                id          TEXT PRIMARY KEY,
                ext_type    TEXT NOT NULL,
                name        TEXT UNIQUE NOT NULL,
                config      TEXT DEFAULT '{}',
                is_active   INTEGER DEFAULT 1,
                created_at  TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS access_logs (
                id                TEXT PRIMARY KEY,
                link_id           TEXT NOT NULL,
                context           TEXT NOT NULL,
                matched_rule      TEXT,
                action_taken      TEXT NOT NULL,
                response_time_ms  INTEGER,
                created_at        TEXT NOT NULL,
                code              TEXT,
                visitor_ip        TEXT,
                identity_type     TEXT,
                device_type       TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_links_code ON links(code) WHERE is_active = 1")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_links_owner ON links(owner_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_routes_link_id ON routes(link_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_access_logs_link_time ON access_logs(link_id, created_at)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_access_logs_code ON access_logs(code)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_access_logs_identity_type ON access_logs(identity_type)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_access_logs_device_type ON access_logs(device_type)")
            .execute(&self.pool)
            .await?;

        tracing::info!("SQLite tables initialized");
        Ok(())
    }
}

// SQLite 行结构（用于查询结果映射）
#[derive(sqlx::FromRow)]
struct LinkRow {
    id: String,
    code: String,
    payload: String,
    owner_id: String,
    created_at: String,
    updated_at: String,
    metadata: String,
    is_active: i32,
}

impl From<LinkRow> for Link {
    fn from(row: LinkRow) -> Self {
        Link {
            id: row.id,
            code: row.code,
            payload: serde_json::from_str(&row.payload).unwrap_or_default(),
            owner: row.owner_id,
            created_at: row.created_at.parse().unwrap_or_else(|_| Utc::now()),
            metadata: serde_json::from_str(&row.metadata).unwrap_or_default(),
            is_active: row.is_active != 0,
        }
    }
}

#[derive(sqlx::FromRow)]
struct RouteRow {
    id: String,
    link_id: String,
    rules: String,
    default_target: String,
    version: i32,
    created_at: String,
}

impl From<RouteRow> for Route {
    fn from(row: RouteRow) -> Self {
        Route {
            id: row.id,
            link_id: row.link_id,
            rules: serde_json::from_str(&row.rules).unwrap_or_default(),
            default_target: serde_json::from_str(&row.default_target).unwrap_or_else(|_| Target {
                action: Action::Redirect,
                params: serde_json::json!({}),
            }),
            version: row.version,
            created_at: row.created_at.parse().unwrap_or_else(|_| Utc::now()),
        }
    }
}

#[derive(sqlx::FromRow)]
struct ExtensionRow {
    id: String,
    ext_type: String,
    name: String,
    config: String,
    is_active: i32,
    created_at: String,
}

impl From<ExtensionRow> for Extension {
    fn from(row: ExtensionRow) -> Self {
        Extension {
            id: row.id,
            ext_type: row.ext_type,
            name: row.name,
            config: serde_json::from_str(&row.config).unwrap_or_default(),
            is_active: row.is_active != 0,
            created_at: row.created_at.parse().unwrap_or_else(|_| Utc::now()),
        }
    }
}

#[async_trait]
impl Store for SqliteStore {
    // ─── Link 操作 ───────────────────────────────────────────

    async fn create_link(&self, link: &Link) -> Result<Link, StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO links (id, code, payload, owner_id, created_at, updated_at, metadata, is_active)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&link.id)
        .bind(&link.code)
        .bind(serde_json::to_string(&link.payload)?)
        .bind(&link.owner)
        .bind(&now)
        .bind(&now)
        .bind(serde_json::to_string(&link.metadata)?)
        .bind(if link.is_active { 1 } else { 0 })
        .execute(&self.pool)
        .await?;

        Ok(link.clone())
    }

    async fn get_link_by_code(&self, code: &str) -> Result<Option<Link>, StoreError> {
        let row = sqlx::query_as::<_, LinkRow>(
            "SELECT * FROM links WHERE code = ? AND is_active = 1",
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Link::from))
    }

    async fn update_link(&self, code: &str, payload: &serde_json::Value, metadata: &serde_json::Value) -> Result<Link, StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE links SET payload = ?, metadata = ?, updated_at = ? WHERE code = ? AND is_active = 1",
        )
        .bind(serde_json::to_string(payload)?)
        .bind(serde_json::to_string(metadata)?)
        .bind(&now)
        .bind(code)
        .execute(&self.pool)
        .await?;

        self.get_link_by_code(code)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("Link '{}' not found", code)))
    }

    async fn delete_link(&self, code: &str) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE links SET is_active = 0, updated_at = ? WHERE code = ?",
        )
        .bind(&now)
        .bind(code)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound(format!("Link '{}' not found", code)));
        }
        Ok(())
    }

    async fn list_links(&self, offset: i64, limit: i64) -> Result<Vec<Link>, StoreError> {
        let rows = sqlx::query_as::<_, LinkRow>(
            "SELECT * FROM links WHERE is_active = 1 ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Link::from).collect())
    }

    async fn count_links(&self) -> Result<i64, StoreError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM links",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }

    async fn count_active_links(&self) -> Result<i64, StoreError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM links WHERE is_active = 1",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }

    // ─── Route 操作 ──────────────────────────────────────────

    async fn create_route(&self, route: &Route) -> Result<Route, StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO routes (id, link_id, rules, default_target, version, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&route.id)
        .bind(&route.link_id)
        .bind(serde_json::to_string(&route.rules)?)
        .bind(serde_json::to_string(&route.default_target)?)
        .bind(route.version)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(route.clone())
    }

    async fn get_route_by_link_id(&self, link_id: &str) -> Result<Option<Route>, StoreError> {
        let row = sqlx::query_as::<_, RouteRow>(
            "SELECT * FROM routes WHERE link_id = ? ORDER BY version DESC LIMIT 1",
        )
        .bind(link_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Route::from))
    }

    async fn update_route(&self, id: &str, route: &Route) -> Result<Route, StoreError> {
        sqlx::query(
            r#"
            UPDATE routes SET rules = ?, default_target = ?, version = version + 1
            WHERE id = ?
            "#,
        )
        .bind(serde_json::to_string(&route.rules)?)
        .bind(serde_json::to_string(&route.default_target)?)
        .bind(id)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, RouteRow>("SELECT * FROM routes WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok(Route::from(row))
    }

    async fn delete_route(&self, id: &str) -> Result<(), StoreError> {
        let result = sqlx::query("DELETE FROM routes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound(format!("Route '{}' not found", id)));
        }
        Ok(())
    }

    // ─── Extension 操作 ─────────────────────────────────────

    async fn register_extension(&self, ext: &Extension) -> Result<Extension, StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO extensions (id, ext_type, name, config, is_active, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&ext.id)
        .bind(&ext.ext_type)
        .bind(&ext.name)
        .bind(serde_json::to_string(&ext.config)?)
        .bind(if ext.is_active { 1 } else { 0 })
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(ext.clone())
    }

    async fn list_extensions(&self) -> Result<Vec<Extension>, StoreError> {
        let rows = sqlx::query_as::<_, ExtensionRow>(
            "SELECT * FROM extensions WHERE is_active = 1",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Extension::from).collect())
    }

    async fn get_extension_by_name(&self, name: &str) -> Result<Option<Extension>, StoreError> {
        let row = sqlx::query_as::<_, ExtensionRow>(
            "SELECT * FROM extensions WHERE name = ? AND is_active = 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Extension::from))
    }

    async fn delete_extension(&self, name: &str) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE extensions SET is_active = 0 WHERE name = ?",
        )
        .bind(name)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound(format!("Extension '{}' not found", name)));
        }
        Ok(())
    }

    // ─── Access Log 操作 ────────────────────────────────────

    async fn log_access(&self, log: &AccessLog) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO access_logs (id, link_id, context, matched_rule, action_taken, response_time_ms, created_at, code, visitor_ip, identity_type, device_type)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&log.id)
        .bind(&log.link_id)
        .bind(serde_json::to_string(&log.context)?)
        .bind(&log.matched_rule)
        .bind(&log.action_taken)
        .bind(log.response_time_ms)
        .bind(&now)
        .bind(&log.code)
        .bind(&log.visitor_ip)
        .bind(&log.identity_type)
        .bind(&log.device_type)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_link_stats(&self, link_id: &str) -> Result<LinkStats, StoreError> {
        // 首先获取链接信息
        let link = sqlx::query_as::<_, LinkRow>(
            "SELECT * FROM links WHERE id = ?",
        )
        .bind(link_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::NotFound(format!("Link '{}' not found", link_id)))?;

        let now = Utc::now();
        let _24h_ago = (now - chrono::Duration::hours(24)).to_rfc3339();
        let _7d_ago = (now - chrono::Duration::days(7)).to_rfc3339();

        // 总访问次数
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM access_logs WHERE link_id = ?",
        )
        .bind(link_id)
        .fetch_one(&self.pool)
        .await?;

        // 唯一访客数（按 visitor_ip 去重，回退到 context.identity.id）
        let unique: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(DISTINCT COALESCE(visitor_ip, json_extract(context, '$.identity.id'))) FROM access_logs WHERE link_id = ?"#,
        )
        .bind(link_id)
        .fetch_one(&self.pool)
        .await?;

        // 24h 访问次数
        let h24: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM access_logs WHERE link_id = ? AND created_at >= ?",
        )
        .bind(link_id)
        .bind(&_24h_ago)
        .fetch_one(&self.pool)
        .await?;

        // 7d 访问次数
        let d7: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM access_logs WHERE link_id = ? AND created_at >= ?",
        )
        .bind(link_id)
        .bind(&_7d_ago)
        .fetch_one(&self.pool)
        .await?;

        // Phase 2: 设备分布
        let device_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT COALESCE(device_type, 'unknown') as dt, COUNT(*) as cnt FROM access_logs WHERE link_id = ? GROUP BY device_type",
        )
        .bind(link_id)
        .fetch_all(&self.pool)
        .await?;

        let mut device_distribution = serde_json::Map::new();
        for (dt, cnt) in device_rows {
            device_distribution.insert(dt, serde_json::Value::Number(cnt.into()));
        }

        // Phase 2: 身份类型分布
        let identity_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT COALESCE(identity_type, 'unknown') as it, COUNT(*) as cnt FROM access_logs WHERE link_id = ? GROUP BY identity_type",
        )
        .bind(link_id)
        .fetch_all(&self.pool)
        .await?;

        let mut identity_distribution = serde_json::Map::new();
        for (it, cnt) in identity_rows {
            identity_distribution.insert(it, serde_json::Value::Number(cnt.into()));
        }

        Ok(LinkStats {
            link_id: link_id.to_string(),
            code: link.code,
            total_accesses: total.0,
            unique_identities: unique.0,
            accesses_24h: h24.0,
            accesses_7d: d7.0,
            device_distribution: serde_json::Value::Object(device_distribution),
            identity_distribution: serde_json::Value::Object(identity_distribution),
        })
    }

    async fn get_overview_stats(&self) -> Result<OverviewStats, StoreError> {
        let now = Utc::now();
        let today_start = now.format("%Y-%m-%dT00:00:00+00:00").to_string();

        // 总链接数
        let total_links: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM links",
        )
        .fetch_one(&self.pool)
        .await?;

        // 活跃链接数
        let active_links: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM links WHERE is_active = 1",
        )
        .fetch_one(&self.pool)
        .await?;

        // 总访问次数
        let total_accesses: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM access_logs",
        )
        .fetch_one(&self.pool)
        .await?;

        // 今日访问次数
        let accesses_today: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM access_logs WHERE created_at >= ?",
        )
        .bind(&today_start)
        .fetch_one(&self.pool)
        .await?;

        // 热门链接 Top 5
        let top_rows: Vec<(String, String, i64)> = sqlx::query_as(
            r#"SELECT COALESCE(al.code, l.code) as code, al.link_id, COUNT(*) as cnt
               FROM access_logs al
               LEFT JOIN links l ON al.link_id = l.id
               GROUP BY al.link_id
               ORDER BY cnt DESC
               LIMIT 5"#,
        )
        .fetch_all(&self.pool)
        .await?;

        let top_links: Vec<TopLink> = top_rows
            .into_iter()
            .map(|(code, link_id, access_count)| TopLink { code, link_id, access_count })
            .collect();

        Ok(OverviewStats {
            total_links: total_links.0,
            active_links: active_links.0,
            total_accesses: total_accesses.0,
            accesses_today: accesses_today.0,
            top_links,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlink_core::{Action, Target};

    #[tokio::test]
    async fn test_sqlite_store_crud() {
        // 使用内存数据库进行测试
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        // 创建链接
        let link = Link {
            id: uuid::Uuid::new_v4().to_string(),
            code: "test01".to_string(),
            payload: serde_json::json!({"url": "https://example.com"}),
            owner: "test-user".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
            is_active: true,
        };

        let created = store.create_link(&link).await.unwrap();
        assert_eq!(created.code, "test01");

        // 查询链接
        let found = store.get_link_by_code("test01").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().owner, "test-user");

        // 更新链接
        let updated = store
            .update_link("test01", &serde_json::json!({"url": "https://updated.com"}), &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(updated.payload["url"], "https://updated.com");

        // 删除链接（软删除）
        store.delete_link("test01").await.unwrap();
        let found = store.get_link_by_code("test01").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_sqlite_store_route() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        // 先创建链接
        let link = Link {
            id: "link-rtest".to_string(),
            code: "rtest01".to_string(),
            payload: serde_json::json!({}),
            owner: "test-user".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
            is_active: true,
        };
        store.create_link(&link).await.unwrap();

        // 创建路由
        let route = Route {
            id: "route-1".to_string(),
            link_id: "link-rtest".to_string(),
            rules: vec![],
            default_target: Target {
                action: Action::Redirect,
                params: serde_json::json!({"url": "https://example.com", "status_code": 302}),
            },
            version: 1,
            created_at: Utc::now(),
        };

        store.create_route(&route).await.unwrap();

        // 查询路由
        let found = store.get_route_by_link_id("link-rtest").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().default_target.action, Action::Redirect);
    }

    #[tokio::test]
    async fn test_sqlite_store_extension() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        let ext = Extension {
            id: uuid::Uuid::new_v4().to_string(),
            ext_type: "action".to_string(),
            name: "redirect".to_string(),
            config: serde_json::json!({"status_code": 302}),
            is_active: true,
            created_at: Utc::now(),
        };

        store.register_extension(&ext).await.unwrap();

        let exts = store.list_extensions().await.unwrap();
        assert_eq!(exts.len(), 1);

        let found = store.get_extension_by_name("redirect").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_sqlite_duplicate_code() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        let link1 = Link {
            id: uuid::Uuid::new_v4().to_string(),
            code: "dup01".to_string(),
            payload: serde_json::json!({}),
            owner: "user1".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
            is_active: true,
        };

        let link2 = Link {
            id: uuid::Uuid::new_v4().to_string(),
            code: "dup01".to_string(), // 同一短码
            payload: serde_json::json!({}),
            owner: "user2".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
            is_active: true,
        };

        store.create_link(&link1).await.unwrap();
        let result = store.create_link(&link2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sqlite_list_links() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        for i in 0..5 {
            let link = Link {
                id: uuid::Uuid::new_v4().to_string(),
                code: format!("list{}", i),
                payload: serde_json::json!({}),
                owner: "test-user".to_string(),
                created_at: Utc::now(),
                metadata: serde_json::json!({}),
                is_active: true,
            };
            store.create_link(&link).await.unwrap();
        }

        let links = store.list_links(0, 3).await.unwrap();
        assert_eq!(links.len(), 3);

        let count = store.count_active_links().await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_sqlite_overview_stats() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        let stats = store.get_overview_stats().await.unwrap();
        assert_eq!(stats.total_links, 0);
        assert_eq!(stats.total_accesses, 0);
    }

    #[tokio::test]
    async fn test_sqlite_access_log_with_enhanced_fields() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        let link = Link {
            id: "link-alog".to_string(),
            code: "alog01".to_string(),
            payload: serde_json::json!({}),
            owner: "test-user".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
            is_active: true,
        };
        store.create_link(&link).await.unwrap();

        let log = AccessLog {
            id: uuid::Uuid::new_v4().to_string(),
            link_id: "link-alog".to_string(),
            context: serde_json::json!({"code": "alog01"}),
            matched_rule: None,
            action_taken: "redirect".to_string(),
            response_time_ms: Some(42),
            created_at: Utc::now(),
            code: Some("alog01".to_string()),
            visitor_ip: Some("127.0.0.1".to_string()),
            identity_type: Some("human".to_string()),
            device_type: Some("desktop".to_string()),
        };
        store.log_access(&log).await.unwrap();

        let stats = store.get_link_stats("link-alog").await.unwrap();
        assert_eq!(stats.total_accesses, 1);
        assert_eq!(stats.code, "alog01");
    }
}
