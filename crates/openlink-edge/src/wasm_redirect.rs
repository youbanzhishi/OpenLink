//! # WASM 边缘重定向（Phase 5）
//!
//! 轻量级重定向决策引擎，无需查数据库。
//! 设计为可在 WASM 环境中运行（无 std::net、无数据库、纯计算）。
//!
//! ## 功能
//! - 纯函数式重定向决策
//! - 基于规则的快速匹配（无需网络 IO）
//! - 条件路由支持（Identity/Device/Geo 预匹配）
//! - 热链快速路径

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 边缘重定向规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRedirectRule {
    /// 规则 ID
    pub id: String,
    /// 短码
    pub code: String,
    /// 匹配条件（可选，无条件则直接重定向）
    #[serde(default)]
    pub condition: Option<EdgeCondition>,
    /// 目标 URL
    pub target_url: String,
    /// 状态码
    #[serde(default = "default_status_code")]
    pub status_code: u16,
    /// 优先级（数值越大越优先）
    #[serde(default)]
    pub priority: i32,
}

fn default_status_code() -> u16 {
    302
}

/// 边缘条件（简化版，无数据库查询）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeCondition {
    /// 条件类型
    #[serde(rename = "type")]
    pub condition_type: EdgeConditionType,
    /// 条件参数
    #[serde(default)]
    pub params: HashMap<String, String>,
}

/// 边缘条件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeConditionType {
    /// 始终匹配
    Always,
    /// 身份类型匹配：human / agent / service
    IdentityType,
    /// 设备类型匹配：mobile / desktop / server
    DeviceType,
    /// 地理区域匹配
    GeoRegion,
    /// 自定义 Header 匹配
    HeaderMatch,
}

/// 请求信息（边缘版，轻量级）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRequest {
    /// 短码
    pub code: String,
    /// 客户端 IP
    pub client_ip: Option<String>,
    /// User-Agent
    pub user_agent: Option<String>,
    /// 设备类型（预检测）
    pub device_type: Option<String>,
    /// 身份类型（预检测）
    pub identity_type: Option<String>,
    /// 地理区域（预检测）
    pub geo_region: Option<String>,
    /// 自定义 Headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// 重定向决策结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RedirectDecision {
    /// 目标 URL
    pub target_url: String,
    /// 状态码
    pub status_code: u16,
    /// 匹配的规则 ID
    pub matched_rule_id: Option<String>,
    /// 是否为缓存命中（热链快速路径）
    pub cache_hit: bool,
}

/// 边缘重定向引擎
///
/// 纯计算引擎，无 IO，无数据库，适合 WASM 运行。
pub struct EdgeRedirectEngine {
    /// 按短码索引的规则（code → rules）
    rules_by_code: HashMap<String, Vec<EdgeRedirectRule>>,
    /// 热链快速路径（code → target_url）
    hot_links: HashMap<String, String>,
}

impl EdgeRedirectEngine {
    /// 创建空引擎
    pub fn new() -> Self {
        Self {
            rules_by_code: HashMap::new(),
            hot_links: HashMap::new(),
        }
    }

    /// 从规则列表构建引擎
    pub fn from_rules(rules: Vec<EdgeRedirectRule>) -> Self {
        let mut rules_by_code: HashMap<String, Vec<EdgeRedirectRule>> = HashMap::new();
        for rule in rules {
            rules_by_code
                .entry(rule.code.clone())
                .or_default()
                .push(rule);
        }

        // 对每个 code 的规则按优先级排序（降序）
        for rules in rules_by_code.values_mut() {
            rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        }

        Self {
            rules_by_code,
            hot_links: HashMap::new(),
        }
    }

    /// 注册热链（快速路径，跳过条件评估）
    pub fn register_hot_link(&mut self, code: String, target_url: String) {
        tracing::debug!(code = %code, "Registered hot link fast path");
        self.hot_links.insert(code, target_url);
    }

    /// 批量注册热链
    pub fn register_hot_links(&mut self, links: Vec<(String, String)>) {
        for (code, url) in links {
            self.hot_links.insert(code, url);
        }
        tracing::info!(count = self.hot_links.len(), "Hot links registered");
    }

