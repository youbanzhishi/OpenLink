//! AgentPermission 数据结构 - User+Agent双维度组合权限模型
//!
//! 权限 = User授权 × Agent角色限制
//! Agent只获得完成任务所需的最小权限

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 权限唯一标识
pub type PermissionId = String;

/// User标识
pub type UserId = String;

/// Agent标识
pub type AgentId = String;

/// Extension标识
pub type ExtensionId = String;

/// Agent类型 - 用于权限策略匹配
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// 主Agent（主人自己的Agent）
    Owner,
    /// 授权Guest Agent
    Guest,
    /// 系统内置Agent
    System,
    /// 第三方Agent
    ThirdParty,
}

/// 操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// 读取
    Read,
    /// 写入/修改
    Write,
    /// 删除
    Delete,
    /// 执行（调用Extension）
    Execute,
    /// 管理（创建/修改权限配置）
    Admin,
}

/// 权限状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    /// 生效中
    Active,
    /// 已暂停
    Suspended,
    /// 已撤销
    Revoked,
    /// 已过期
    Expired,
}

/// 资源访问限制
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLimits {
    /// 单文件最大大小（字节）
    pub max_file_size: u64,

    /// 允许的存储后端
    pub allowed_storage_backends: Vec<String>,

    /// 最大存储配额（字节），0 = 无配额
    pub max_storage_quota: u64,

    /// 每分钟最大请求数
    pub max_requests_per_minute: u32,

    /// 允许的IP范围（CIDR格式），空 = 不限制
    pub allowed_ip_ranges: Vec<String>,

    /// 允许的域名列表，空 = 不限制
    pub allowed_domains: Vec<String>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_file_size: 10 * 1024 * 1024, // 10MB
            allowed_storage_backends: vec![],
            max_storage_quota: 0,
            max_requests_per_minute: 60,
            allowed_ip_ranges: vec![],
            allowed_domains: vec![],
        }
    }
}

/// Agent权限配置 - User+Agent双维度组合权限
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermission {
    /// 权限唯一标识
    pub permission_id: PermissionId,

    /// 关联的User ID（权限来源）
    pub user_id: UserId,

    /// 关联的Agent ID（权限受体）
    pub agent_id: AgentId,

    /// Agent显示名称（用于审计）
    pub agent_name: String,

    /// Agent类型（用于权限策略匹配）
    pub agent_type: AgentType,

    /// 允许调用的Extension列表（白名单）
    /// 空列表 = 禁止所有Extension
    pub allowed_extensions: Vec<ExtensionId>,

    /// 允许的操作类型
    pub allowed_operations: Vec<Operation>,

    /// 资源访问限制
    pub resource_limits: ResourceLimits,

    /// 权限有效期起始
    pub valid_from: DateTime<Utc>,

    /// 权限有效期截止
    pub valid_until: DateTime<Utc>,

    /// 权限状态
    pub status: PermissionStatus,

    /// 创建者
    pub created_by: UserId,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 最后使用时间
    pub last_used_at: Option<DateTime<Utc>>,

    /// 描述
    pub description: Option<String>,
}

impl AgentPermission {
    /// 创建新的权限配置
    pub fn new(user_id: UserId, agent_id: AgentId, agent_name: String, agent_type: AgentType) -> Self {
        let now = Utc::now();
        Self {
            permission_id: Uuid::new_v4().to_string(),
            user_id,
            agent_id,
            agent_name,
            agent_type,
            allowed_extensions: vec![],
            allowed_operations: vec![Operation::Read],
            resource_limits: ResourceLimits::default(),
            valid_from: now,
            valid_until: now + chrono::Duration::days(30),
            status: PermissionStatus::Active,
            created_by: String::new(),
            created_at: now,
            last_used_at: None,
            description: None,
        }
    }

    /// 检查权限是否在有效期内
    pub fn is_valid(&self) -> bool {
        let now = Utc::now();
        self.status == PermissionStatus::Active && now >= self.valid_from && now <= self.valid_until
    }

    /// 检查Extension是否被允许
    pub fn is_extension_allowed(&self, ext_id: &ExtensionId) -> bool {
        self.allowed_extensions.contains(ext_id)
    }

    /// 检查操作是否被允许
    pub fn is_operation_allowed(&self, op: &Operation) -> bool {
        self.allowed_operations.contains(op)
    }

    /// 撤销权限
    pub fn revoke(&mut self) {
        self.status = PermissionStatus::Revoked;
    }

    /// 暂停权限
    pub fn suspend(&mut self) {
        self.status = PermissionStatus::Suspended;
    }

    /// 恢复权限
    pub fn activate(&mut self) {
        self.status = PermissionStatus::Active;
    }

    /// 标记使用
    pub fn mark_used(&mut self) {
        self.last_used_at = Some(Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_creation() {
        let perm = AgentPermission::new(
            "user-001".into(),
            "agent-001".into(),
            "TestAgent".into(),
            AgentType::Guest,
        );

        assert!(!perm.permission_id.is_empty());
        assert_eq!(perm.user_id, "user-001");
        assert_eq!(perm.agent_id, "agent-001");
        assert_eq!(perm.agent_name, "TestAgent");
        assert_eq!(perm.agent_type, AgentType::Guest);
        assert_eq!(perm.status, PermissionStatus::Active);
        assert!(perm.is_valid());
    }

    #[test]
    fn test_extension_allowlist() {
        let mut perm = AgentPermission::new(
            "user-001".into(),
            "agent-001".into(),
            "TestAgent".into(),
            AgentType::Guest,
        );

        // 默认空白名单
        assert!(!perm.is_extension_allowed(&"knowledge-search".into()));

        // 添加Extension
        perm.allowed_extensions.push("knowledge-search".into());
        assert!(perm.is_extension_allowed(&"knowledge-search".into()));
        assert!(!perm.is_extension_allowed(&"admin-panel".into()));
    }

    #[test]
    fn test_operation_check() {
        let mut perm = AgentPermission::new(
            "user-001".into(),
            "agent-001".into(),
            "TestAgent".into(),
            AgentType::Guest,
        );

        // 默认只有Read
        assert!(perm.is_operation_allowed(&Operation::Read));
        assert!(!perm.is_operation_allowed(&Operation::Write));
        assert!(!perm.is_operation_allowed(&Operation::Delete));

        // 添加Write
        perm.allowed_operations.push(Operation::Write);
        assert!(perm.is_operation_allowed(&Operation::Write));
    }

    #[test]
    fn test_permission_lifecycle() {
        let mut perm = AgentPermission::new(
            "user-001".into(),
            "agent-001".into(),
            "TestAgent".into(),
            AgentType::Guest,
        );

        assert!(perm.is_valid());

        // 暂停
        perm.suspend();
        assert_eq!(perm.status, PermissionStatus::Suspended);
        assert!(!perm.is_valid());

        // 恢复
        perm.activate();
        assert!(perm.is_valid());

        // 撤销
        perm.revoke();
        assert_eq!(perm.status, PermissionStatus::Revoked);
        assert!(!perm.is_valid());
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_file_size, 10 * 1024 * 1024);
        assert_eq!(limits.max_requests_per_minute, 60);
        assert!(limits.allowed_storage_backends.is_empty());
        assert!(limits.allowed_ip_ranges.is_empty());
    }

    #[test]
    fn test_mark_used() {
        let mut perm = AgentPermission::new(
            "user-001".into(),
            "agent-001".into(),
            "TestAgent".into(),
            AgentType::Guest,
        );

        assert!(perm.last_used_at.is_none());
        perm.mark_used();
        assert!(perm.last_used_at.is_some());
    }
}
