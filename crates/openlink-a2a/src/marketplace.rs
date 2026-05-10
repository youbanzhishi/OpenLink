//! # Agent 发现市场 (Phase 10)
//!
//! Agent 能力的市场注册、搜索、推荐和交易。
//! 支持能力互补推荐，让 Agent 自动发现协作伙伴。

use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// Agent 市场档案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Agent ID
    pub agent_id: AgentId,
    /// Agent 名称
    pub name: String,
    /// Agent 描述
    pub description: String,
    /// 能力标签
    pub tags: Vec<String>,
    /// 评分 (0.0 - 5.0)
    pub rating: f64,
    /// 使用次数统计
    pub usage_count: u64,
    /// 成功率
    pub success_rate: f64,
    /// 提供的能力列表
    pub provided_capabilities: Vec<String>,
    /// 需要的能力列表
    pub needed_capabilities: Vec<String>,
    /// 注册时间
    pub registered_at: i64,
    /// 最后活跃时间
    pub last_active_at: i64,
}

/// 市场搜索查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceQuery {
    /// 按能力关键词搜索
    #[serde(default)]
    pub capability: Option<String>,
    /// 按标签搜索
    #[serde(default)]
    pub tags: Vec<String>,
    /// 最低评分过滤
    #[serde(default)]
    pub min_rating: Option<f64>,
    /// 按能力提供/需求过滤
    #[serde(default)]
    pub capability_type: Option<CapabilityType>,
}

/// 能力类型（提供/需求）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityType {
    /// 提供能力
    Provided,
    /// 需求能力
    Needed,
}

/// 推荐结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// 被推荐的 Agent 档案
    pub profile: AgentProfile,
    /// 推荐分数 (0.0 - 1.0)
    pub score: f64,
    /// 推荐原因
    pub reason: String,
}

/// 市场注册表
pub struct MarketplaceRegistry {
    /// Agent 档案
    profiles: Arc<RwLock<HashMap<AgentId, AgentProfile>>>,
}

impl MarketplaceRegistry {
    /// 创建空市场
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册 Agent 档案
    pub async fn register(&self, profile: AgentProfile) -> Result<(), MarketplaceError> {
        let mut profiles = self.profiles.write().await;
        if profiles.contains_key(&profile.agent_id) {
            return Err(MarketplaceError::AlreadyRegistered(profile.agent_id.clone()));
        }
        tracing::info!(
            agent_id = %profile.agent_id,
            name = %profile.name,
            tags = ?profile.tags,
            "Agent profile registered in marketplace"
        );
        profiles.insert(profile.agent_id.clone(), profile);
        Ok(())
    }

    /// 注销 Agent 档案
    pub async fn deregister(&self, agent_id: &str) -> Result<AgentProfile, MarketplaceError> {
        let mut profiles = self.profiles.write().await;
        profiles.remove(agent_id).ok_or_else(|| MarketplaceError::NotFound(agent_id.to_string()))
    }

