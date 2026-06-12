//! # Extension Search — 三桥模式延迟加载扩展搜索
//!
//! 实现 Tool Search POC 的核心模块，解决 MCP "工具税"问题：
//! - Bridge 1: extension_search — 搜索 Extension 索引（BM25 + 子串回退）
//! - Bridge 2: extension_describe — 按需加载 Extension Schema
//! - Bridge 3: extension_execute — 执行 Extension

use crate::error::CoreError;
use crate::primitives::{Context, HookPhase, Target};
use crate::registry::{ActionHandler, ConditionHandler, ExtensionRegistry, HookHandler};
// use async_trait::async_trait; // unused
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ─── Extension Type ──────────────────────────────────────────

/// Extension 类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionType {
    Action,
    Condition,
    Hook,
    Protocol,
}

impl ExtensionType {
    /// 从字符串解析 ExtensionType
    pub fn try_from_str(s: &str) -> Result<Self, String> {
        match s {
            "action" => Ok(ExtensionType::Action),
            "condition" => Ok(ExtensionType::Condition),
            "hook" => Ok(ExtensionType::Hook),
            "protocol" => Ok(ExtensionType::Protocol),
            _ => Err(format!("Unknown extension type: {}", s)),
        }
    }
}

impl std::fmt::Display for ExtensionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionType::Action => write!(f, "action"),
            ExtensionType::Condition => write!(f, "condition"),
            ExtensionType::Hook => write!(f, "hook"),
            ExtensionType::Protocol => write!(f, "protocol"),
        }
    }
}

// ─── Extension Index ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionIndex {
    pub name: String,
    pub ext_type: ExtensionType,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub parameter_names: Vec<String>,
}

impl ExtensionIndex {
    pub fn build_search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name,
            self.description,
            self.tags.join(" "),
            self.parameter_names.join(" ")
        )
    }
}

// ─── Extension Schema ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSchema {
    pub name: String,
    pub ext_type: ExtensionType,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_phase: Option<HookPhase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_priority: Option<i32>,
    pub version: String,
}

// ─── Search Request / Response ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSearchRequest {
    pub query: String,
    #[serde(default)]
    pub ext_type: Option<ExtensionType>,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSearchResponse {
    pub matches: Vec<ExtensionIndex>,
    pub search_time_ms: i64,
    pub total: usize,
}

// ─── Execute Request / Response ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionExecuteRequest {
    pub name: String,
    pub ext_type: ExtensionType,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionExecuteResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ─── BM25 搜索器 ──────────────────────────────────────────────

pub struct Bm25Searcher {
    documents: HashMap<String, String>,
    metadata: HashMap<String, ExtensionIndex>,
    doc_count: usize,
}

const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;

