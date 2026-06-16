//! ContextFilter - 条件化Extension暴露引擎
//!
//! 根据Context中的task_phase等字段动态过滤Extension列表
//! 实现按需装备，避免信息过载

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 任务阶段枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    /// 发现阶段 - Agent探索环境和可用资源
    Discovery,
    /// 规划阶段 - Agent制定执行计划
    Planning,
    /// 执行阶段 - Agent执行具体操作
    Execution,
    /// 验证阶段 - Agent验证执行结果
    Verification,
    /// 完成阶段 - Agent总结和收尾
    Completion,
    /// 异常恢复阶段 - Agent处理错误和恢复
    ErrorRecovery,
    /// 空闲阶段 - Agent等待指令
    #[default]
    Idle,
}

impl TaskPhase {
    /// 获取阶段优先级
    pub fn priority(&self) -> u8 {
        match self {
            TaskPhase::Idle => 0,
            TaskPhase::Discovery => 1,
            TaskPhase::Planning => 2,
            TaskPhase::Execution => 3,
            TaskPhase::Verification => 4,
            TaskPhase::Completion => 5,
            TaskPhase::ErrorRecovery => 6,
        }
    }

    /// 获取阶段描述
    pub fn description(&self) -> &'static str {
        match self {
            TaskPhase::Discovery => "探索环境和可用资源",
            TaskPhase::Planning => "制定执行计划",
            TaskPhase::Execution => "执行具体操作",
            TaskPhase::Verification => "验证执行结果",
            TaskPhase::Completion => "总结和收尾",
            TaskPhase::ErrorRecovery => "处理错误和恢复",
            TaskPhase::Idle => "等待新指令",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "discovery" => Some(TaskPhase::Discovery),
            "planning" => Some(TaskPhase::Planning),
            "execution" => Some(TaskPhase::Execution),
            "verification" => Some(TaskPhase::Verification),
            "completion" => Some(TaskPhase::Completion),
            "error_recovery" | "errorrecovery" => Some(TaskPhase::ErrorRecovery),
            "idle" => Some(TaskPhase::Idle),
            _ => None,
        }
    }

    /// 转为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskPhase::Discovery => "discovery",
            TaskPhase::Planning => "planning",
            TaskPhase::Execution => "execution",
            TaskPhase::Verification => "verification",
            TaskPhase::Completion => "completion",
            TaskPhase::ErrorRecovery => "error_recovery",
            TaskPhase::Idle => "idle",
        }
    }
}

/// Extension过滤条件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionFilter {
    /// 适用的任务阶段（空=所有阶段）
    pub task_phases: Vec<TaskPhase>,
    /// 需要的最小权限等级
    pub min_permission_level: Option<u8>,
    /// 需要的标签（Extension必须有这些标签之一）
    pub required_tags: Vec<String>,
    /// 排除的标签（Extension有这些标签则排除）
    pub excluded_tags: Vec<String>,
}

impl ExtensionFilter {
    pub fn new() -> Self {
        Self {
            task_phases: vec![],
            min_permission_level: None,
            required_tags: vec![],
            excluded_tags: vec![],
        }
    }

    /// 适用于指定阶段的过滤器
    pub fn for_phases(phases: Vec<TaskPhase>) -> Self {
        Self {
            task_phases: phases,
            ..Default::default()
        }
    }

    /// 检查Extension是否符合条件
    pub fn matches(&self, ext: &ExtensionFilterTarget) -> bool {
        // 1. 检查任务阶段
        if !self.task_phases.is_empty() && !self.task_phases.contains(&ext.task_phase) {
            // 如果Extension声明了特定阶段，则必须匹配
            if !ext.supported_phases.is_empty() && !ext.supported_phases.contains(&ext.task_phase) {
                return false;
            }
        }

        // 2. 检查标签
        if !self.required_tags.is_empty() {
            let has_required = self.required_tags.iter().any(|tag| ext.tags.contains(tag));
            if !has_required {
                return false;
            }
        }

        // 3. 检查排除标签
        if !self.excluded_tags.is_empty() {
            let has_excluded = self.excluded_tags.iter().any(|tag| ext.tags.contains(tag));
            if has_excluded {
                return false;
            }
        }

        true
    }
}

impl Default for ExtensionFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension过滤目标（简化版Extension信息）
#[derive(Debug, Clone)]
pub struct ExtensionFilterTarget {
    /// Extension ID
    pub id: String,
    /// 当前任务阶段
    pub task_phase: TaskPhase,
    /// 支持的任务阶段（空=所有阶段）
    pub supported_phases: Vec<TaskPhase>,
    /// 标签
    pub tags: HashSet<String>,
    /// 权限等级
    pub permission_level: u8,
}

/// ContextFilter引擎
pub struct ContextFilterEngine {
    default_filter: ExtensionFilter,
}

impl ContextFilterEngine {
    pub fn new() -> Self {
        Self {
            default_filter: ExtensionFilter::new(),
        }
    }

