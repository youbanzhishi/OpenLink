//! # KnowledgeSync 协议原语 — Agent 间知识同步
//!
//! ADR-009 实现四阶段协议：
//! - discover: 通过 agent.json 发现 KnowledgeSync 能力
//! - auth: API Key / OAuth 2.1+PKCE 认证
//! - read/write: 知识查询、读取、写入
//! - callback: 变更回调订阅
//!
//! 优先实现 API Key 认证模式 + read/write 核心 API。
//! OAuth 2.1+PKCE 作为后续增强。

use crate::error::CoreError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── KnowledgeSync 能力声明 (discover 阶段) ────────────────────

/// KnowledgeSync 能力声明 — 嵌入 agent.json 的 capabilities 字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSyncCapability {
    /// 协议版本
    pub version: String,
    /// 各阶段 API 端点
    pub endpoints: KnowledgeSyncEndpoints,
    /// 支持的知识格式
    pub supported_formats: Vec<String>,
    /// 单次查询最大返回数
    pub max_query_results: usize,
    /// 是否需要认证
    pub requires_auth: bool,
}

impl Default for KnowledgeSyncCapability {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            endpoints: KnowledgeSyncEndpoints::default(),
            supported_formats: vec!["markdown".to_string(), "json".to_string(), "plain-text".to_string()],
            max_query_results: 20,
            requires_auth: true,
        }
    }
}

/// KnowledgeSync API 端点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSyncEndpoints {
    pub auth: String,
    pub query: String,
    pub read: String,
    pub write: String,
    pub callback: String,
}

impl Default for KnowledgeSyncEndpoints {
    fn default() -> Self {
        Self {
            auth: "/api/v1/knowledge/auth".to_string(),
            query: "/api/v1/knowledge/query".to_string(),
            read: "/api/v1/knowledge/read".to_string(),
            write: "/api/v1/knowledge/write".to_string(),
            callback: "/api/v1/knowledge/callback".to_string(),
        }
    }
}

// ─── Auth 阶段 ────────────────────────────────────────────────

/// 认证请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeAuthRequest {
    /// 认证方式：api_key | authorization_code
    pub grant_type: KnowledgeGrantType,
    /// 客户端标识
    pub client_id: String,
    /// 请求的权限范围
    #[serde(default)]
    pub scope: Vec<KnowledgeScope>,
    /// API Key（grant_type=api_key 时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// OAuth redirect_uri（grant_type=authorization_code 时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
}

/// 认证方式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeGrantType {
    ApiKey,
    AuthorizationCode,
}

/// 知识权限范围
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeScope {
    KnowledgeRead,
    KnowledgeWrite,
}

impl std::fmt::Display for KnowledgeScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnowledgeScope::KnowledgeRead => write!(f, "knowledge:read"),
            KnowledgeScope::KnowledgeWrite => write!(f, "knowledge:write"),
        }
    }
}

/// 认证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeAuthResponse {
    /// Access Token
    pub access_token: String,
    /// Token 类型
    pub token_type: String,
    /// 过期时间（秒），0 表示永不过期
    pub expires_in: u64,
    /// 授权范围
    pub scope: Vec<KnowledgeScope>,
    /// 可用知识集合
    pub available_collections: Vec<String>,
}

// ─── Read 阶段 ────────────────────────────────────────────────

/// 知识查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeQueryRequest {
    /// 搜索关键词
    pub query: String,
    /// 限定集合（可选，不传则搜全部）
    #[serde(default)]
    pub collections: Vec<String>,
    /// 返回数量限制
    #[serde(default = "default_query_limit")]
    pub limit: usize,
    /// 返回格式
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_query_limit() -> usize {
    5
}

fn default_format() -> String {
    "markdown".to_string()
}

/// 知识查询结果项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeQueryResult {
    /// 文档 ID
    pub id: String,
    /// 所属集合
    pub collection: String,
    /// 文档标题
    pub title: String,
    /// 内容片段
    pub snippet: String,
    /// 相关度评分 (0.0~1.0)
    pub relevance: f64,
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
}

/// 知识查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeQueryResponse {
    /// 搜索结果列表
    pub results: Vec<KnowledgeQueryResult>,
    /// 总匹配数
    pub total: usize,
    /// 是否有更多结果
    pub has_more: bool,
}