    /// 执行重定向决策
    ///
    /// 决策流程：
    /// 1. 检查热链快速路径
    /// 2. 查找短码对应的规则列表
    /// 3. 按优先级评估条件，命中即返回
    /// 4. 无匹配则返回 None
    pub fn resolve(&self, request: &EdgeRequest) -> Option<RedirectDecision> {
        // 1. 热链快速路径
        if let Some(target_url) = self.hot_links.get(&request.code) {
            return Some(RedirectDecision {
                target_url: target_url.clone(),
                status_code: 302,
                matched_rule_id: None,
                cache_hit: true,
            });
        }

        // 2. 查找规则
        let rules = self.rules_by_code.get(&request.code)?;

        // 3. 按优先级评估
        for rule in rules {
            if self.evaluate_condition(&rule.condition, request) {
                return Some(RedirectDecision {
                    target_url: rule.target_url.clone(),
                    status_code: rule.status_code,
                    matched_rule_id: Some(rule.id.clone()),
                    cache_hit: false,
                });
            }
        }

        None
    }

    /// 评估条件
    fn evaluate_condition(
        &self,
        condition: &Option<EdgeCondition>,
        request: &EdgeRequest,
    ) -> bool {
        match condition {
            None => true, // 无条件 = 始终匹配
            Some(cond) => match cond.condition_type {
                EdgeConditionType::Always => true,
                EdgeConditionType::IdentityType => {
                    let target = cond.params.get("type").map(|s| s.as_str()).unwrap_or("");
                    request
                        .identity_type
                        .as_deref()
                        .map(|t| t.eq_ignore_ascii_case(target))
                        .unwrap_or(false)
                }
                EdgeConditionType::DeviceType => {
                    let target = cond.params.get("type").map(|s| s.as_str()).unwrap_or("");
                    request
                        .device_type
                        .as_deref()
                        .map(|t| t.eq_ignore_ascii_case(target))
                        .unwrap_or(false)
                }
                EdgeConditionType::GeoRegion => {
                    let target = cond.params.get("region").map(|s| s.as_str()).unwrap_or("");
                    request
                        .geo_region
                        .as_deref()
                        .map(|t| t.eq_ignore_ascii_case(target))
                        .unwrap_or(false)
                }
                EdgeConditionType::HeaderMatch => {
                    let header_name = cond.params.get("header").map(|s| s.as_str()).unwrap_or("");
                    let pattern = cond.params.get("pattern").map(|s| s.as_str()).unwrap_or("");
                    if header_name.is_empty() || pattern.is_empty() {
                        return false;
                    }
                    request
                        .headers
                        .iter()
                        .any(|(k, v)| {
                            k.eq_ignore_ascii_case(header_name)
                                && v.to_lowercase().contains(&pattern.to_lowercase())
                        })
                }
            },
        }
    }

    /// 添加规则
    pub fn add_rule(&mut self, rule: EdgeRedirectRule) {
        let rules = self.rules_by_code.entry(rule.code.clone()).or_default();
        rules.push(rule);
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 移除规则
    pub fn remove_rule(&mut self, code: &str, rule_id: &str) -> bool {
        if let Some(rules) = self.rules_by_code.get_mut(code) {
            let before = rules.len();
            rules.retain(|r| r.id != rule_id);
            rules.len() < before
        } else {
            false
        }
    }

    /// 获取规则数量
    pub fn rule_count(&self) -> usize {
        self.rules_by_code.values().map(|r| r.len()).sum()
    }

    /// 获取热链数量
    pub fn hot_link_count(&self) -> usize {
        self.hot_links.len()
    }

    /// 导出所有规则（用于持久化/同步）
    pub fn export_rules(&self) -> Vec<EdgeRedirectRule> {
        self.rules_by_code
            .values()
            .flat_map(|rules| rules.iter().cloned())
            .collect()
    }
}

impl Default for EdgeRedirectEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(code: &str) -> EdgeRequest {
        EdgeRequest {
            code: code.to_string(),
            client_ip: None,
            user_agent: None,
            device_type: None,
            identity_type: None,
            geo_region: None,
            headers: HashMap::new(),
        }
    }

    #[test]
    fn test_simple_redirect() {
        let rules = vec![EdgeRedirectRule {
            id: "r1".to_string(),
            code: "abc".to_string(),
            condition: None,
            target_url: "https://example.com".to_string(),
            status_code: 302,
            priority: 0,
        }];
        let engine = EdgeRedirectEngine::from_rules(rules);

        let decision = engine.resolve(&make_request("abc")).unwrap();
        assert_eq!(decision.target_url, "https://example.com");
        assert_eq!(decision.status_code, 302);
        assert!(!decision.cache_hit);
    }

    #[test]
    fn test_hot_link_fast_path() {
        let mut engine = EdgeRedirectEngine::new();
        engine.register_hot_link("hot1".to_string(), "https://hot.example.com".to_string());

        let decision = engine.resolve(&make_request("hot1")).unwrap();
        assert_eq!(decision.target_url, "https://hot.example.com");
        assert!(decision.cache_hit);
    }

