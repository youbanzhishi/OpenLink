//! # 信任与声誉系统 (Phase 10)
//!
//! 基于交互历史计算信任分，支持信誉衰减和黑白名单。
//! 与 Phase 6 的 TrustLevel/TrustRecord 无缝集成，提供更精细的信任管理。

use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// 信任评分（连续值 0.0-100.0）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    /// Agent ID
    pub agent_id: AgentId,
    /// 信任分数 (0.0 - 100.0)
    pub score: f64,
    /// 交互总次数
    pub total_interactions: u64,
    /// 成功次数
    pub success_count: u64,
    /// 失败次数
    pub failure_count: u64,
    /// 首次交互时间
    pub first_interaction: i64,
    /// 最近交互时间
    pub last_interaction: i64,
    /// 计算时间
    pub computed_at: i64,
}

/// 信任配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustConfig {
    /// 信誉衰减速率（每天减少的分数）
    #[serde(default = "default_decay_rate")]
    pub decay_rate: f64,
    /// 不活跃天数阈值（超过此天数开始衰减）
    #[serde(default = "default_inactive_threshold_days")]
    pub inactive_threshold_days: i64,
    /// 最低信任分（衰减不会低于此值）
    #[serde(default = "default_min_score")]
    pub min_score: f64,
    /// 成功交互奖励分数
    #[serde(default = "default_success_bonus")]
    pub success_bonus: f64,
    /// 失败交互惩罚分数
    #[serde(default = "default_failure_penalty")]
    pub failure_penalty: f64,
    /// 信任阈值：低于此值自动加入观察名单
    #[serde(default = "default_watch_threshold")]
    pub watch_threshold: f64,
    /// 信任阈值：低于此值自动加入黑名单
    #[serde(default = "default_blacklist_threshold")]
    pub blacklist_threshold: f64,
}

fn default_decay_rate() -> f64 { 1.0 }
fn default_inactive_threshold_days() -> i64 { 7 }
fn default_min_score() -> f64 { 10.0 }
fn default_success_bonus() -> f64 { 2.0 }
fn default_failure_penalty() -> f64 { 5.0 }
fn default_watch_threshold() -> f64 { 30.0 }
fn default_blacklist_threshold() -> f64 { 15.0 }

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            decay_rate: default_decay_rate(),
            inactive_threshold_days: default_inactive_threshold_days(),
            min_score: default_min_score(),
            success_bonus: default_success_bonus(),
            failure_penalty: default_failure_penalty(),
            watch_threshold: default_watch_threshold(),
            blacklist_threshold: default_blacklist_threshold(),
        }
    }
}

/// 名单类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListType {
    /// 白名单（始终信任）
    Whitelist,
    /// 黑名单（始终拒绝）
    Blacklist,
    /// 观察名单（需要额外验证）
    Watchlist,
}

/// 名单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEntry {
    /// Agent ID
    pub agent_id: AgentId,
    /// 名单类型
    pub list_type: ListType,
    /// 加入时间
    pub added_at: i64,
    /// 加入原因
    pub reason: String,
    /// 过期时间（None = 永久）
    pub expires_at: Option<i64>,
}

/// 信任管理器
pub struct TrustManager {
    /// 配置
    config: TrustConfig,
    /// 信任分数表
    scores: Arc<RwLock<HashMap<AgentId, TrustScore>>>,
    /// 名单条目
    list_entries: Arc<RwLock<HashMap<AgentId, ListEntry>>>,
    /// 黑名单（快速查找）
    blacklist: Arc<RwLock<HashSet<AgentId>>>,
    /// 白名单（快速查找）
    whitelist: Arc<RwLock<HashSet<AgentId>>>,
}

