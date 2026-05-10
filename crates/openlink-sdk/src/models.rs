//! # SDK 数据模型
//!
//! 对应 OpenLink 核心原语，便于 SDK 使用。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Link 模型 ─────────────────────────────────────────────

/// 链接创建请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLinkRequest {
    /// 目标 URL 或数据
    pub target: String,
    /// 自定义短码（可选）
    pub code: Option<String>,
    /// 链接元数据
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// 是否启用
    #[serde(default = "default_true")]
    pub is_active: bool,
    /// 所有者（Agent/用户ID）
    #[serde(default)]
    pub owner: Option<String>,
}

fn default_true() -> bool {
    true
}

/// 链接响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkResponse {
    pub id: String,
    pub code: String,
    pub target: String,
    pub owner: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
    pub is_active: bool,
}

/// 链接查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LinkQuery {
    /// 所有者过滤
    pub owner: Option<String>,
    /// 是否启用
    pub is_active: Option<bool>,
    /// 限制数量
    pub limit: Option<usize>,
    /// 偏移量
    pub offset: Option<usize>,
}

// ─── Route 模型 ────────────────────────────────────────────

/// 路由规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRule {
    /// 条件类型
    pub condition_type: String,
    /// 条件参数
    #[serde(default)]
    pub params: serde_json::Value,
    /// 目标 Action
    pub target: ActionTarget,
    /// 优先级
    #[serde(default = "default_priority")]
    pub priority: i32,
}

/// Action 目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTarget {
    /// Action 类型
    pub action: String,
    /// Action 参数
    #[serde(default)]
    pub params: serde_json::Value,
}

/// 路由创建请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRouteRequest {
    pub link_id: String,
    pub rules: Vec<RouteRule>,
    pub default_action: ActionTarget,
}

/// 路由响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResponse {
    pub id: String,
    pub link_id: String,
    pub rules: Vec<RouteRule>,
    pub default_action: ActionTarget,
    pub version: i32,
}

// ─── 文件传输模型 ───────────────────────────────────────────

/// 文件上传请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadRequest {
    /// 文件名
    pub filename: String,
    /// 文件大小（字节）
    pub size: u64,
    /// MIME 类型
    pub content_type: String,
    /// 存储后端（可选，默认 "auto"）
    #[serde(default)]
    pub storage: Option<String>,
    /// 是否生成分享链接
    #[serde(default = "default_true")]
    pub generate_share_link: bool,
    /// 分享链接过期时间（秒）
    #[serde(default)]
    pub share_link_ttl_secs: Option<u64>,
}

/// 文件上传响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadResponse {
    /// 文件 ID
    pub file_id: String,
    /// 上传 URL（预签名 URL）
    pub upload_url: String,
    /// 文件访问 URL
    pub access_url: Option<String>,
    /// 分享短码
    pub share_code: Option<String>,
    /// 过期时间
    pub expires_at: Option<DateTime<Utc>>,
}

/// 文件下载响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDownloadResponse {
    /// 文件 ID
    pub file_id: String,
    /// 下载 URL（预签名 URL）
    pub download_url: String,
    /// 过期时间
    pub expires_at: DateTime<Utc>,
}

/// 文件分享响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileShareResponse {
    /// 文件 ID
    pub file_id: String,
    /// 分享短码
    pub share_code: String,
    /// 分享链接
    pub share_url: String,
    /// 过期时间
    pub expires_at: Option<DateTime<Utc>>,
}

/// 文件元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub file_id: String,
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    pub storage: String,
    pub owner: String,
    pub created_at: DateTime<Utc>,
    pub access_count: u64,
}

// ─── Agent 专用模型 ─────────────────────────────────────────

/// 批量解析请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResolveRequest {
    /// 要解析的短码列表
    pub codes: Vec<String>,
}

/// 批量解析响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResolveResponse {
    pub results: Vec<ResolveResult>,
}

/// 单个解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveResult {
    pub code: String,
    pub link_id: Option<String>,
    pub target: Option<String>,
    pub action: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub found: bool,
}

/// 发现请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverRequest {
    /// 发现类型
    pub discover_type: String,
    /// 过滤器
    #[serde(default)]
    pub filters: serde_json::Value,
    /// 限制数量
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

/// 发现响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverResponse {
    pub links: Vec<LinkResponse>,
    pub total: usize,
}

// ─── Batch 操作模型 ─────────────────────────────────────────

/// 批量创建请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateRequest {
    /// 链接创建请求列表
    pub links: Vec<CreateLinkRequest>,
}

/// 批量创建响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateResponse {
    /// 成功创建的链接
    pub results: Vec<LinkResponse>,
    /// 成功数量
    pub succeeded: usize,
    /// 失败数量
    pub failed: usize,
}

/// 批量删除请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeleteRequest {
    /// 要删除的短码列表
    pub codes: Vec<String>,
}

/// 单个删除结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeleteResult {
    /// 短码
    pub code: String,
    /// 是否删除成功
    pub deleted: bool,
}

/// 批量删除响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeleteResponse {
    /// 删除结果列表
    pub results: Vec<BatchDeleteResult>,
    /// 成功数量
    pub succeeded: usize,
    /// 失败数量
    pub failed: usize,
}

// ─── 辅助函数 ───────────────────────────────────────────────

fn default_priority() -> i32 {
    10
}