    /// 更新 Agent 档案
    pub async fn update(&self, agent_id: &str, update_fn: impl FnOnce(&mut AgentProfile)) -> Result<(), MarketplaceError> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles.get_mut(agent_id)
            .ok_or_else(|| MarketplaceError::NotFound(agent_id.to_string()))?;
        update_fn(profile);
        Ok(())
    }

    /// 获取 Agent 档案
    pub async fn get(&self, agent_id: &str) -> Option<AgentProfile> {
        let profiles = self.profiles.read().await;
        profiles.get(agent_id).cloned()
    }

    /// 搜索市场
    pub async fn search(&self, query: &MarketplaceQuery) -> Vec<AgentProfile> {
        let profiles = self.profiles.read().await;

        let mut results: Vec<AgentProfile> = profiles.values()
            .filter(|p| {
                // 按能力关键词过滤
                if let Some(ref cap) = query.capability {
                    let cap_lower = cap.to_lowercase();
                    let matches_provided = p.provided_capabilities.iter()
                        .any(|c| c.to_lowercase().contains(&cap_lower));
                    let matches_needed = p.needed_capabilities.iter()
                        .any(|c| c.to_lowercase().contains(&cap_lower));
                    let matches_desc = p.description.to_lowercase().contains(&cap_lower);

                    match query.capability_type {
                        Some(CapabilityType::Provided) if !matches_provided && !matches_desc => return false,
                        Some(CapabilityType::Needed) if !matches_needed && !matches_desc => return false,
                        None if !matches_provided && !matches_needed && !matches_desc => return false,
                        _ => {}
                    }
                }

                // 按标签过滤
                if !query.tags.is_empty() {
                    if !query.tags.iter().any(|tag| p.tags.iter().any(|t| t == tag)) {
                        return false;
                    }
                }

                // 按最低评分过滤
                if let Some(min_rating) = query.min_rating {
                    if p.rating < min_rating {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // 按评分降序排列
        results.sort_by(|a, b| b.rating.partial_cmp(&a.rating).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// 基于能力互补推荐协作 Agent
    ///
    /// 找到"我需要的恰好是对方提供的"这样的互补关系。
    pub async fn recommend(&self, agent_id: &str, limit: usize) -> Vec<Recommendation> {
        let profiles = self.profiles.read().await;

        let my_profile = match profiles.get(agent_id) {
            Some(p) => p,
            None => return vec![],
        };

        let mut recommendations: Vec<Recommendation> = profiles.values()
            .filter(|p| p.agent_id != agent_id)
            .filter_map(|p| {
                // 计算互补度：我需要的 / 对方提供的 + 对方需要的 / 我提供的
                let my_needs_met = my_profile.needed_capabilities.iter()
                    .filter(|need| p.provided_capabilities.iter().any(|prov| prov == *need))
                    .count();

                let their_needs_met = p.needed_capabilities.iter()
                    .filter(|need| my_profile.provided_capabilities.iter().any(|prov| prov == *need))
                    .count();

                let total_possible = my_profile.needed_capabilities.len()
                    + p.needed_capabilities.len();

                if total_possible == 0 {
                    return None;
                }

                let complementarity = (my_needs_met + their_needs_met) as f64 / total_possible as f64;

                // 额外考虑评分和成功率
                let rating_factor = p.rating / 5.0;
                let success_factor = p.success_rate;

                let score = complementarity * 0.5 + rating_factor * 0.3 + success_factor * 0.2;

                if score > 0.0 {
                    let reason = format!(
                        "Complementary: {} of your needs met, {} of their needs met (score: {:.2})",
                        my_needs_met, their_needs_met, score
                    );
                    Some(Recommendation {
                        profile: p.clone(),
                        score,
                        reason,
                    })
                } else {
                    None
                }
            })
            .collect();

        // 按推荐分数降序排列
        recommendations.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        recommendations.truncate(limit);
        recommendations
    }

    /// 记录使用（更新使用统计）
    pub async fn record_usage(&self, agent_id: &str, success: bool) -> Result<(), MarketplaceError> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles.get_mut(agent_id)
            .ok_or_else(|| MarketplaceError::NotFound(agent_id.to_string()))?;

        profile.usage_count += 1;
        // 更新成功率（滑动平均）
        let old_rate = profile.success_rate;
        let n = profile.usage_count as f64;
        if success {
            profile.success_rate = old_rate * ((n - 1.0) / n) + 1.0 / n;
        } else {
            profile.success_rate = old_rate * ((n - 1.0) / n);
        }
        profile.last_active_at = chrono::Utc::now().timestamp();

        Ok(())
    }

    /// 更新评分
    pub async fn update_rating(&self, agent_id: &str, new_rating: f64) -> Result<(), MarketplaceError> {
        if new_rating < 0.0 || new_rating > 5.0 {
            return Err(MarketplaceError::InvalidRating(new_rating));
        }

        let mut profiles = self.profiles.write().await;
        let profile = profiles.get_mut(agent_id)
            .ok_or_else(|| MarketplaceError::NotFound(agent_id.to_string()))?;
        profile.rating = new_rating;
        Ok(())
    }

    /// 列出所有档案
    pub async fn list_all(&self) -> Vec<AgentProfile> {
        let profiles = self.profiles.read().await;
        profiles.values().cloned().collect()
    }
}

impl Default for MarketplaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 市场错误
#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("Agent already registered in marketplace: {0}")]
    AlreadyRegistered(AgentId),

    #[error("Agent not found in marketplace: {0}")]
    NotFound(String),

    #[error("Invalid rating: {0}, must be 0.0-5.0")]
    InvalidRating(f64),
}

// ─── 单元测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(id: &str, provided: Vec<&str>, needed: Vec<&str>) -> AgentProfile {
        AgentProfile {
            agent_id: id.to_string(),
            name: format!("Agent {}", id),
            description: format!("Test agent {}", id),
            tags: vec!["test".to_string()],
            rating: 4.0,
            usage_count: 0,
            success_rate: 0.9,
            provided_capabilities: provided.into_iter().map(|s| s.to_string()).collect(),
            needed_capabilities: needed.into_iter().map(|s| s.to_string()).collect(),
            registered_at: chrono::Utc::now().timestamp(),
            last_active_at: chrono::Utc::now().timestamp(),
        }
    }

    #[tokio::test]
    async fn test_marketplace_register_and_get() {
        let market = MarketplaceRegistry::new();
        let profile = make_profile("agent-1", vec!["text-gen"], vec!["image-analysis"]);

        market.register(profile).await.unwrap();
        let retrieved = market.get("agent-1").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Agent agent-1");
    }

    #[tokio::test]
    async fn test_marketplace_duplicate_registration() {
        let market = MarketplaceRegistry::new();
        let profile = make_profile("agent-1", vec![], vec![]);

        market.register(profile.clone()).await.unwrap();
        assert!(market.register(profile).await.is_err());
    }

    #[tokio::test]
    async fn test_marketplace_deregister() {
        let market = MarketplaceRegistry::new();
        let profile = make_profile("agent-1", vec![], vec![]);

        market.register(profile).await.unwrap();
        market.deregister("agent-1").await.unwrap();
        assert!(market.get("agent-1").await.is_none());
    }

    #[tokio::test]
    async fn test_marketplace_search_by_capability() {
        let market = MarketplaceRegistry::new();

        let p1 = make_profile("agent-1", vec!["text-gen"], vec![]);
        let p2 = make_profile("agent-2", vec!["image-analysis"], vec![]);
        let p3 = make_profile("agent-3", vec!["text-gen", "code-review"], vec![]);

        market.register(p1).await.unwrap();
        market.register(p2).await.unwrap();
        market.register(p3).await.unwrap();

        let query = MarketplaceQuery {
            capability: Some("text-gen".to_string()),
            tags: vec![],
            min_rating: None,
            capability_type: None,
        };

        let results = market.search(&query).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_marketplace_search_by_tags() {
        let market = MarketplaceRegistry::new();

        let mut p1 = make_profile("agent-1", vec![], vec![]);
        p1.tags = vec!["nlp".to_string(), "llm".to_string()];

        let mut p2 = make_profile("agent-2", vec![], vec![]);
        p2.tags = vec!["vision".to_string()];

        market.register(p1).await.unwrap();
        market.register(p2).await.unwrap();

        let query = MarketplaceQuery {
            capability: None,
            tags: vec!["nlp".to_string()],
            min_rating: None,
            capability_type: None,
        };

        let results = market.search(&query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, "agent-1");
    }

    #[tokio::test]
    async fn test_marketplace_search_min_rating() {
        let market = MarketplaceRegistry::new();

        let mut p1 = make_profile("agent-1", vec![], vec![]);
        p1.rating = 4.5;

        let mut p2 = make_profile("agent-2", vec![], vec![]);
        p2.rating = 2.0;

        market.register(p1).await.unwrap();
        market.register(p2).await.unwrap();

        let query = MarketplaceQuery {
            capability: None,
            tags: vec![],
            min_rating: Some(3.0),
            capability_type: None,
        };

        let results = market.search(&query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, "agent-1");
    }

    #[tokio::test]
    async fn test_marketplace_recommend_complementary() {
        let market = MarketplaceRegistry::new();

        // agent-1 provides text-gen, needs image-analysis
        let p1 = make_profile("agent-1", vec!["text-gen"], vec!["image-analysis"]);
        // agent-2 provides image-analysis, needs text-gen — perfect complement!
        let p2 = make_profile("agent-2", vec!["image-analysis"], vec!["text-gen"]);
        // agent-3 provides code-review, needs translation — no overlap
        let p3 = make_profile("agent-3", vec!["code-review"], vec!["translation"]);

        market.register(p1).await.unwrap();
        market.register(p2).await.unwrap();
        market.register(p3).await.unwrap();

        let recommendations = market.recommend("agent-1", 10).await;
        assert!(!recommendations.is_empty());
        // agent-2 should be top recommendation (complementary)
        assert_eq!(recommendations[0].profile.agent_id, "agent-2");
    }

    #[tokio::test]
    async fn test_marketplace_record_usage() {
        let market = MarketplaceRegistry::new();
        let profile = make_profile("agent-1", vec![], vec![]);
        market.register(profile).await.unwrap();

        market.record_usage("agent-1", true).await.unwrap();
        market.record_usage("agent-1", true).await.unwrap();
        market.record_usage("agent-1", false).await.unwrap();

        let p = market.get("agent-1").await.unwrap();
        assert_eq!(p.usage_count, 3);
        // success rate should be ~0.67 (2 out of 3)
        assert!(p.success_rate > 0.5 && p.success_rate < 0.8);
    }

    #[tokio::test]
    async fn test_marketplace_update_rating() {
        let market = MarketplaceRegistry::new();
        let profile = make_profile("agent-1", vec![], vec![]);
        market.register(profile).await.unwrap();

        market.update_rating("agent-1", 4.8).await.unwrap();
        let p = market.get("agent-1").await.unwrap();
        assert_eq!(p.rating, 4.8);
    }

    #[tokio::test]
    async fn test_marketplace_invalid_rating() {
        let market = MarketplaceRegistry::new();
        let profile = make_profile("agent-1", vec![], vec![]);
        market.register(profile).await.unwrap();

        assert!(market.update_rating("agent-1", 6.0).await.is_err());
        assert!(market.update_rating("agent-1", -1.0).await.is_err());
    }
}
