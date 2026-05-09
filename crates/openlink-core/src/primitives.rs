//! # 核心原语定义
//!
//! OpenLink 的五个核心原语：Link / Route / Action / Context / Hook
//! 这些原语是架构的基石，永不需要新增。
//! 设计铁律：核心层零业务逻辑，路由引擎不知道"短链"是什么。

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 全局唯一标识
pub type LinkID = String;

/// 人类可读短码 (e.g., d.aw/abc)
pub type ShortCode = String;

/// 会话追踪标识
pub type SessionID = String;

// ─── Identity ───────────────────────────────────────────────

/// 访问者身份类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum IdentityType {
    Human,
    Agent,
    Service,
}

/// 访问者身份 — 谁在访问
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// 身份标识
    pub id: String,
    /// 身份类型：人类 / Agent / 服务
    #[serde(rename = "type")]
    pub identity_type: IdentityType,
    /// Agent 子类型（仅 Agent 类型有效）
    pub agent_type: Option<String>,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            id: "anonymous".to_string(),
            identity_type: IdentityType::Human,
            agent_type: None,
        }
    }
}

// ─── DeviceInfo ─────────────────────────────────────────────

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// 设备类型：mobile / desktop / server / iot
    pub device_type: Option<String>,
    /// 操作系统
    pub os: Option<String>,
    /// 浏览器 / 客户端
    pub browser: Option<String>,
    /// 带宽等级：low / medium / high
    pub bandwidth: Option<String>,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            device_type: None,
            os: None,
            browser: None,
            bandwidth: None,
        }
    }
}

// ─── GeoInfo ────────────────────────────────────────────────

/// 地理位置信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoInfo {
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

impl Default for GeoInfo {
    fn default() -> Self {
        Self {
            country: None,
            region: None,
            city: None,
            lat: None,
            lon: None,
        }
    }
}

// ─── Context ────────────────────────────────────────────────

/// 请求上下文 — 路由决策的输入，决定走哪条路
///
/// 这是整个架构的核心输入。路由引擎接收 Context，
/// 通过规则匹配，输出对应的 Action。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// 谁在访问
    pub identity: Identity,
    /// 设备信息
    pub device: DeviceInfo,
    /// 地理位置
    pub location: GeoInfo,
    /// 时间
    pub time: DateTime<Utc>,
    /// 访问意图（Agent 可显式声明）
    pub intent: serde_json::Value,
    /// 会话追踪
    pub session: SessionID,
    /// 扩展上下文（Extension 填充）
    pub custom: serde_json::Value,
}

impl Context {
    /// 从 HTTP 请求构建基础 Context
    /// Phase 1 只提取最基本的身份和设备信息
    pub fn from_request(
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Self {
        let device = DeviceInfo {
            device_type: user_agent.and_then(|ua| {
                if ua.contains("Mobile") || ua.contains("Android") || ua.contains("iPhone") {
                    Some("mobile".to_string())
                } else {
                    Some("desktop".to_string())
                }
            }),
            os: None,
            browser: None,
            bandwidth: None,
        };

        Self {
            identity: Identity {
                id: ip.unwrap_or("unknown").to_string(),
                identity_type: IdentityType::Human,
                agent_type: None,
            },
            device,
            location: GeoInfo::default(),
            time: Utc::now(),
            intent: serde_json::Value::Null,
            session: uuid::Uuid::new_v4().to_string(),
            custom: serde_json::Value::Null,
        }
    }
}

// ─── Link ───────────────────────────────────────────────────

/// 链接实体 — 可寻址、可识别的实体。短链是 Link 的最简形态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// 全局唯一标识
    pub id: LinkID,
    /// 人类可读短码
    pub code: ShortCode,
    /// 结构化元数据（链接即数据包）
    pub payload: serde_json::Value,
    /// 创建者
    pub owner: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 扩展元数据（不影响路由）
    pub metadata: serde_json::Value,
    /// 是否启用
    pub is_active: bool,
}

// ─── Action ─────────────────────────────────────────────────

/// 执行动作 — Link 被解析后"做什么"。不只是重定向，一切皆 Action。
///
/// 关键设计：Custom Action 通过扩展注册，核心永远不需要改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 302/301 重定向（传统短链）
    Redirect,
    /// 跨设备/跨环境文件传输
    FileTransfer,
    /// 触发外部 HTTP 回调
    Webhook,
    /// 执行多步编排
    Workflow,
    /// 请求改写后转发
    Transform,
    /// 委托给另一个 Link
    Delegate,
    /// 扩展注册的自定义 Action
    Custom(String),
}

impl Action {
    /// 获取 Action 的字符串标识，用于 Extension Registry 查找
    pub fn as_str(&self) -> &str {
        match self {
            Action::Redirect => "redirect",
            Action::FileTransfer => "file_transfer",
            Action::Webhook => "webhook",
            Action::Workflow => "workflow",
            Action::Transform => "transform",
            Action::Delegate => "delegate",
            Action::Custom(name) => name.as_str(),
        }
    }
}