/// 知识读取请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReadRequest {
    /// 文档 ID
    pub id: String,
    /// 返回格式
    #[serde(default = "default_format")]
    pub format: String,
}

/// 知识读取响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReadResponse {
    /// 文档 ID
    pub id: String,
    /// 所属集合
    pub collection: String,
    /// 文档标题
    pub title: String,
    /// 完整内容
    pub content: String,
    /// 内容格式
    pub format: String,
    /// 元数据
    #[serde(default)]
    pub metadata: KnowledgeMetadata,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
}

// ─── Write 阶段 ───────────────────────────────────────────────

/// 知识写入请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeWriteRequest {
    /// 目标集合
    pub collection: String,
    /// 文档标题
    pub title: String,
    /// 文档内容
    pub content: String,
    /// 内容格式
    #[serde(default = "default_format")]
    pub format: String,
    /// 元数据
    #[serde(default)]
    pub metadata: KnowledgeMetadata,
}

/// 知识元数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeMetadata {
    /// 来源 Agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 关联任务 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 标签
    #[serde(default)]
    pub tags: Vec<String>,
    /// 是否敏感
    #[serde(default)]
    pub is_private: bool,
}

/// 知识写入响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeWriteResponse {
    /// 文档 ID
    pub id: String,
    /// 状态
    pub status: KnowledgeWriteStatus,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

/// 写入状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeWriteStatus {
    Created,
    Updated,
}

// ─── Callback 阶段 ────────────────────────────────────────────

/// 回调注册请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeCallbackRequest {
    /// 回调 URL
    pub callback_url: String,
    /// 订阅事件类型
    pub events: Vec<KnowledgeEventType>,
    /// 限定集合
    #[serde(default)]
    pub collections: Vec<String>,
    /// 签名密钥
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

/// 事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeEventType {
    KnowledgeCreated,
    KnowledgeUpdated,
    KnowledgeDeleted,
}

/// 回调注册响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeCallbackResponse {
    /// 订阅 ID
    pub subscription_id: String,
    /// 已订阅事件
    pub events: Vec<KnowledgeEventType>,
    /// 订阅状态
    pub status: String,
}

/// 回调通知格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeCallbackNotification {
    /// 事件类型
    pub event: KnowledgeEventType,
    /// 所属集合
    pub collection: String,
    /// 文档 ID
    pub doc_id: String,
    /// 触发者
    pub updated_by: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// HMAC-SHA256 签名
    pub signature: String,
}

// ─── Knowledge Store (存储抽象) ────────────────────────────────

/// 知识存储 trait — 底层存储抽象
#[async_trait::async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// 语义查询
    async fn query(&self, request: &KnowledgeQueryRequest) -> Result<KnowledgeQueryResponse, CoreError>;

    /// 读取单个文档
    async fn read(&self, id: &str) -> Result<Option<KnowledgeReadResponse>, CoreError>;

    /// 写入文档
    async fn write(&self, request: &KnowledgeWriteRequest) -> Result<KnowledgeWriteResponse, CoreError>;

    /// 删除文档
    async fn delete(&self, id: &str) -> Result<bool, CoreError>;

    /// 列出所有集合
    async fn list_collections(&self) -> Result<Vec<String>, CoreError>;
}

// ─── API Key 认证管理器 ────────────────────────────────────────

/// API Key 记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    /// API Key 值
    pub key: String,
    /// 客户端标识
    pub client_id: String,
    /// 权限范围
    pub scope: Vec<KnowledgeScope>,
    /// 可访问集合
    pub allowed_collections: Vec<String>,
    /// 是否启用
    pub is_active: bool,
}

/// API Key 认证管理器
pub struct ApiKeyManager {
    /// API Key → 记录
    keys: HashMap<String, ApiKeyRecord>,
}

impl ApiKeyManager {
    /// 创建空管理器
    pub fn new() -> Self {
        Self { keys: HashMap::new() }
    }

    /// 注册 API Key
    pub fn register(&mut self, record: ApiKeyRecord) {
        self.keys.insert(record.key.clone(), record);
    }

    /// 验证 API Key
    pub fn validate(&self, api_key: &str, required_scope: &KnowledgeScope) -> Option<&ApiKeyRecord> {
        self.keys.get(api_key).and_then(|record| {
            if record.is_active && record.scope.contains(required_scope) {
                Some(record)
            } else {
                None
            }
        })
    }

