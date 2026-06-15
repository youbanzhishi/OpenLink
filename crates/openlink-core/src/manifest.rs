//! Capability Manifest - Extension能力声明规范
//!
//! Extension需要一个标准化的能力声明schema
//! 让Agent和路由引擎能自动发现Extension的能力、副作用和约束

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Capability Manifest - Extension能力声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityManifest {
    /// Extension名称
    pub name: String,
    /// 版本号
    pub version: String,
    /// 能力列表
    pub capabilities: Vec<Capability>,
    /// 副作用声明
    pub side_effects: Vec<SideEffect>,
    /// 约束条件
    pub constraints: Vec<Constraint>,
    /// 依赖关系
    pub dependencies: Vec<String>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

impl CapabilityManifest {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            capabilities: vec![],
            side_effects: vec![],
            constraints: vec![],
            dependencies: vec![],
            metadata: HashMap::new(),
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<Capability>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_side_effects(mut self, effects: Vec<SideEffect>) -> Self {
        self.side_effects = effects;
        self
    }

    pub fn with_constraints(mut self, constraints: Vec<Constraint>) -> Self {
        self.constraints = constraints;
        self
    }
}

/// 单个能力
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    /// 能力名称
    pub name: String,
    /// 能力描述
    pub description: String,
    /// 输入参数
    pub input_schema: serde_json::Value,
    /// 输出参数
    pub output_schema: serde_json::Value,
}

/// 副作用声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideEffect {
    /// 副作用类型
    pub effect_type: SideEffectType,
    /// 描述
    pub description: String,
    /// 严重程度
    pub severity: SideEffectSeverity,
}

/// 副作用类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectType {
    /// 修改状态
    ModifiesState,
    /// 外部调用
    ExternalCall,
    /// 消耗积分
    ConsumesCredits,
    /// 发送通知
    SendsNotification,
    /// 数据存储
    DataStorage,
}

/// 副作用严重程度
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectSeverity {
    /// 低 - 无影响或可忽略
    Low,
    /// 中 - 需要用户确认
    Medium,
    /// 高 - 风险操作需显式授权
    High,
    /// 严重 - 可能导致数据丢失或安全问题
    Critical,
}

/// 约束条件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    /// 约束类型
    pub constraint_type: ConstraintType,
    /// 描述
    pub description: String,
    /// 值
    pub value: serde_json::Value,
}

/// 约束类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    /// 最大文件大小
    MaxFileSize,
    /// 最大请求频率
    MaxRequestsPerMinute,
    /// 需要权限
    RequiresPermission,
    /// 限流
    RateLimit,
    /// 超时时间
    Timeout,
}

/// Manifest验证器
pub struct ManifestValidator;

impl ManifestValidator {
    /// 验证Manifest
    pub fn validate(manifest: &CapabilityManifest) -> Result<(), ManifestError> {
        if manifest.name.is_empty() {
            return Err(ManifestError::EmptyName);
        }
        if manifest.version.is_empty() {
            return Err(ManifestError::EmptyVersion);
        }
        if manifest.capabilities.is_empty() {
            return Err(ManifestError::NoCapabilities);
        }
        Ok(())
    }

    /// 搜索支持特定能力的Extension
    pub fn search_by_capability(
        manifests: &[CapabilityManifest],
        capability_name: &str,
    ) -> Vec<&CapabilityManifest> {
        manifests
            .iter()
            .filter(|m| {
                m.capabilities
                    .iter()
                    .any(|c| c.name == capability_name)
            })
            .collect()
    }
}

/// Manifest错误
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestError {
    EmptyName,
    EmptyVersion,
    NoCapabilities,
    InvalidSchema,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::EmptyName => write!(f, "Extension name cannot be empty"),
            ManifestError::EmptyVersion => write!(f, "Extension version cannot be empty"),
            ManifestError::NoCapabilities => write!(f, "Extension must declare at least one capability"),
            ManifestError::InvalidSchema => write!(f, "Invalid JSON schema"),
        }
    }
}

/// 示例Manifest
pub mod examples {
    use super::*;

    pub fn knowledge_search_manifest() -> CapabilityManifest {
        CapabilityManifest::new("knowledge-search", "1.0.0")
            .with_capabilities(vec![
                Capability {
                    name: "search".to_string(),
                    description: "Search the knowledge base".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"},
                            "limit": {"type": "integer", "default": 10}
                        }
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "results": {"type": "array"}
                        }
                    }),
                }
            ])
            .with_side_effects(vec![
                SideEffect {
                    effect_type: SideEffectType::DataStorage,
                    description: "Logs search queries for analytics",
                    severity: SideEffectSeverity::Low,
                }
            ])
            .with_constraints(vec![
                Constraint {
                    constraint_type: ConstraintType::MaxRequestsPerMinute,
                    description: "Rate limit",
                    value: serde_json::json!(60),
                }
            ])
    }

    pub fn file_transfer_manifest() -> CapabilityManifest {
        CapabilityManifest::new("file-transfer", "1.0.0")
            .with_capabilities(vec![
                Capability {
                    name: "upload".to_string(),
                    description: "Upload a file".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file": {"type": "string", "format": "binary"},
                            "path": {"type": "string"}
                        }
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file_id": {"type": "string"}
                        }
                    }),
                },
                Capability {
                    name: "download".to_string(),
                    description: "Download a file".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "file_id": {"type": "string"}
                        }
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "url": {"type": "string"}
                        }
                    }),
                }
            ])
            .with_side_effects(vec![
                SideEffect {
                    effect_type: SideEffectType::ModifiesState,
                    description: "Creates a file on the storage",
                    severity: SideEffectSeverity::Medium,
                },
                SideEffect {
                    effect_type: SideEffectType::ConsumesCredits,
                    description: "Bandwidth usage",
                    severity: SideEffectSeverity::Low,
                }
            ])
            .with_constraints(vec![
                Constraint {
                    constraint_type: ConstraintType::MaxFileSize,
                    description: "Maximum upload size",
                    value: serde_json::json!(100 * 1024 * 1024),
                }
            ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_creation() {
        let manifest = CapabilityManifest::new("test-ext", "1.0.0");
        assert_eq!(manifest.name, "test-ext");
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn test_manifest_validation() {
        let manifest = CapabilityManifest::new("test", "1.0.0");
        assert!(ManifestValidator::validate(&manifest).is_err()); // No capabilities
        
        let manifest = manifest
            .with_capabilities(vec![
                Capability {
                    name: "test".to_string(),
                    description: "Test capability".to_string(),
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                }
            ]);
        assert!(ManifestValidator::validate(&manifest).is_ok());
    }

    #[test]
    fn test_search_by_capability() {
        let manifests = vec![
            examples::knowledge_search_manifest(),
            examples::file_transfer_manifest(),
        ];
        
        let results = ManifestValidator::search_by_capability(&manifests, "upload");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "file-transfer");
    }

    #[test]
    fn test_example_manifests() {
        let km = examples::knowledge_search_manifest();
        let ft = examples::file_transfer_manifest();
        
        assert!(!km.capabilities.is_empty());
        assert!(!ft.capabilities.is_empty());
        assert!(!ft.side_effects.is_empty());
    }
}