impl TrustManager {
    /// 创建信任管理器
    pub fn new(config: TrustConfig) -> Self {
        Self {
            config,
            scores: Arc::new(RwLock::new(HashMap::new())),
            list_entries: Arc::new(RwLock::new(HashMap::new())),
            blacklist: Arc::new(RwLock::new(HashSet::new())),
            whitelist: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 记录成功交互
    pub async fn record_success(&self, agent_id: &str) -> TrustScore {
        let mut scores = self.scores.write().await;
        let now = chrono::Utc::now().timestamp();

        let score = scores.entry(agent_id.to_string()).or_insert_with(|| TrustScore {
            agent_id: agent_id.to_string(),
            score: 50.0, // 默认起始分
            total_interactions: 0,
            success_count: 0,
            failure_count: 0,
            first_interaction: now,
            last_interaction: now,
            computed_at: now,
        });

        score.total_interactions += 1;
        score.success_count += 1;
        score.last_interaction = now;
        score.computed_at = now;

        // 加分，但不超过 100
        score.score = (score.score + self.config.success_bonus).min(100.0);

        // 如果在观察名单且分数恢复，移出观察名单
        let current_score = score.score;
        drop(scores);

        if current_score >= self.config.watch_threshold {
            let mut entries = self.list_entries.write().await;
            if let Some(entry) = entries.get(agent_id) {
                if entry.list_type == ListType::Watchlist {
                    entries.remove(agent_id);
                    tracing::info!(agent_id = %agent_id, score = current_score, "Agent removed from watchlist");
                }
            }
        }

        // 返回更新后的分数
        let scores = self.scores.read().await;
        scores.get(agent_id).cloned().unwrap()
    }

    /// 记录失败交互
    pub async fn record_failure(&self, agent_id: &str) -> TrustScore {
        let mut scores = self.scores.write().await;
        let now = chrono::Utc::now().timestamp();

        let score = scores.entry(agent_id.to_string()).or_insert_with(|| TrustScore {
            agent_id: agent_id.to_string(),
            score: 50.0,
            total_interactions: 0,
            success_count: 0,
            failure_count: 0,
            first_interaction: now,
            last_interaction: now,
            computed_at: now,
        });

        score.total_interactions += 1;
        score.failure_count += 1;
        score.last_interaction = now;
        score.computed_at = now;

        // 扣分，但不低于最低分
        score.score = (score.score - self.config.failure_penalty).max(self.config.min_score);

        let current_score = score.score;
        drop(scores);

        // 如果分数低于阈值，自动加入观察名单或黑名单
        if current_score <= self.config.blacklist_threshold {
            self.add_to_list(agent_id, ListType::Blacklist, "Auto-blacklisted due to low trust score").await;
        } else if current_score <= self.config.watch_threshold {
            self.add_to_list(agent_id, ListType::Watchlist, "Auto-watchlisted due to declining trust score").await;
        }

        let scores = self.scores.read().await;
        scores.get(agent_id).cloned().unwrap()
    }

    /// 应用信誉衰减
    ///
    /// 对所有长期不活跃的 Agent 降低信任分。
    /// 返回受影响的 Agent 数量。
    pub async fn apply_decay(&self) -> usize {
        let mut scores = self.scores.write().await;
        let now = chrono::Utc::now().timestamp();
        let threshold_secs = self.config.inactive_threshold_days * 86400;

        let mut decayed_count = 0;

        for score in scores.values_mut() {
            let inactive_secs = now - score.last_interaction;
            if inactive_secs > threshold_secs {
                let inactive_days = inactive_secs / 86400;
                let decay = self.config.decay_rate * inactive_days as f64;
                let old_score = score.score;
                score.score = (score.score - decay).max(self.config.min_score);
                score.computed_at = now;

                if score.score != old_score {
                    decayed_count += 1;
                    tracing::debug!(
                        agent_id = %score.agent_id,
                        old_score = old_score,
                        new_score = score.score,
                        "Trust score decayed"
                    );
                }
            }
        }

        if decayed_count > 0 {
            tracing::info!(count = decayed_count, "Applied trust decay");
        }
        decayed_count
    }

    /// 获取信任分
    pub async fn get_score(&self, agent_id: &str) -> Option<TrustScore> {
        let scores = self.scores.read().await;
        scores.get(agent_id).cloned()
    }

    /// 检查 Agent 是否被信任（不在黑名单中）
    pub async fn is_trusted(&self, agent_id: &str) -> bool {
        // 白名单始终信任
        {
            let whitelist = self.whitelist.read().await;
            if whitelist.contains(agent_id) {
                return true;
            }
        }

        // 黑名单始终不信任
        {
            let blacklist = self.blacklist.read().await;
            if blacklist.contains(agent_id) {
                return false;
            }
        }

        // 检查信任分
        let scores = self.scores.read().await;
        match scores.get(agent_id) {
            Some(score) => score.score >= self.config.watch_threshold,
            None => false, // 未知 Agent 不信任
        }
    }

    /// 加入名单
    pub async fn add_to_list(&self, agent_id: &str, list_type: ListType, reason: &str) {
        let now = chrono::Utc::now().timestamp();

        let entry = ListEntry {
            agent_id: agent_id.to_string(),
            list_type: list_type.clone(),
            added_at: now,
            reason: reason.to_string(),
            expires_at: None,
        };

        {
            let mut entries = self.list_entries.write().await;
            entries.insert(agent_id.to_string(), entry);
        }

        match list_type {
            ListType::Whitelist => {
                let mut whitelist = self.whitelist.write().await;
                whitelist.insert(agent_id.to_string());
                tracing::info!(agent_id = %agent_id, reason = %reason, "Agent added to whitelist");
            }
            ListType::Blacklist => {
                let mut blacklist = self.blacklist.write().await;
                blacklist.insert(agent_id.to_string());
                tracing::warn!(agent_id = %agent_id, reason = %reason, "Agent added to blacklist");
            }
            ListType::Watchlist => {
                tracing::info!(agent_id = %agent_id, reason = %reason, "Agent added to watchlist");
            }
        }
    }

    /// 从名单中移除
    pub async fn remove_from_list(&self, agent_id: &str) -> bool {
        let entry = {
            let mut entries = self.list_entries.write().await;
            entries.remove(agent_id)
        };

        if let Some(entry) = entry {
            match entry.list_type {
                ListType::Whitelist => {
                    let mut whitelist = self.whitelist.write().await;
                    whitelist.remove(agent_id);
                }
                ListType::Blacklist => {
                    let mut blacklist = self.blacklist.write().await;
                    blacklist.remove(agent_id);
                }
                ListType::Watchlist => {}
            }
            tracing::info!(agent_id = %agent_id, "Agent removed from list");
            true
        } else {
            false
        }
    }

    /// 检查是否在黑名单中
    pub async fn is_blacklisted(&self, agent_id: &str) -> bool {
        self.blacklist.read().await.contains(agent_id)
    }

    /// 检查是否在白名单中
    pub async fn is_whitelisted(&self, agent_id: &str) -> bool {
        self.whitelist.read().await.contains(agent_id)
    }

    /// 将 TrustScore 映射到 TrustLevel
    pub fn score_to_trust_level(score: &TrustScore) -> TrustLevel {
        if score.score >= 80.0 {
            TrustLevel::Trusted
        } else if score.score >= 60.0 {
            TrustLevel::Verified
        } else if score.score >= 40.0 {
            TrustLevel::Basic
        } else {
            TrustLevel::Unverified
        }
    }

    /// 获取所有信任分数
    pub async fn list_scores(&self) -> Vec<TrustScore> {
        let scores = self.scores.read().await;
        scores.values().cloned().collect()
    }

    /// 获取配置
    pub fn config(&self) -> &TrustConfig {
        &self.config
    }
}

impl Default for TrustManager {
    fn default() -> Self {
        Self::new(TrustConfig::default())
    }
}

// ─── 单元测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trust_initial_score() {
        let manager = TrustManager::default();
        let score = manager.record_success("agent-1").await;
        assert_eq!(score.total_interactions, 1);
        assert!(score.score > 50.0); // Should get bonus
    }

