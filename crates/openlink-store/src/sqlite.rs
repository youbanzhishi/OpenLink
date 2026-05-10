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
//! - 新增 list_links
//! - 新增 get_overview_stats
//! - 增强 get_link_stats（设备/身份分布）

use crate::error::StoreError;
use crate::traits::Store;
use async_trait::async_trait;
use chrono::Utc;
use openlink_core::{
    AccessLog, Action, Extension, Link, LinkStats, OverviewStats, Route, Target, TopLink,
};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

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

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_access_logs_device_type ON access_logs(device_type)",
        )
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

#[derive(sqlx::FromRow)]
struct AccessLogRow {
    id: String,
    link_id: String,
    context: String,
    matched_rule: Option<String>,
    action_taken: String,
    response_time_ms: Option<i32>,
    created_at: String,
    code: Option<String>,
    visitor_ip: Option<String>,
    identity_type: Option<String>,
    device_type: Option<String>,
}

impl From<AccessLogRow> for AccessLog {
    fn from(row: AccessLogRow) -> Self {
        AccessLog {
            id: row.id,
            link_id: row.link_id,
            context: serde_json::from_str(&row.context).unwrap_or_default(),
            matched_rule: row.matched_rule,
            action_taken: row.action_taken,
            response_time_ms: row.response_time_ms.map(|v| v as i64),
            created_at: row.created_at.parse().unwrap_or_else(|_| Utc::now()),
            code: row.code,
            visitor_ip: row.visitor_ip,
            identity_type: row.identity_type,
            device_type: row.device_type,
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

    async fn get_link(&self, id: &str) -> Result<Option<Link>, StoreError> {
        let row = sqlx::query_as::<_, LinkRow>("SELECT * FROM links WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(Link::from))
    }

    async fn get_link_by_code(&self, code: &str) -> Result<Option<Link>, StoreError> {
        let row =
            sqlx::query_as::<_, LinkRow>("SELECT * FROM links WHERE code = ? AND is_active = 1")
                .bind(code)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(Link::from))
    }

    async fn update_link(&self, link: &Link) -> Result<Link, StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE links SET code = ?, payload = ?, metadata = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&link.code)
        .bind(serde_json::to_string(&link.payload)?)
        .bind(serde_json::to_string(&link.metadata)?)
        .bind(&now)
        .bind(&link.id)
        .execute(&self.pool)
        .await?;

        self.get_link(&link.id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("Link '{}' not found", link.id)))
    }

    async fn delete_link(&self, id: &str) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query("UPDATE links SET is_active = 0, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound(format!("Link '{}' not found", id)));
        }
        Ok(())
    }

    async fn list_links(&self, owner: Option<&str>, limit: usize) -> Result<Vec<Link>, StoreError> {
        let rows = match owner {
            Some(owner_id) => {
                sqlx::query_as::<_, LinkRow>(
                    "SELECT * FROM links WHERE is_active = 1 AND owner_id = ? ORDER BY created_at DESC LIMIT ?",
                )
                .bind(owner_id)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, LinkRow>(
                    "SELECT * FROM links WHERE is_active = 1 ORDER BY created_at DESC LIMIT ?",
                )
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows.into_iter().map(Link::from).collect())
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

    async fn get_route(&self, id: &str) -> Result<Option<Route>, StoreError> {
        let row = sqlx::query_as::<_, RouteRow>("SELECT * FROM routes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(Route::from))
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

    async fn update_route(&self, route: &Route) -> Result<Route, StoreError> {
        sqlx::query(
            r#"
            UPDATE routes SET rules = ?, default_target = ?, version = version + 1
            WHERE id = ?
            "#,
        )
        .bind(serde_json::to_string(&route.rules)?)
        .bind(serde_json::to_string(&route.default_target)?)
        .bind(&route.id)
        .execute(&self.pool)
        .await?;

        self.get_route(&route.id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("Route '{}' not found", route.id)))
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

    async fn list_routes(&self, link_id: Option<&str>) -> Result<Vec<Route>, StoreError> {
        let rows = match link_id {
            Some(lid) => {
                sqlx::query_as::<_, RouteRow>(
                    "SELECT * FROM routes WHERE link_id = ? ORDER BY version DESC",
                )
                .bind(lid)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, RouteRow>("SELECT * FROM routes ORDER BY created_at DESC")
                    .fetch_all(&self.pool)
                    .await?
            }
        };

        Ok(rows.into_iter().map(Route::from).collect())
    }

    // ─── Extension 操作 ─────────────────────────────────────

    async fn list_extensions(&self) -> Result<Vec<Extension>, StoreError> {
        let rows =
            sqlx::query_as::<_, ExtensionRow>("SELECT * FROM extensions WHERE is_active = 1")
                .fetch_all(&self.pool)
                .await?;

        Ok(rows.into_iter().map(Extension::from).collect())
    }

    async fn save_extension(&self, ext: &Extension) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO extensions (id, ext_type, name, config, is_active, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                ext_type = excluded.ext_type,
                config = excluded.config,
                is_active = excluded.is_active
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

    async fn get_access_logs(
        &self,
        link_id: &str,
        limit: usize,
    ) -> Result<Vec<AccessLog>, StoreError> {
        let rows = sqlx::query_as::<_, AccessLogRow>(
            "SELECT * FROM access_logs WHERE link_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(link_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(AccessLog::from).collect())
    }

    async fn get_link_stats(&self, link_id: &str) -> Result<LinkStats, StoreError> {
        // 首先获取链接信息
        let link = sqlx::query_as::<_, LinkRow>("SELECT * FROM links WHERE id = ?")
            .bind(link_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("Link '{}' not found", link_id)))?;

        let now = Utc::now();
        let _24h_ago = (now - chrono::Duration::hours(24)).to_rfc3339();
        let _7d_ago = (now - chrono::Duration::days(7)).to_rfc3339();

        // 总访问次数
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM access_logs WHERE link_id = ?")
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
        let total_links: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links")
            .fetch_one(&self.pool)
            .await?;

        // 活跃链接数
        let active_links: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links WHERE is_active = 1")
            .fetch_one(&self.pool)
            .await?;

        // 总访问次数
        let total_accesses: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM access_logs")
            .fetch_one(&self.pool)
            .await?;

        // 今日访问次数
        let accesses_today: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM access_logs WHERE created_at >= ?")
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
            .map(|(code, link_id, access_count)| TopLink {
                code,
                link_id,
                access_count,
            })
            .collect();

        Ok(OverviewStats {
            total_links: total_links.0,
            active_links: active_links.0,
            total_accesses: total_accesses.0,
            accesses_today: accesses_today.0,
            top_links,
        })
    }

    // ─── Health Check (Phase 5) ─────────────────────────────

    async fn health_check(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await?;
        Ok(())
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
        let mut updated_link = link.clone();
        updated_link.payload = serde_json::json!({"url": "https://updated.com"});
        let updated = store.update_link(&updated_link).await.unwrap();
        assert_eq!(updated.payload["url"], "https://updated.com");

        // 删除链接（软删除）
        store.delete_link(&link.id).await.unwrap();
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

        store.save_extension(&ext).await.unwrap();

        let exts = store.list_extensions().await.unwrap();
        assert_eq!(exts.len(), 1);
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

        // 测试 list_links(owner, limit)
        let links = store.list_links(Some("test-user"), 3).await.unwrap();
        assert_eq!(links.len(), 3);

        // 测试 list_links(None, limit) 获取所有
        let all_links = store.list_links(None, 10).await.unwrap();
        assert_eq!(all_links.len(), 5);
    }

    #[tokio::test]
    async fn test_sqlite_list_routes() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        // 先创建链接
        let link = Link {
            id: "link-routes".to_string(),
            code: "rt01".to_string(),
            payload: serde_json::json!({}),
            owner: "test-user".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
            is_active: true,
        };
        store.create_link(&link).await.unwrap();

        // 创建多个路由
        for i in 0..3 {
            let route = Route {
                id: format!("route-{}", i),
                link_id: "link-routes".to_string(),
                rules: vec![],
                default_target: Target {
                    action: Action::Redirect,
                    params: serde_json::json!({"url": format!("https://example{}.com", i)}),
                },
                version: 1,
                created_at: Utc::now(),
            };
            store.create_route(&route).await.unwrap();
        }

        // 测试 list_routes(Some(link_id))
        let routes = store.list_routes(Some("link-routes")).await.unwrap();
        assert_eq!(routes.len(), 3);

        // 测试 list_routes(None) 获取所有
        let all_routes = store.list_routes(None).await.unwrap();
        assert!(all_routes.len() >= 3);
    }

    #[tokio::test]
    async fn test_sqlite_get_access_logs() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();

        // 先创建链接
        let link = Link {
            id: "link-logs".to_string(),
            code: "log01".to_string(),
            payload: serde_json::json!({}),
            owner: "test-user".to_string(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
            is_active: true,
        };
        store.create_link(&link).await.unwrap();

        // 创建多条访问日志
        for i in 0..5 {
            let log = AccessLog {
                id: uuid::Uuid::new_v4().to_string(),
                link_id: "link-logs".to_string(),
                context: serde_json::json!({"code": "log01"}),
                matched_rule: None,
                action_taken: "redirect".to_string(),
                response_time_ms: Some(42 + i),
                created_at: Utc::now(),
                code: Some("log01".to_string()),
                visitor_ip: Some(format!("127.0.0.{}", i)),
                identity_type: Some("human".to_string()),
                device_type: Some("desktop".to_string()),
            };
            store.log_access(&log).await.unwrap();
        }

        // 测试 get_access_logs
        let logs = store.get_access_logs("link-logs", 3).await.unwrap();
        assert_eq!(logs.len(), 3);
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

    #[tokio::test]
    async fn test_sqlite_health_check() {
        let store = SqliteStore::new("sqlite::memory:").await.unwrap();
        let result = store.health_check().await;
        assert!(result.is_ok());
    }
}