// ─── Condition ──────────────────────────────────────────────

/// 条件 — 什么情况下走这条路
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// 条件类型标识（对应注册的 Condition Handler）
    #[serde(rename = "type")]
    pub condition_type: String,
    /// 条件参数
    pub params: serde_json::Value,
}

// ─── Target ─────────────────────────────────────────────────

/// 目标 — 去哪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    /// 执行什么 Action
    pub action: Action,
    /// Action 专用参数（如 Redirect 的 URL 和状态码）
    pub params: serde_json::Value,
}

// ─── Rule ───────────────────────────────────────────────────

/// 路由规则 — 有序规则列表中的一条
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// 条件
    pub condition: Condition,
    /// 命中后的目标
    pub target: Target,
    /// 优先级（数值越大越优先）
    pub priority: i32,
}

// ─── Route ──────────────────────────────────────────────────

/// 路由规则 — 从 Link 到 Action 的映射，支持条件分支
///
/// 传统短链重定向就是只有 default 的 Route。
/// 有条件路由时，按 rules 的 priority 依次匹配，命中即停。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    /// 路由 ID
    pub id: String,
    /// 所属 Link ID
    pub link_id: LinkID,
    /// 有序规则列表（命中即停）
    pub rules: Vec<Rule>,
    /// 兜底目标（传统短链重定向就是只有 default 的 Route）
    #[serde(rename = "default")]
    pub default_target: Target,
    /// 版本号
    pub version: i32,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

// ─── Hook ───────────────────────────────────────────────────

/// 钩子触发阶段
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    /// 路由前：改写 Context
    BeforeRoute,
    /// 路由后：记录日志、触发通知
    AfterRoute,
    /// 出错时：降级处理、告警
    OnError,
}

/// 钩子动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookAction {
    /// 钩子名称
    pub name: String,
    /// 钩子配置
    pub config: serde_json::Value,
}

/// 钩子 — 路由前后的拦截器，扩展系统的核心机制
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    /// 触发阶段
    pub phase: HookPhase,
    /// 钩子动作
    pub action: HookAction,
    /// 优先级
    pub priority: i32,
}

// ─── ActionResult ───────────────────────────────────────────

/// Action 执行结果 — 路由引擎的输出
///
/// API 层将 ActionResult 转换为对应的 HTTP 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionResult {
    /// 重定向结果
    Redirect {
        url: String,
        status_code: u16,
    },
    /// JSON 数据响应
    Json(serde_json::Value),
    /// 自定义响应
    Custom {
        content_type: String,
        body: String,
    },
}

// ─── Extension metadata (for storage) ──────────────────────

/// 扩展注册记录（存储层）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extension {
    pub id: String,
    /// 扩展类型：action / condition / hook / protocol
    pub ext_type: String,
    /// 扩展名称（唯一）
    pub name: String,
    /// 扩展配置
    pub config: serde_json::Value,
    /// 是否启用
    pub is_active: bool,
    /// 注册时间
    pub created_at: DateTime<Utc>,
}

// ─── AccessLog ──────────────────────────────────────────────

/// 访问日志 — 可观测内置：每次路由决策都有完整上下文记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLog {
    pub id: String,
    pub link_id: LinkID,
    /// 完整上下文快照
    pub context: serde_json::Value,
    /// 命中的规则
    pub matched_rule: Option<String>,
    /// 执行的动作
    pub action_taken: String,
    /// 响应时间（毫秒）
    pub response_time_ms: Option<i64>,
    /// 记录时间
    pub created_at: DateTime<Utc>,
}

/// 链接访问统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkStats {
    pub link_id: LinkID,
    pub code: ShortCode,
    /// 总访问次数
    pub total_accesses: i64,
    /// 唯一身份数
    pub unique_identities: i64,
    /// 最近 24h 访问次数
    pub accesses_24h: i64,
    /// 最近 7d 访问次数
    pub accesses_7d: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_as_str() {
        assert_eq!(Action::Redirect.as_str(), "redirect");
        assert_eq!(Action::Custom("my-action".to_string()).as_str(), "my-action");
    }

    #[test]
    fn test_context_from_request() {
        let ctx = Context::from_request(Some("Mozilla/5.0 Mobile"), Some("1.2.3.4"));
        assert_eq!(ctx.identity.id, "1.2.3.4");
        assert_eq!(ctx.identity.identity_type, IdentityType::Human);
        assert_eq!(ctx.device.device_type.as_deref(), Some("mobile"));
    }

    #[test]
    fn test_action_serde_roundtrip() {
        let action = Action::Redirect;
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, decoded);
    }

    #[test]
    fn test_custom_action_serde() {
        let action = Action::Custom("open-daw-project".to_string());
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, decoded);
    }
}
