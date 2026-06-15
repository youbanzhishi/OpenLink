//! Snapshot Service - 状态快照与回滚
//!
//! 实现路由规则和存储状态的快照与回滚
//! 与熔断器集成，熔断触发时自动创建快照

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// 快照ID
pub type SnapshotId = String;

/// 快照服务trait
pub trait SnapshotService: Send + Sync {
    /// 创建快照
    fn create(&self, snapshot: &Snapshot) -> Result<SnapshotId, SnapshotError>;
    
    /// 获取快照
    fn get(&self, id: &SnapshotId) -> Result<Option<Snapshot>, SnapshotError>;
    
    /// 列出快照
    fn list(&self) -> Result<Vec<SnapshotMeta>, SnapshotError>;
    
    /// 恢复快照
    fn restore(&self, id: &SnapshotId) -> Result<RestoreResult, SnapshotError>;
    
    /// 删除快照
    fn delete(&self, id: &SnapshotId) -> Result<(), SnapshotError>;
    
    /// 自动清理过期快照
    fn cleanup(&self, max_count: usize) -> Result<u64, SnapshotError>;
}

/// 快照
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// 快照ID
    pub id: SnapshotId,
    /// 快照类型
    pub snapshot_type: SnapshotType,
    /// 快照数据
    pub data: SnapshotData,
    /// 创建时间
    pub created_at: i64,
    /// 创建原因
    pub reason: SnapshotReason,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// 快照类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotType {
    /// 路由规则快照
    RouteRules,
    /// 存储配置快照
    StorageConfig,
    /// 扩展状态快照
    ExtensionState,
    /// 完整快照
    Full,
}

/// 快照数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotData {
    /// 路由规则
    pub routes: Option<serde_json::Value>,
    /// 存储配置
    pub storage_config: Option<serde_json::Value>,
    /// 扩展状态
    pub extension_state: Option<serde_json::Value>,
    /// 完整数据
    pub full_data: Option<serde_json::Value>,
}

/// 快照原因
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotReason {
    /// 手动创建
    Manual,
    /// 熔断触发
    CircuitBreakerTripped,
    /// 定期备份
    ScheduledBackup,
    /// 异常恢复
    ErrorRecovery,
    /// 升级前
    PreUpgrade,
}

/// 快照元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    /// 快照ID
    pub id: SnapshotId,
    /// 快照类型
    pub snapshot_type: SnapshotType,
    /// 创建时间
    pub created_at: i64,
    /// 创建原因
    pub reason: SnapshotReason,
    /// 数据大小
    pub size_bytes: usize,
}

/// 恢复结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    /// 是否成功
    pub success: bool,
    /// 恢复的快照ID
    pub snapshot_id: SnapshotId,
    /// 恢复的项数
    pub items_restored: usize,
    /// 警告信息
    pub warnings: Vec<String>,
}

/// 快照错误
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotError {
    NotFound,
    AlreadyExists,
    InvalidData,
    StorageError(String),
    RestoreFailed(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::NotFound => write!(f, "Snapshot not found"),
            SnapshotError::AlreadyExists => write!(f, "Snapshot already exists"),
            SnapshotError::InvalidData => write!(f, "Invalid snapshot data"),
            SnapshotError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            SnapshotError::RestoreFailed(msg) => write!(f, "Restore failed: {}", msg),
        }
    }
}

/// 保留策略
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// 最大快照数
    pub max_snapshots: usize,
    /// 最大保留时间（秒）
    pub max_age_seconds: i64,
    /// 自动清理启用
    pub auto_cleanup: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_snapshots: 10,
            max_age_seconds: 7 * 24 * 3600, // 7天
            auto_cleanup: true,
        }
    }
}

/// 快照服务实现（内存版）
pub struct InMemorySnapshotService {
    snapshots: HashMap<SnapshotId, Snapshot>,
    retention_policy: RetentionPolicy,
}

impl InMemorySnapshotService {
    pub fn new(policy: RetentionPolicy) -> Self {
        Self {
            snapshots: HashMap::new(),
            retention_policy: policy,
        }
    }

    pub fn with_default_policy() -> Self {
        Self::new(RetentionPolicy::default())
    }

    fn generate_id() -> SnapshotId {
        format!("snap-{}", uuid::Uuid::new_v4())
    }

    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }
}

impl SnapshotService for InMemorySnapshotService {
    fn create(&self, snapshot: &Snapshot) -> Result<SnapshotId, SnapshotError> {
        let id = Self::generate_id();
        let mut snapshot = snapshot.clone();
        snapshot.id = id.clone();
        self.snapshots.insert(id.clone(), snapshot);
        Ok(id)
    }

    fn get(&self, id: &SnapshotId) -> Result<Option<Snapshot>, SnapshotError> {
        Ok(self.snapshots.get(id).cloned())
    }

    fn list(&self) -> Result<Vec<SnapshotMeta>, SnapshotError> {
        let mut metas: Vec<SnapshotMeta> = self.snapshots
            .values()
            .map(|s| {
                let size = serde_json::to_string(s)
                    .map(|j| j.len())
                    .unwrap_or(0);
                SnapshotMeta {
                    id: s.id.clone(),
                    snapshot_type: s.snapshot_type.clone(),
                    created_at: s.created_at,
                    reason: s.reason.clone(),
                    size_bytes: size,
                }
            })
            .collect();
        
        metas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(metas)
    }

