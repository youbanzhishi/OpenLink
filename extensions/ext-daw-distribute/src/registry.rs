//! # PluginRegistry — 插件注册表
//!
//! 管理插件注册、查询、搜索，支持版本管理和依赖检查。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use parking_lot::RwLock;

use super::plugin::{PluginFormat, PluginType};

/// 语义化版本
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.trim_start_matches('v').split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    /// 检查此版本是否满足指定最低版本要求
    pub fn satisfies(&self, min_version: &SemVer) -> bool {
        self >= min_version
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// 插件依赖声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// 依赖的插件 ID
    pub plugin_id: String,
    /// 最低版本要求
    pub min_version: SemVer,
}

/// 插件注册信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistration {
    /// 插件 ID
    pub id: String,
    /// 插件名称
    pub name: String,
    /// 插件描述
    #[serde(default)]
    pub description: String,
    /// 插件类型
    pub plugin_type: PluginType,
    /// 插件格式
    pub format: PluginFormat,
    /// 版本
    pub version: SemVer,
    /// 作者
    #[serde(default)]
    pub author: String,
    /// 标签
    #[serde(default)]
    pub tags: Vec<String>,
    /// 下载 URL
    pub download_url: Option<String>,
    /// 兼容性信息（如 DAW 名称列表）
    #[serde(default)]
    pub compatibility: Vec<String>,
    /// 依赖声明
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    /// 注册时间
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

/// 插件搜索条件
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSearchQuery {
    /// 按类型过滤
    pub plugin_type: Option<PluginType>,
    /// 按格式过滤
    pub format: Option<PluginFormat>,
    /// 按标签过滤
    pub tags: Vec<String>,
    /// 按作者过滤
    pub author: Option<String>,
    /// 按兼容性过滤
    pub compatibility: Option<String>,
    /// 名称关键词搜索
    pub keyword: Option<String>,
}