    #[test]
    fn test_condition_identity_type() {
        let rules = vec![EdgeRedirectRule {
            id: "r1".to_string(),
            code: "api".to_string(),
            condition: Some(EdgeCondition {
                condition_type: EdgeConditionType::IdentityType,
                params: {
                    let mut m = HashMap::new();
                    m.insert("type".to_string(), "agent".to_string());
                    m
                },
            }),
            target_url: "https://api.example.com".to_string(),
            status_code: 302,
            priority: 10,
        }];
        let engine = EdgeRedirectEngine::from_rules(rules);

        // Agent 请求应匹配
        let mut req = make_request("api");
        req.identity_type = Some("agent".to_string());
        let decision = engine.resolve(&req).unwrap();
        assert_eq!(decision.target_url, "https://api.example.com");

        // Human 请求不应匹配
        let mut req = make_request("api");
        req.identity_type = Some("human".to_string());
        assert!(engine.resolve(&req).is_none());
    }

    #[test]
    fn test_condition_device_type() {
        let rules = vec![EdgeRedirectRule {
            id: "r1".to_string(),
            code: "m".to_string(),
            condition: Some(EdgeCondition {
                condition_type: EdgeConditionType::DeviceType,
                params: {
                    let mut m = HashMap::new();
                    m.insert("type".to_string(), "mobile".to_string());
                    m
                },
            }),
            target_url: "https://m.example.com".to_string(),
            status_code: 302,
            priority: 10,
        }];
        let engine = EdgeRedirectEngine::from_rules(rules);

        let mut req = make_request("m");
        req.device_type = Some("mobile".to_string());
        let decision = engine.resolve(&req).unwrap();
        assert_eq!(decision.target_url, "https://m.example.com");
    }

    #[test]
    fn test_priority_ordering() {
        let rules = vec![
            EdgeRedirectRule {
                id: "low".to_string(),
                code: "x".to_string(),
                condition: None,
                target_url: "https://low.example.com".to_string(),
                status_code: 302,
                priority: 1,
            },
            EdgeRedirectRule {
                id: "high".to_string(),
                code: "x".to_string(),
                condition: None,
                target_url: "https://high.example.com".to_string(),
                status_code: 302,
                priority: 10,
            },
        ];
        let engine = EdgeRedirectEngine::from_rules(rules);

        let decision = engine.resolve(&make_request("x")).unwrap();
        assert_eq!(decision.target_url, "https://high.example.com");
        assert_eq!(decision.matched_rule_id, Some("high".to_string()));
    }

    #[test]
    fn test_no_match() {
        let engine = EdgeRedirectEngine::new();
        assert!(engine.resolve(&make_request("nonexistent")).is_none());
    }

    #[test]
    fn test_add_and_remove_rule() {
        let mut engine = EdgeRedirectEngine::new();
        engine.add_rule(EdgeRedirectRule {
            id: "r1".to_string(),
            code: "test".to_string(),
            condition: None,
            target_url: "https://example.com".to_string(),
            status_code: 302,
            priority: 0,
        });

        assert!(engine.resolve(&make_request("test")).is_some());
        assert!(engine.remove_rule("test", "r1"));
        assert!(engine.resolve(&make_request("test")).is_none());
    }

    #[test]
    fn test_header_match_condition() {
        let rules = vec![EdgeRedirectRule {
            id: "r1".to_string(),
            code: "api".to_string(),
            condition: Some(EdgeCondition {
                condition_type: EdgeConditionType::HeaderMatch,
                params: {
                    let mut m = HashMap::new();
                    m.insert("header".to_string(), "X-Agent".to_string());
                    m.insert("pattern".to_string(), "openai".to_string());
                    m
                },
            }),
            target_url: "https://ai.example.com".to_string(),
            status_code: 302,
            priority: 10,
        }];
        let engine = EdgeRedirectEngine::from_rules(rules);

        let mut req = make_request("api");
        req.headers.insert("X-Agent".to_string(), "OpenAI/1.0".to_string());
        let decision = engine.resolve(&req).unwrap();
        assert_eq!(decision.target_url, "https://ai.example.com");
    }

    #[test]
    fn test_export_rules() {
        let rules = vec![
            EdgeRedirectRule {
                id: "r1".to_string(),
                code: "a".to_string(),
                condition: None,
                target_url: "https://a.com".to_string(),
                status_code: 302,
                priority: 0,
            },
            EdgeRedirectRule {
                id: "r2".to_string(),
                code: "b".to_string(),
                condition: None,
                target_url: "https://b.com".to_string(),
                status_code: 301,
                priority: 0,
            },
        ];
        let engine = EdgeRedirectEngine::from_rules(rules);
        let exported = engine.export_rules();
        assert_eq!(exported.len(), 2);
    }
}