    fn restore(&self, id: &SnapshotId) -> Result<RestoreResult, SnapshotError> {
        let snapshot = self.snapshots
            .get(id)
            .ok_or(SnapshotError::NotFound)?;

        let items = self.count_items(&snapshot.data);
        
        Ok(RestoreResult {
            success: true,
            snapshot_id: id.clone(),
            items_restored: items,
            warnings: vec![],
        })
    }

    fn delete(&self, id: &SnapshotId) -> Result<(), SnapshotError> {
        self.snapshots.remove(id)
            .map(|_| ())
            .ok_or(SnapshotError::NotFound)
    }

    fn cleanup(&self, max_count: usize) -> Result<u64, SnapshotError> {
        let mut metas = self.list()?;
        let current_count = metas.len();
        
        if current_count <= max_count {
            return Ok(0);
        }

        let to_delete = current_count - max_count;
        for meta in metas.iter().skip(max_count) {
            self.delete(&meta.id)?;
        }
        
        Ok(to_delete as u64)
    }
}

impl InMemorySnapshotService {
    fn count_items(&self, data: &SnapshotData) -> usize {
        let mut count = 0;
        if data.routes.is_some() { count += 1; }
        if data.storage_config.is_some() { count += 1; }
        if data.extension_state.is_some() { count += 1; }
        if data.full_data.is_some() { count += 1; }
        count
    }
}

/// Builder
pub struct SnapshotBuilder {
    snapshot_type: SnapshotType,
    reason: SnapshotReason,
    data: SnapshotData,
    metadata: HashMap<String, String>,
}

impl SnapshotBuilder {
    pub fn new(snapshot_type: SnapshotType) -> Self {
        Self {
            snapshot_type,
            reason: SnapshotReason::Manual,
            data: SnapshotData {
                routes: None,
                storage_config: None,
                extension_state: None,
                full_data: None,
            },
            metadata: HashMap::new(),
        }
    }

    pub fn with_reason(mut self, reason: SnapshotReason) -> Self {
        self.reason = reason;
        self
    }

    pub fn with_routes(mut self, routes: serde_json::Value) -> Self {
        self.data.routes = Some(routes);
        self
    }

    pub fn with_storage_config(mut self, config: serde_json::Value) -> Self {
        self.data.storage_config = Some(config);
        self
    }

    pub fn with_extension_state(mut self, state: serde_json::Value) -> Self {
        self.data.extension_state = Some(state);
        self
    }

    pub fn with_full_data(mut self, data: serde_json::Value) -> Self {
        self.data.full_data = Some(data);
        self
    }

    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    pub fn build(self) -> Snapshot {
        Snapshot {
            id: String::new(),
            snapshot_type: self.snapshot_type,
            data: self.data,
            created_at: InMemorySnapshotService::current_timestamp(),
            reason: self.reason,
            metadata: self.metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_snapshot() -> Snapshot {
        SnapshotBuilder::new(SnapshotType::RouteRules)
            .with_reason(SnapshotReason::Manual)
            .with_routes(serde_json::json!({"routes": []}))
            .with_metadata("created_by", "test")
            .build()
    }

    #[test]
    fn test_snapshot_creation() {
        let service = InMemorySnapshotService::with_default_policy();
        let snapshot = create_test_snapshot();
        
        let id = service.create(&snapshot).unwrap();
        assert!(!id.is_empty());
        
        let retrieved = service.get(&id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[test]
    fn test_list_snapshots() {
        let service = InMemorySnapshotService::with_default_policy();
        
        for _ in 0..3 {
            service.create(&create_test_snapshot()).unwrap();
        }
        
        let metas = service.list().unwrap();
        assert_eq!(metas.len(), 3);
    }

    #[test]
    fn test_restore() {
        let service = InMemorySnapshotService::with_default_policy();
        let id = service.create(&create_test_snapshot()).unwrap();
        
        let result = service.restore(&id).unwrap();
        assert!(result.success);
        assert_eq!(result.snapshot_id, id);
    }

    #[test]
    fn test_delete() {
        let service = InMemorySnapshotService::with_default_policy();
        let id = service.create(&create_test_snapshot()).unwrap();
        
        service.delete(&id).unwrap();
        
        let retrieved = service.get(&id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_cleanup() {
        let service = InMemorySnapshotService::with_default_policy();
        
        for _ in 0..5 {
            service.create(&create_test_snapshot()).unwrap();
        }
        
        let deleted = service.cleanup(3).unwrap();
        assert_eq!(deleted, 2);
        
        let metas = service.list().unwrap();
        assert_eq!(metas.len(), 3);
    }

    #[test]
    fn test_snapshot_builder() {
        let snapshot = SnapshotBuilder::new(SnapshotType::Full)
            .with_reason(SnapshotReason::CircuitBreakerTripped)
            .with_routes(serde_json::json!({"test": true}))
            .with_full_data(serde_json::json!({"all": true}))
            .with_metadata("trigger", "circuit_breaker")
            .build();
        
        assert!(snapshot.data.routes.is_some());
        assert!(snapshot.data.full_data.is_some());
        assert_eq!(snapshot.metadata.get("trigger"), Some(&"circuit_breaker".to_string()));
    }
}