    #[tokio::test]
    async fn test_trust_success_increases_score() {
        let manager = TrustManager::default();
        let initial = manager.record_success("agent-1").await;
        let after = manager.record_success("agent-1").await;
        assert!(after.score > initial.score);
    }

    #[tokio::test]
    async fn test_trust_failure_decreases_score() {
        let manager = TrustManager::default();
        manager.record_success("agent-1").await;
        let before = manager.get_score("agent-1").await.unwrap().score;
        let after = manager.record_failure("agent-1").await;
        assert!(after.score < before);
    }

    #[tokio::test]
    async fn test_trust_score_capped_at_100() {
        let manager = TrustManager::default();
        // Record many successes
        for _ in 0..100 {
            manager.record_success("agent-1").await;
        }
        let score = manager.get_score("agent-1").await.unwrap();
        assert!(score.score <= 100.0);
    }

    #[tokio::test]
    async fn test_trust_score_min_floor() {
        let config = TrustConfig {
            min_score: 10.0,
            failure_penalty: 20.0,
            ..Default::default()
        };
        let manager = TrustManager::new(config);
        // Record many failures
        for _ in 0..100 {
            manager.record_failure("agent-1").await;
        }
        let score = manager.get_score("agent-1").await.unwrap();
        assert!(score.score >= 10.0);
    }