/// 插件注册表
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, PluginRegistration>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
        }
    }

    /// 注册插件
    pub fn register(&self, plugin: PluginRegistration) -> Result<(), String> {
        let mut plugins = self.plugins.write();
        if plugins.contains_key(&plugin.id) {
            // Check if new version is higher
            if let Some(existing) = plugins.get(&plugin.id) {
                if plugin.version <= existing.version {
                    return Err(format!("Plugin {} already registered with version {} (>= {})",
                        plugin.id, existing.version, plugin.version));
                }
            }
        }
        plugins.insert(plugin.id.clone(), plugin);
        Ok(())
    }

    /// 查询插件
    pub fn get(&self, id: &str) -> Option<PluginRegistration> {
        self.plugins.read().get(id).cloned()
    }

    /// 搜索插件
    pub fn search(&self, query: &PluginSearchQuery) -> Vec<PluginRegistration> {
        let plugins = self.plugins.read();
        plugins.values()
            .filter(|p| {
                // Filter by type
                if let Some(ref pt) = query.plugin_type {
                    if p.plugin_type != *pt {
                        return false;
                    }
                }
                // Filter by format
                if let Some(ref fmt) = query.format {
                    if p.format != *fmt {
                        return false;
                    }
                }
                // Filter by author
                if let Some(ref author) = query.author {
                    if p.author != *author {
                        return false;
                    }
                }
                // Filter by compatibility
                if let Some(ref compat) = query.compatibility {
                    if !p.compatibility.contains(compat) {
                        return false;
                    }
                }
                // Filter by tags (match any)
                if !query.tags.is_empty() {
                    let has_tag = query.tags.iter().any(|t| p.tags.contains(t));
                    if !has_tag {
                        return false;
                    }
                }
                // Filter by keyword
                if let Some(ref keyword) = query.keyword {
                    let kw = keyword.to_lowercase();
                    if !p.name.to_lowercase().contains(&kw)
                        && !p.description.to_lowercase().contains(&kw)
                    {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect()
    }

    /// 检查插件依赖是否满足
    pub fn check_dependencies(&self, plugin_id: &str) -> Result<Vec<String>, String> {
        let plugins = self.plugins.read();
        let plugin = plugins.get(plugin_id)
            .ok_or_else(|| format!("Plugin {} not found", plugin_id))?;

        let mut missing = Vec::new();
        for dep in &plugin.dependencies {
            match plugins.get(&dep.plugin_id) {
                Some(dep_plugin) => {
                    if !dep_plugin.version.satisfies(&dep.min_version) {
                        missing.push(format!(
                            "{}: need >= {}, found {}",
                            dep.plugin_id, dep.min_version, dep_plugin.version
                        ));
                    }
                }
                None => {
                    missing.push(format!("{}: not installed (need >= {})", dep.plugin_id, dep.min_version));
                }
            }
        }
        Ok(missing)
    }

    /// 列出所有插件
    pub fn list_all(&self) -> Vec<PluginRegistration> {
        self.plugins.read().values().cloned().collect()
    }

    /// 取消注册插件
    pub fn unregister(&self, id: &str) -> Option<PluginRegistration> {
        self.plugins.write().remove(id)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plugin(id: &str, name: &str, version: &str) -> PluginRegistration {
        PluginRegistration {
            id: id.to_string(),
            name: name.to_string(),
            description: "Test plugin".to_string(),
            plugin_type: PluginType::Effect,
            format: PluginFormat::Vst3,
            version: SemVer::parse(version).unwrap(),
            author: "test".to_string(),
            tags: vec!["eq".to_string()],
            download_url: Some("https://example.com/plugin.vst3".to_string()),
            compatibility: vec!["OpenDAW".to_string()],
            dependencies: vec![],
            registered_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_semver_parse() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_semver_compare() {
        let v1 = SemVer::parse("1.0.0").unwrap();
        let v2 = SemVer::parse("2.0.0").unwrap();
        let v3 = SemVer::parse("1.1.0").unwrap();
        assert!(v2 > v1);
        assert!(v3 > v1);
        assert!(v1.satisfies(&v1));
        assert!(v2.satisfies(&v1));
        assert!(!v1.satisfies(&v2));
    }

    #[test]
    fn test_registry_register_get() {
        let registry = PluginRegistry::new();
        let plugin = make_plugin("eq-1", "EQ", "1.0.0");
        registry.register(plugin).unwrap();

        let found = registry.get("eq-1").unwrap();
        assert_eq!(found.name, "EQ");
    }

    #[test]
    fn test_registry_search_by_type() {
        let registry = PluginRegistry::new();
        let mut plugin = make_plugin("eq-1", "EQ", "1.0.0");
        plugin.plugin_type = PluginType::Effect;
        registry.register(plugin).unwrap();

        let mut synth = make_plugin("synth-1", "Synth", "1.0.0");
        synth.plugin_type = PluginType::Instrument;
        registry.register(synth).unwrap();

        let query = PluginSearchQuery {
            plugin_type: Some(PluginType::Instrument),
            ..Default::default()
        };
        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Synth");
    }

    #[test]
    fn test_registry_search_by_keyword() {
        let registry = PluginRegistry::new();
        registry.register(make_plugin("eq-1", "Parametric EQ", "1.0.0")).unwrap();
        registry.register(make_plugin("comp-1", "Compressor", "1.0.0")).unwrap();

        let query = PluginSearchQuery {
            keyword: Some("parametric".to_string()),
            ..Default::default()
        };
        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "eq-1");
    }

    #[test]
    fn test_registry_dependency_check() {
        let registry = PluginRegistry::new();
        let base = make_plugin("base-1", "Base", "1.0.0");
        registry.register(base).unwrap();

        let mut dependent = make_plugin("dep-1", "Dependent", "1.0.0");
        dependent.dependencies = vec![PluginDependency {
            plugin_id: "base-1".to_string(),
            min_version: SemVer::parse("0.5.0").unwrap(),
        }];
        registry.register(dependent).unwrap();

        let missing = registry.check_dependencies("dep-1").unwrap();
        assert!(missing.is_empty());
    }

    #[test]
    fn test_registry_dependency_missing() {
        let registry = PluginRegistry::new();
        let mut dependent = make_plugin("dep-2", "Dependent 2", "1.0.0");
        dependent.dependencies = vec![PluginDependency {
            plugin_id: "nonexistent".to_string(),
            min_version: SemVer::parse("1.0.0").unwrap(),
        }];
        registry.register(dependent).unwrap();

        let missing = registry.check_dependencies("dep-2").unwrap();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("not installed"));
    }

    #[test]
    fn test_registry_version_upgrade() {
        let registry = PluginRegistry::new();
        registry.register(make_plugin("eq-1", "EQ", "1.0.0")).unwrap();
        let v2 = make_plugin("eq-1", "EQ", "2.0.0");
        registry.register(v2).unwrap();

        let found = registry.get("eq-1").unwrap();
        assert_eq!(found.version, SemVer::parse("2.0.0").unwrap());
    }

    #[test]
    fn test_registry_version_downgrade_rejected() {
        let registry = PluginRegistry::new();
        registry.register(make_plugin("eq-1", "EQ", "2.0.0")).unwrap();
        let result = registry.register(make_plugin("eq-1", "EQ", "1.0.0"));
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_unregister() {
        let registry = PluginRegistry::new();
        registry.register(make_plugin("eq-1", "EQ", "1.0.0")).unwrap();
        let removed = registry.unregister("eq-1").unwrap();
        assert_eq!(removed.name, "EQ");
        assert!(registry.get("eq-1").is_none());
    }
}