    /// 过滤Extension列表
    pub fn filter_extensions(
        &self,
        extensions: Vec<ExtensionFilterTarget>,
        filter: Option<&ExtensionFilter>,
    ) -> Vec<ExtensionFilterTarget> {
        let filter = filter.unwrap_or(&self.default_filter);
        extensions.into_iter().filter(|ext| filter.matches(ext)).collect()
    }

    /// 根据Context过滤
    pub fn filter_by_context(
        &self,
        extensions: Vec<ExtensionFilterTarget>,
        context: &FilterContext,
    ) -> Vec<ExtensionFilterTarget> {
        let filter = ExtensionFilter {
            task_phases: vec![context.task_phase],
            min_permission_level: None,
            required_tags: context.required_tags.clone(),
            excluded_tags: context.excluded_tags.clone(),
        };
        self.filter_extensions(extensions, Some(&filter))
    }
}

impl Default for ContextFilterEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 过滤上下文
#[derive(Debug, Clone)]
pub struct FilterContext {
    /// 当前任务阶段
    pub task_phase: TaskPhase,
    /// 需要的标签
    pub required_tags: Vec<String>,
    /// 排除的标签
    pub excluded_tags: Vec<String>,
}

impl Default for FilterContext {
    fn default() -> Self {
        Self {
            task_phase: TaskPhase::Idle,
            required_tags: vec![],
            excluded_tags: vec![],
        }
    }
}

impl FilterContext {
    pub fn new(task_phase: TaskPhase) -> Self {
        Self {
            task_phase,
            ..Default::default()
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.required_tags = tags;
        self
    }
}

/// 过滤结果统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterStats {
    /// 总Extension数
    pub total: usize,
    /// 过滤后数量
    pub filtered: usize,
    /// 过滤掉的数量
    pub removed: usize,
    /// 过滤耗时（纳秒）
    pub duration_ns: u64,
}

impl FilterStats {
    pub fn new(total: usize, filtered: usize, duration_ns: u64) -> Self {
        Self {
            total,
            filtered,
            removed: total.saturating_sub(filtered),
            duration_ns,
        }
    }

    /// 检查是否满足性能要求（<1ms）
    pub fn is_performance_ok(&self) -> bool {
        self.duration_ns < 1_000_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_extension(id: &str, phases: Vec<TaskPhase>, tags: Vec<&str>) -> ExtensionFilterTarget {
        ExtensionFilterTarget {
            id: id.to_string(),
            task_phase: TaskPhase::Execution,
            supported_phases: phases,
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            permission_level: 1,
        }
    }

    #[test]
    fn test_task_phase_default() {
        let phase = TaskPhase::default();
        assert_eq!(phase, TaskPhase::Idle);
    }

    #[test]
    fn test_task_phase_from_str() {
        assert_eq!(TaskPhase::from_str("discovery"), Some(TaskPhase::Discovery));
        assert_eq!(TaskPhase::from_str("Execution"), Some(TaskPhase::Execution));
        assert_eq!(TaskPhase::from_str("unknown"), None);
    }

    #[test]
    fn test_extension_filter_for_phases() {
        let filter = ExtensionFilter::for_phases(vec![TaskPhase::Planning, TaskPhase::Execution]);
        assert!(filter.task_phases.contains(&TaskPhase::Planning));
        assert!(filter.task_phases.contains(&TaskPhase::Execution));
    }

    #[test]
    fn test_context_filter_engine() {
        let engine = ContextFilterEngine::new();

        let extensions = vec![
            create_test_extension("ext1", vec![], vec!["search"]),
            create_test_extension("ext2", vec![TaskPhase::Planning], vec!["planning"]),
            create_test_extension("ext3", vec![TaskPhase::Execution], vec!["execute"]),
        ];

        let context = FilterContext::new(TaskPhase::Execution);
        let filtered = engine.filter_by_context(extensions, &context);

        // 所有Extension都应该被包含（因为task_phase为空表示所有阶段）
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_filter_with_required_tags() {
        let engine = ContextFilterEngine::new();

        let extensions = vec![
            create_test_extension("ext1", vec![], vec!["search", "read"]),
            create_test_extension("ext2", vec![], vec!["write"]),
            create_test_extension("ext3", vec![], vec!["search", "write"]),
        ];

        let context = FilterContext::new(TaskPhase::Execution).with_tags(vec!["search".to_string()]);

        let filtered = engine.filter_by_context(extensions, &context);

        // ext1 和 ext3 有 search 标签
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|e| e.id == "ext1"));
        assert!(filtered.iter().any(|e| e.id == "ext3"));
    }

    #[test]
    fn test_filter_stats() {
        let stats = FilterStats::new(10, 7, 500_000);
        assert_eq!(stats.total, 10);
        assert_eq!(stats.filtered, 7);
        assert_eq!(stats.removed, 3);
        assert!(stats.is_performance_ok()); // 500us < 1ms
    }
}