    #[tokio::test]
    async fn test_blacklist_auto() {
        let config = TrustConfig {
            blacklist_threshold: 20.0,
            failure_penalty: 20.0,
            ..Default::default()
        };
        let manager = TrustManager::new(config);

        // Fail enough to trigger blacklist
        for _ in 0..5 {
            manager.record_failure("agent-1").await;
        }

        assert!(manager.is_blacklisted("agent-1").await);
        assert!(!manager.is_trusted("agent-1").await);
    }

    #[tokio::test]
    async fn test_whitelist_overrides_blacklist() {
        let manager = TrustManager::default();

        // Add to blacklist
        manager.add_to_list("agent-1", ListType::Blacklist, "bad behavior").await;
        assert!(!manager.is_trusted("agent-1").await);

        // Override with whitelist
        manager.add_to_list("agent-1", ListType::Whitelist, "admin override").await;
        assert!(manager.is_trusted("agent-1").await);
    }

    #[tokio::test]
    async fn test_remove_from_list() {
        let manager = TrustManager::default();
        manager.add_to_list("agent-1", ListType::Blacklist, "test").await;
        assert!(manager.is_blacklisted("agent-1").await);

        let removed = manager.remove_from_list("agent-1").await;
        assert!(removed);
        assert!(!manager.is_blacklisted("agent-1").await);
    }

    #[tokio::test]
    async fn test_score_to_trust_level() {
        let make_score = |s: f64| TrustScore {
            agent_id: "test".to_string(),
            score: s,
            total_interactions: 10,
            success_count: 8,
            failure_count: 2,
            first_interaction: 0,
            last_interaction: 0,
            computed_at: 0,
        };

        assert_eq!(TrustManager::score_to_trust_level(&make_score(90.0)), TrustLevel::Trusted);
        assert_eq!(TrustManager::score_to_trust_level(&make_score(70.0)), TrustLevel::Verified);
        assert_eq!(TrustManager::score_to_trust_level(&make_score(50.0)), TrustLevel::Basic);
        assert_eq!(TrustManager::score_to_trust_level(&make_score(20.0)), TrustLevel::Unverified);
    }

    #[tokio::test]
    async fn test_decay() {
        let manager = TrustManager::new(TrustConfig {
            inactive_threshold_days: 1,
            decay_rate: 5.0,
            ..Default::default()
        });

        // Create a score with old last_interaction
        let mut scores = manager.scores.write().await;
        let now = chrono::Utc::now().timestamp();
        scores.insert("agent-1".to_string(), TrustScore {
            agent_id: "agent-1".to_string(),
            score: 80.0,
            total_interactions: 10,
            success_count: 10,
            failure_count: 0,
            first_interaction: now - 86400 * 30,
            last_interaction: now - 86400 * 30, // 30 days ago
            computed_at: now,
        });
        drop(scores);

        let count = manager.apply_decay().await;
        assert_eq!(count, 1);

        let score = manager.get_score("agent-1").await.unwrap();
        assert!(score.score < 80.0);
    }
}