impl Bm25Searcher {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            metadata: HashMap::new(),
            doc_count: 0,
        }
    }

    pub fn index(&mut self, ext_index: ExtensionIndex, search_text: String) {
        let name = ext_index.name.clone();
        if self.documents.contains_key(&name) {
            self.documents.remove(&name);
            self.metadata.remove(&name);
        } else {
            self.doc_count += 1;
        }
        self.documents.insert(name.clone(), search_text);
        self.metadata.insert(name, ext_index);
    }

    pub fn remove(&mut self, name: &str) -> bool {
        if self.documents.remove(name).is_some() {
            self.metadata.remove(name);
            self.doc_count = self.doc_count.saturating_sub(1);
            true
        } else {
            false
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<ExtensionIndex> {
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().filter(|t| !t.is_empty()).collect();
        if query_terms.is_empty() {
            return Vec::new();
        }

        let avg_dl = self.average_document_length();

        let mut scored: Vec<(String, f64)> = self
            .documents
            .iter()
            .map(|(name, doc)| (name.clone(), self.bm25_score(&query_terms, doc, avg_dl)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut results: Vec<ExtensionIndex> = scored
            .into_iter()
            .take(limit)
            .filter_map(|(name, score)| {
                if score > 0.0 {
                    self.metadata.get(&name).cloned()
                } else {
                    self.fallback_substring_match(&name, &query_lower)
                }
            })
            .collect();

        if results.is_empty() {
            results = self.global_substring_search(&query_terms, limit);
        }
        results
    }

    fn bm25_score(&self, query_terms: &[&str], document: &str, avg_dl: f64) -> f64 {
        let doc_lower = document.to_lowercase();
        let dl = doc_lower.split_whitespace().count() as f64;
        let mut score = 0.0;
        for term in query_terms {
            let tf = doc_lower.matches(term).count() as f64;
            if tf == 0.0 {
                continue;
            }
            let df = self
                .documents
                .values()
                .filter(|d| d.to_lowercase().contains(term))
                .count() as f64;
            let idf = ((self.doc_count as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();
            let tf_part = (tf * (BM25_K1 + 1.0)) / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avg_dl.max(1.0)));
            score += idf * tf_part;
        }
        score
    }

    fn fallback_substring_match(&self, name: &str, query_lower: &str) -> Option<ExtensionIndex> {
        if name.to_lowercase().contains(query_lower) {
            self.metadata.get(name).cloned()
        } else {
            None
        }
    }

    fn global_substring_search(&self, query_terms: &[&str], limit: usize) -> Vec<ExtensionIndex> {
        let mut results: Vec<(String, usize)> = self
            .documents
            .iter()
            .map(|(name, doc)| {
                let dl = doc.to_lowercase();
                let mc = query_terms.iter().filter(|t| dl.contains(*t)).count();
                (name.clone(), mc)
            })
            .filter(|(_, c)| *c > 0)
            .collect();
        results.sort_by_key(|a| std::cmp::Reverse(a.1));
        results
            .into_iter()
            .take(limit)
            .filter_map(|(name, _)| self.metadata.get(&name).cloned())
            .collect()
    }

    fn average_document_length(&self) -> f64 {
        if self.documents.is_empty() {
            return 1.0;
        }
        let total: usize = self.documents.values().map(|d| d.split_whitespace().count()).sum();
        total as f64 / self.documents.len() as f64
    }

    pub fn len(&self) -> usize {
        self.metadata.len()
    }
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }
    pub fn all_indices(&self) -> Vec<ExtensionIndex> {
        self.metadata.values().cloned().collect()
    }
}

impl Default for Bm25Searcher {
    fn default() -> Self {
        Self::new()
    }
}

// ─── LazyExtensionRegistry ────────────────────────────────────

pub struct LazyExtensionRegistry {
    inner: RwLock<ExtensionRegistry>,
    searcher: RwLock<Bm25Searcher>,
    schema_cache: RwLock<HashMap<String, ExtensionSchema>>,
}

impl LazyExtensionRegistry {
    pub fn new(inner: ExtensionRegistry) -> Self {
        Self {
            inner: RwLock::new(inner),
            searcher: RwLock::new(Bm25Searcher::new()),
            schema_cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn new_empty() -> Self {
        Self::new(ExtensionRegistry::new())
    }

    pub fn register_with_index(
        &self,
        ext_type: ExtensionType,
        name: String,
        description: String,
        tags: Vec<String>,
        input_schema: serde_json::Value,
    ) -> Result<(), CoreError> {
        let param_names = extract_param_names(&input_schema);
        let index = ExtensionIndex {
            name: name.clone(),
            ext_type: ext_type.clone(),
            description: description.clone(),
            tags,
            parameter_names: param_names,
        };
        let schema = ExtensionSchema {
            name: name.clone(),
            ext_type,
            description,
            input_schema: input_schema.clone(),
            output_schema: serde_json::json!({ "type": "object" }),
            config: serde_json::json!({}),
            hook_phase: None,
            hook_priority: None,
            version: "1.0.0".to_string(),
        };
        let search_text = index.build_search_text();
        self.searcher
            .write()
            .map_err(|e| CoreError::InternalError(format!("Lock: {}", e)))?
            .index(index, search_text);
        self.schema_cache
            .write()
            .map_err(|e| CoreError::InternalError(format!("Lock: {}", e)))?
            .insert(name, schema);
        Ok(())
    }

    pub fn register_action_with_index(
        &self,
        handler: Arc<dyn ActionHandler>,
        description: String,
        tags: Vec<String>,
        input_schema: serde_json::Value,
    ) -> Result<(), CoreError> {
        let name = handler.name().to_string();
        self.inner
            .write()
            .map_err(|e| CoreError::InternalError(format!("Lock: {}", e)))?
            .register_action(handler)?;
        self.register_with_index(ExtensionType::Action, name, description, tags, input_schema)
    }

    pub fn register_condition_with_index(
        &self,
        handler: Arc<dyn ConditionHandler>,
        description: String,
        tags: Vec<String>,
        input_schema: serde_json::Value,
    ) -> Result<(), CoreError> {
        let name = handler.name().to_string();
        self.inner
            .write()
            .map_err(|e| CoreError::InternalError(format!("Lock: {}", e)))?
            .register_condition(handler)?;
        self.register_with_index(ExtensionType::Condition, name, description, tags, input_schema)
    }

    pub fn register_hook_with_index(
        &self,
        handler: Arc<dyn HookHandler>,
        description: String,
        tags: Vec<String>,
        input_schema: serde_json::Value,
    ) -> Result<(), CoreError> {
        let name = handler.name().to_string();
        let phase = handler.phase();
        let priority = handler.priority();
        self.inner
            .write()
            .map_err(|e| CoreError::InternalError(format!("Lock: {}", e)))?
            .register_hook(handler)?;
        let param_names = extract_param_names(&input_schema);
        let index = ExtensionIndex {
            name: name.clone(),
            ext_type: ExtensionType::Hook,
            description: description.clone(),
            tags,
            parameter_names: param_names,
        };
        let search_text = index.build_search_text();
        self.searcher
            .write()
            .map_err(|e| CoreError::InternalError(format!("Lock: {}", e)))?
            .index(index, search_text);
        let schema = ExtensionSchema {
            name,
            ext_type: ExtensionType::Hook,
            description,
            input_schema,
            output_schema: serde_json::json!({ "type": "object" }),
            config: serde_json::json!({}),
            hook_phase: Some(phase),
            hook_priority: Some(priority),
            version: "1.0.0".to_string(),
        };
        self.schema_cache
            .write()
            .map_err(|e| CoreError::InternalError(format!("Lock: {}", e)))?
            .insert(schema.name.clone(), schema);
        Ok(())
    }

    /// Bridge 1: search
    pub async fn search(&self, query: &str, ext_type: Option<ExtensionType>, limit: usize) -> ExtensionSearchResponse {
        let start = std::time::Instant::now();
        let searcher = self.searcher.read().unwrap_or_else(|e| e.into_inner());
        let mut results = searcher.search(query, limit * 2);
        if let Some(ref t) = ext_type {
            results.retain(|idx| idx.ext_type == *t);
        }
        results.truncate(limit);
        let total = results.len();
        ExtensionSearchResponse {
            matches: results,
            search_time_ms: start.elapsed().as_millis() as i64,
            total,
        }
    }

    /// Bridge 2: describe
    pub fn describe(&self, name: &str) -> Option<ExtensionSchema> {
        self.schema_cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }

    /// Bridge 3: execute
    #[allow(clippy::await_holding_lock)]
    pub async fn execute(
        &self,
        name: &str,
        ext_type: &ExtensionType,
        arguments: serde_json::Value,
        ctx: &Context,
    ) -> Result<ExtensionExecuteResponse, CoreError> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        match ext_type {
            ExtensionType::Action => {
                let target = Target {
                    action: crate::primitives::Action::Custom(name.to_string()),
                    params: arguments,
                };
                let handler = inner
                    .get_action_handler(name)
                    .ok_or_else(|| CoreError::ExtensionError(format!("Action '{}' not found", name)))?;
                let result = handler.execute(ctx, &target).await?;
                Ok(ExtensionExecuteResponse {
                    ok: true,
                    result: Some(serde_json::to_value(result).unwrap_or_default()),
                    error: None,
                })
            }
            ExtensionType::Condition => {
                let handler = inner
                    .get_condition_handler(name)
                    .ok_or_else(|| CoreError::ExtensionError(format!("Condition '{}' not found", name)))?;
                let result = handler.evaluate(ctx, &arguments).await?;
                Ok(ExtensionExecuteResponse {
                    ok: true,
                    result: Some(serde_json::json!({ "matched": result })),
                    error: None,
                })
            }
            ExtensionType::Hook | ExtensionType::Protocol => Err(CoreError::ExtensionError(format!(
                "Direct execute not supported for {:?}",
                ext_type
            ))),
        }
    }

    pub fn inner(&self) -> std::sync::RwLockReadGuard<'_, ExtensionRegistry> {
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn list_all(&self) -> Vec<ExtensionIndex> {
        self.searcher.read().unwrap_or_else(|e| e.into_inner()).all_indices()
    }

    pub fn index_count(&self) -> usize {
        self.searcher.read().unwrap_or_else(|e| e.into_inner()).len()
    }
}

fn extract_param_names(schema: &serde_json::Value) -> Vec<String> {
    schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_index(name: &str, ext_type: ExtensionType, desc: &str, tags: Vec<&str>) -> ExtensionIndex {
        ExtensionIndex {
            name: name.to_string(),
            ext_type,
            description: desc.to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            parameter_names: vec![],
        }
    }

    #[test]
    fn test_bm25_search_basic() {
        let mut searcher = Bm25Searcher::new();
        searcher.index(
            make_index("redirect", ExtensionType::Action, "HTTP 302/301 redirect", vec!["http"]),
            "redirect HTTP 302 301 redirect http".into(),
        );
        searcher.index(
            make_index(
                "webhook",
                ExtensionType::Action,
                "Trigger external HTTP callback",
                vec!["http"],
            ),
            "webhook Trigger external HTTP callback http".into(),
        );
        let results = searcher.search("redirect", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "redirect");
    }

    #[test]
    fn test_bm25_empty_query() {
        let mut s = Bm25Searcher::new();
        s.index(make_index("test", ExtensionType::Action, "test", vec![]), "test".into());
        assert!(s.search("", 5).is_empty());
    }

    #[test]
    fn test_bm25_remove() {
        let mut s = Bm25Searcher::new();
        s.index(make_index("test", ExtensionType::Action, "test", vec![]), "test".into());
        assert_eq!(s.len(), 1);
        assert!(s.remove("test"));
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn test_extension_type_try_from_str() {
        assert_eq!(ExtensionType::try_from_str("action").unwrap(), ExtensionType::Action);
        assert_eq!(ExtensionType::try_from_str("hook").unwrap(), ExtensionType::Hook);
        assert!(ExtensionType::try_from_str("invalid").is_err());
    }

    #[tokio::test]
    async fn test_lazy_registry_register_and_search() {
        let registry = LazyExtensionRegistry::new_empty();
        registry
            .register_with_index(
                ExtensionType::Action,
                "redirect".into(),
                "HTTP redirect".into(),
                vec!["http".into()],
                serde_json::json!({"properties":{"url":{"type":"string"}}}),
            )
            .unwrap();
        registry
            .register_with_index(
                ExtensionType::Condition,
                "identity-type".into(),
                "Check identity type".into(),
                vec!["identity".into()],
                serde_json::json!({}),
            )
            .unwrap();

        let resp = registry.search("redirect", None, 5).await;
        assert_eq!(resp.matches.len(), 1);
        assert_eq!(resp.matches[0].name, "redirect");

        let schema = registry.describe("redirect").unwrap();
        assert_eq!(schema.name, "redirect");

        assert!(registry.describe("nonexistent").is_none());
    }
}