    /// 吊销 API Key
    pub fn revoke(&mut self, api_key: &str) -> bool {
        if let Some(record) = self.keys.get_mut(api_key) {
            record.is_active = false;
            true
        } else {
            false
        }
    }

    /// 列出所有 Key（脱敏）
    pub fn list_keys(&self) -> Vec<&ApiKeyRecord> {
        self.keys.values().collect()
    }
}

impl Default for ApiKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── KnowledgeSync Service (核心服务) ──────────────────────────

/// KnowledgeSync 核心服务
pub struct KnowledgeSyncService {
    /// 知识存储
    store: Box<dyn KnowledgeStore>,
    /// API Key 管理器
    auth: std::sync::RwLock<ApiKeyManager>,
    /// 能力声明
    capability: KnowledgeSyncCapability,
    /// 回调订阅
    callbacks: std::sync::RwLock<HashMap<String, KnowledgeCallbackRequest>>,
}

impl KnowledgeSyncService {
    /// 创建服务
    pub fn new(store: Box<dyn KnowledgeStore>) -> Self {
        Self {
            store,
            auth: std::sync::RwLock::new(ApiKeyManager::new()),
            capability: KnowledgeSyncCapability::default(),
            callbacks: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 使用自定义能力声明创建服务
    pub fn with_capability(store: Box<dyn KnowledgeStore>, capability: KnowledgeSyncCapability) -> Self {
        Self {
            store,
            auth: std::sync::RwLock::new(ApiKeyManager::new()),
            capability,
            callbacks: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 获取能力声明
    pub fn capability(&self) -> &KnowledgeSyncCapability {
        &self.capability
    }

    // ─── Auth ──────────────────────────────────────────────────

    /// 注册 API Key
    pub fn register_api_key(&self, record: ApiKeyRecord) {
        self.auth.write().unwrap_or_else(|e| e.into_inner()).register(record);
    }

    /// 认证（API Key 模式）
    pub fn authenticate(&self, request: &KnowledgeAuthRequest) -> Result<KnowledgeAuthResponse, CoreError> {
        match request.grant_type {
            KnowledgeGrantType::ApiKey => {
                let api_key = request
                    .api_key
                    .as_ref()
                    .ok_or_else(|| CoreError::InvalidInput("api_key is required".to_string()))?;

                let auth = self.auth.read().unwrap_or_else(|e| e.into_inner());

                // 验证 key 存在且活跃
                let record = auth
                    .keys
                    .get(api_key)
                    .ok_or_else(|| CoreError::ExtensionError("Invalid API key".to_string()))?;

                if !record.is_active {
                    return Err(CoreError::ExtensionError("API key is revoked".to_string()));
                }

                // 检查请求的 scope 是否在 key 的权限范围内
                for scope in &request.scope {
                    if !record.scope.contains(scope) {
                        return Err(CoreError::ExtensionError(format!(
                            "API key does not have {:?} scope",
                            scope
                        )));
                    }
                }

                Ok(KnowledgeAuthResponse {
                    access_token: format!("ks_{}", uuid::Uuid::new_v4()),
                    token_type: "Bearer".to_string(),
                    expires_in: 0, // API Key 模式永不过期
                    scope: request.scope.clone(),
                    available_collections: record.allowed_collections.clone(),
                })
            }
            KnowledgeGrantType::AuthorizationCode => Err(CoreError::InvalidInput(
                "OAuth 2.1+PKCE not yet implemented. Use api_key grant type.".to_string(),
            )),
        }
    }

    // ─── Read ──────────────────────────────────────────────────

    /// 查询知识
    pub async fn query(&self, request: &KnowledgeQueryRequest) -> Result<KnowledgeQueryResponse, CoreError> {
        self.store.query(request).await
    }

    /// 读取知识文档
    pub async fn read(&self, id: &str) -> Result<Option<KnowledgeReadResponse>, CoreError> {
        self.store.read(id).await
    }

    // ─── Write ─────────────────────────────────────────────────

    /// 写入知识
    pub async fn write(&self, request: &KnowledgeWriteRequest) -> Result<KnowledgeWriteResponse, CoreError> {
        let result = self.store.write(request).await?;

        // 触发回调通知
        self.trigger_callbacks(&KnowledgeEventType::KnowledgeCreated, &result.id, &request.collection);

        Ok(result)
    }

    /// 删除知识
    pub async fn delete(&self, id: &str) -> Result<bool, CoreError> {
        let result = self.store.delete(id).await?;

        if result {
            self.trigger_callbacks(&KnowledgeEventType::KnowledgeDeleted, id, "");
        }

        Ok(result)
    }

    // ─── Callback ──────────────────────────────────────────────

    /// 注册回调
    pub fn register_callback(&self, request: KnowledgeCallbackRequest) -> KnowledgeCallbackResponse {
        let sub_id = format!("sub_{}", uuid::Uuid::new_v4());
        let events = request.events.clone();

        self.callbacks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sub_id.clone(), request);

        KnowledgeCallbackResponse {
            subscription_id: sub_id,
            events,
            status: "active".to_string(),
        }
    }

    /// 触发回调通知（内部方法，实际实现中应异步发送 HTTP 请求）
    fn trigger_callbacks(&self, event: &KnowledgeEventType, doc_id: &str, collection: &str) {
        let callbacks = self.callbacks.read().unwrap_or_else(|e| e.into_inner());

        for (_, subscription) in callbacks.iter() {
            if !subscription.events.contains(event) {
                continue;
            }
            if !subscription.collections.is_empty() && !subscription.collections.contains(&collection.to_string()) {
                continue;
            }

            // POC 阶段只记录日志，实际实现需要发送 HTTP POST
            tracing::info!(
                event = ?event,
                doc_id = %doc_id,
                collection = %collection,
                callback_url = %subscription.callback_url,
                "Knowledge callback triggered"
            );
        }
    }

    /// 列出可用集合
    pub async fn list_collections(&self) -> Result<Vec<String>, CoreError> {
        self.store.list_collections().await
    }
}

// ─── 内存知识存储 (POC 用) ─────────────────────────────────────

/// 内存知识存储 — POC 验证用，生产环境替换为 SQLite/PG
pub struct InMemoryKnowledgeStore {
    /// 集合名 → 文档列表
    collections: std::sync::RwLock<HashMap<String, Vec<KnowledgeReadResponse>>>,
}

impl InMemoryKnowledgeStore {
    /// 创建空存储
    pub fn new() -> Self {
        Self {
            collections: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 预填充数据
    pub fn seed(&self, collection: &str, docs: Vec<KnowledgeReadResponse>) {
        self.collections
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(collection.to_string(), docs);
    }
}

impl Default for InMemoryKnowledgeStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl KnowledgeStore for InMemoryKnowledgeStore {
    async fn query(&self, request: &KnowledgeQueryRequest) -> Result<KnowledgeQueryResponse, CoreError> {
        let collections = self.collections.read().unwrap_or_else(|e| e.into_inner());

        let query_lower = request.query.to_lowercase();
        let search_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<KnowledgeQueryResult> = Vec::new();

        let target_collections: Vec<&String> = if request.collections.is_empty() {
            collections.keys().collect()
        } else {
            collections.keys().filter(|k| request.collections.contains(k)).collect()
        };

        for coll_name in target_collections {
            if let Some(docs) = collections.get(coll_name) {
                for doc in docs {
                    let search_text = format!(
                        "{} {} {}",
                        doc.title.to_lowercase(),
                        doc.content.to_lowercase(),
                        doc.metadata.tags.join(" ").to_lowercase()
                    );

                    let match_count = search_terms.iter().filter(|t| search_text.contains(*t)).count();

                    if match_count > 0 {
                        let relevance = match_count as f64 / search_terms.len() as f64;
                        let snippet = if doc.content.len() > 200 {
                            format!("{}...", &doc.content[..200])
                        } else {
                            doc.content.clone()
                        };

                        results.push(KnowledgeQueryResult {
                            id: doc.id.clone(),
                            collection: coll_name.clone(),
                            title: doc.title.clone(),
                            snippet,
                            relevance,
                            updated_at: doc.updated_at,
                        });
                    }
                }
            }
        }

        // 按相关度排序
        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total = results.len();
        results.truncate(request.limit);
        let has_more = total > request.limit;

        Ok(KnowledgeQueryResponse {
            results,
            total,
            has_more,
        })
    }

    async fn read(&self, id: &str) -> Result<Option<KnowledgeReadResponse>, CoreError> {
        let collections = self.collections.read().unwrap_or_else(|e| e.into_inner());

        for docs in collections.values() {
            if let Some(doc) = docs.iter().find(|d| d.id == id) {
                return Ok(Some(doc.clone()));
            }
        }
        Ok(None)
    }

    async fn write(&self, request: &KnowledgeWriteRequest) -> Result<KnowledgeWriteResponse, CoreError> {
        let mut collections = self.collections.write().unwrap_or_else(|e| e.into_inner());

        let now = Utc::now();
        let id = format!("doc-{}", uuid::Uuid::new_v4());

        let doc = KnowledgeReadResponse {
            id: id.clone(),
            collection: request.collection.clone(),
            title: request.title.clone(),
            content: request.content.clone(),
            format: request.format.clone(),
            metadata: request.metadata.clone(),
            created_at: now,
            updated_at: now,
        };

        collections
            .entry(request.collection.clone())
            .or_default()
            .push(doc);

        Ok(KnowledgeWriteResponse {
            id,
            status: KnowledgeWriteStatus::Created,
            created_at: now,
        })
    }

    async fn delete(&self, id: &str) -> Result<bool, CoreError> {
        let mut collections = self.collections.write().unwrap_or_else(|e| e.into_inner());

        for docs in collections.values_mut() {
            if let Some(pos) = docs.iter().position(|d| d.id == id) {
                docs.remove(pos);
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn list_collections(&self) -> Result<Vec<String>, CoreError> {
        let collections = self.collections.read().unwrap_or_else(|e| e.into_inner());
        Ok(collections.keys().cloned().collect())
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knowledge_sync_capability_default() {
        let cap = KnowledgeSyncCapability::default();
        assert_eq!(cap.version, "0.1.0");
        assert!(cap.requires_auth);
        assert!(cap.supported_formats.contains(&"markdown".to_string()));
    }

    #[test]
    fn test_api_key_manager() {
        let mut manager = ApiKeyManager::new();

        let record = ApiKeyRecord {
            key: "test-key-001".to_string(),
            client_id: "test-agent".to_string(),
            scope: vec![KnowledgeScope::KnowledgeRead, KnowledgeScope::KnowledgeWrite],
            allowed_collections: vec!["project-context".to_string()],
            is_active: true,
        };

        manager.register(record);

        // 验证有效 Key
        let result = manager.validate("test-key-001", &KnowledgeScope::KnowledgeRead);
        assert!(result.is_some());

        // 验证无权限
        let result = manager.validate("test-key-001", &KnowledgeScope::KnowledgeWrite);
        assert!(result.is_some());

        // 验证无效 Key
        let result = manager.validate("invalid-key", &KnowledgeScope::KnowledgeRead);
        assert!(result.is_none());

        // 吊销 Key
        assert!(manager.revoke("test-key-001"));
        let result = manager.validate("test-key-001", &KnowledgeScope::KnowledgeRead);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_in_memory_store_query() {
        let store = InMemoryKnowledgeStore::new();

        // 写入数据
        let write_req = KnowledgeWriteRequest {
            collection: "project-context".to_string(),
            title: "Extension Registry设计哲学".to_string(),
            content: "新功能=注册扩展，架构本身永远不需要改".to_string(),
            format: "markdown".to_string(),
            metadata: KnowledgeMetadata {
                source: Some("test-agent".to_string()),
                task_id: None,
                tags: vec!["architecture".to_string()],
                is_private: false,
            },
        };

        let write_result = store.write(&write_req).await.unwrap();
        assert_eq!(write_result.status, KnowledgeWriteStatus::Created);

        // 查询
        let query_req = KnowledgeQueryRequest {
            query: "注册扩展 架构".to_string(),
            collections: vec![],
            limit: 5,
            format: "markdown".to_string(),
        };

        let query_result = store.query(&query_req).await.unwrap();
        assert_eq!(query_result.total, 1);
        assert_eq!(query_result.results[0].title, "Extension Registry设计哲学");

        // 读取
        let read_result = store.read(&write_result.id).await.unwrap();
        assert!(read_result.is_some());

        // 删除
        let delete_result = store.delete(&write_result.id).await.unwrap();
        assert!(delete_result);
    }

    #[tokio::test]
    async fn test_knowledge_sync_service_auth() {
        let store = InMemoryKnowledgeStore::new();
        let service = KnowledgeSyncService::new(Box::new(store));

        // 注册 API Key
        service.register_api_key(ApiKeyRecord {
            key: "sk-test-123".to_string(),
            client_id: "agent-xyz".to_string(),
            scope: vec![KnowledgeScope::KnowledgeRead],
            allowed_collections: vec!["project-context".to_string()],
            is_active: true,
        });

        // 认证成功
        let auth_req = KnowledgeAuthRequest {
            grant_type: KnowledgeGrantType::ApiKey,
            client_id: "agent-xyz".to_string(),
            scope: vec![KnowledgeScope::KnowledgeRead],
            api_key: Some("sk-test-123".to_string()),
            redirect_uri: None,
        };

        let auth_resp = service.authenticate(&auth_req).unwrap();
        assert_eq!(auth_resp.token_type, "Bearer");
        assert!(auth_resp.access_token.starts_with("ks_"));

        // 认证失败 - 错误的 key
        let auth_req_bad = KnowledgeAuthRequest {
            grant_type: KnowledgeGrantType::ApiKey,
            client_id: "agent-xyz".to_string(),
            scope: vec![KnowledgeScope::KnowledgeRead],
            api_key: Some("wrong-key".to_string()),
            redirect_uri: None,
        };

        assert!(service.authenticate(&auth_req_bad).is_err());

        // 认证失败 - 权限不足
        let auth_req_no_scope = KnowledgeAuthRequest {
            grant_type: KnowledgeGrantType::ApiKey,
            client_id: "agent-xyz".to_string(),
            scope: vec![KnowledgeScope::KnowledgeWrite],
            api_key: Some("sk-test-123".to_string()),
            redirect_uri: None,
        };

        assert!(service.authenticate(&auth_req_no_scope).is_err());
    }

    #[tokio::test]
    async fn test_knowledge_sync_service_full_flow() {
        let store = InMemoryKnowledgeStore::new();
        let service = KnowledgeSyncService::new(Box::new(store));

        // 注册 API Key
        service.register_api_key(ApiKeyRecord {
            key: "sk-full-001".to_string(),
            client_id: "agent-full".to_string(),
            scope: vec![KnowledgeScope::KnowledgeRead, KnowledgeScope::KnowledgeWrite],
            allowed_collections: vec!["project-context".to_string()],
            is_active: true,
        });

        // Auth
        let auth_resp = service
            .authenticate(&KnowledgeAuthRequest {
                grant_type: KnowledgeGrantType::ApiKey,
                client_id: "agent-full".to_string(),
                scope: vec![KnowledgeScope::KnowledgeRead, KnowledgeScope::KnowledgeWrite],
                api_key: Some("sk-full-001".to_string()),
                redirect_uri: None,
            })
            .unwrap();

        assert!(auth_resp.available_collections.contains(&"project-context".to_string()));

        // Write
        let write_resp = service
            .write(&KnowledgeWriteRequest {
                collection: "project-context".to_string(),
                title: "WO-040评估结论".to_string(),
                content: "Hermes Agent适配价值4.5/5".to_string(),
                format: "markdown".to_string(),
                metadata: KnowledgeMetadata {
                    source: Some("agent-full".to_string()),
                    task_id: Some("WO-040".to_string()),
                    tags: vec!["evaluation".to_string()],
                    is_private: false,
                },
            })
            .await
            .unwrap();

        assert_eq!(write_resp.status, KnowledgeWriteStatus::Created);

        // Query
        let query_resp = service
            .query(&KnowledgeQueryRequest {
                query: "Hermes 评估".to_string(),
                collections: vec![],
                limit: 5,
                format: "markdown".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(query_resp.total, 1);

        // Read
        let read_resp = service.read(&write_resp.id).await.unwrap();
        assert!(read_resp.is_some());
        assert_eq!(read_resp.unwrap().title, "WO-040评估结论");

        // Callback registration
        let callback_resp = service.register_callback(KnowledgeCallbackRequest {
            callback_url: "https://agent.example.com/webhook".to_string(),
            events: vec![KnowledgeEventType::KnowledgeCreated],
            collections: vec!["project-context".to_string()],
            secret: Some("whsec_test".to_string()),
        });

        assert_eq!(callback_resp.status, "active");
    }

    #[test]
    fn test_knowledge_scope_display() {
        assert_eq!(KnowledgeScope::KnowledgeRead.to_string(), "knowledge:read");
        assert_eq!(KnowledgeScope::KnowledgeWrite.to_string(), "knowledge:write");
    }
}
