//! # 任务协商协议 (Phase 10)
//!
//! Agent 间的任务发布、竞标和分配协议。
//! 支持超时和重试机制，实现去中心化的任务分配。

use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// 任务提案 ID
pub type ProposalId = String;

/// 竞标 ID
pub type BidId = String;

/// 任务提案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProposal {
    /// 提案 ID
    pub id: ProposalId,
    /// 发布者 Agent ID
    pub publisher: AgentId,
    /// 任务描述
    pub description: String,
    /// 所需能力
    pub required_capabilities: Vec<String>,
    /// 输入数据描述
    #[serde(default)]
    pub input_description: String,
    /// 期望输出描述
    #[serde(default)]
    pub output_description: String,
    /// 截止时间（Unix timestamp）
    pub deadline: i64,
    /// 最高出价（资源/信用限制）
    #[serde(default)]
    pub max_budget: Option<f64>,
    /// 优先级 (1-10, 10 = 最高)
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// 创建时间
    pub created_at: i64,
    /// 提案状态
    pub status: ProposalStatus,
}

fn default_priority() -> u32 { 5 }

/// 提案状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// 开放竞标中
    Open,
    /// 已选择中标者
    Awarded,
    /// 已完成
    Completed,
    /// 已取消
    Cancelled,
    /// 已过期（无人竞标或超时）
    Expired,
}

/// 任务竞标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBid {
    /// 竞标 ID
    pub id: BidId,
    /// 提案 ID
    pub proposal_id: ProposalId,
    /// 竞标者 Agent ID
    pub bidder: AgentId,
    /// 竞标报价
    #[serde(default)]
    pub price: Option<f64>,
    /// 预计完成时间（秒）
    pub estimated_duration_secs: u64,
    /// 竞标者自信度 (0.0 - 1.0)
    pub confidence: f64,
    /// 竞标说明
    #[serde(default)]
    pub note: String,
    /// 提供的能力证明
    #[serde(default)]
    pub capability_proof: Vec<String>,
    /// 竞标时间
    pub created_at: i64,
}

/// 任务分配
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    /// 分配 ID
    pub id: String,
    /// 提案 ID
    pub proposal_id: ProposalId,
    /// 分配给谁
    pub assignee: AgentId,
    /// 中标竞标 ID
    pub winning_bid_id: BidId,
    /// 分配时间
    pub assigned_at: i64,
    /// 分配状态
    pub status: AssignmentStatus,
    /// 重试次数
    pub retry_count: u32,
    /// 最大重试次数
    pub max_retries: u32,
}

/// 分配状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    /// 已分配，等待执行
    Assigned,
    /// 执行中
    InProgress,
    /// 已完成
    Completed,
    /// 执行失败
    Failed,
    /// 重试中
    Retrying,
}

/// 协商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationConfig {
    /// 竞标超时（秒）
    #[serde(default = "default_bid_timeout")]
    pub bid_timeout_secs: u64,
    /// 执行超时（秒）
    #[serde(default = "default_execution_timeout")]
    pub execution_timeout_secs: u64,
    /// 最大重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_bid_timeout() -> u64 { 300 }
fn default_execution_timeout() -> u64 { 3600 }
fn default_max_retries() -> u32 { 3 }

impl Default for NegotiationConfig {
    fn default() -> Self {
        Self {
            bid_timeout_secs: default_bid_timeout(),
            execution_timeout_secs: default_execution_timeout(),
            max_retries: default_max_retries(),
        }
    }
}

/// 协商引擎
pub struct NegotiationEngine {
    /// 配置
    config: NegotiationConfig,
    /// 活跃提案
    proposals: Arc<RwLock<HashMap<ProposalId, TaskProposal>>>,
    /// 竞标表：proposal_id -> bids
    bids: Arc<RwLock<HashMap<ProposalId, Vec<TaskBid>>>>,
    /// 分配表
    assignments: Arc<RwLock<HashMap<ProposalId, TaskAssignment>>>,
}

impl NegotiationEngine {
    /// 创建协商引擎
    pub fn new(config: NegotiationConfig) -> Self {
        Self {
            config,
            proposals: Arc::new(RwLock::new(HashMap::new())),
            bids: Arc::new(RwLock::new(HashMap::new())),
            assignments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发布任务提案
    pub async fn publish_proposal(&self, proposal: TaskProposal) -> Result<ProposalId, NegotiationError> {
        if proposal.status != ProposalStatus::Open {
            return Err(NegotiationError::InvalidStatus("New proposal must have Open status".to_string()));
        }

        let id = proposal.id.clone();
        tracing::info!(
            proposal_id = %id,
            publisher = %proposal.publisher,
            required_caps = ?proposal.required_capabilities,
            "Task proposal published"
        );

        let mut proposals = self.proposals.write().await;
        proposals.insert(id.clone(), proposal);

        // 初始化竞标列表
        let mut bids = self.bids.write().await;
        bids.insert(id.clone(), vec![]);

        Ok(id)
    }

    /// 竞标
    pub async fn submit_bid(&self, bid: TaskBid) -> Result<BidId, NegotiationError> {
        // 检查提案是否存在且开放
        {
            let proposals = self.proposals.read().await;
            let proposal = proposals.get(&bid.proposal_id)
                .ok_or_else(|| NegotiationError::ProposalNotFound(bid.proposal_id.clone()))?;

            if proposal.status != ProposalStatus::Open {
                return Err(NegotiationError::ProposalNotOpen(bid.proposal_id.clone()));
            }

            // 检查竞标者不是发布者
            if bid.bidder == proposal.publisher {
                return Err(NegotiationError::SelfBid(bid.proposal_id.clone()));
            }

            // 检查预算
            if let Some(max_budget) = proposal.max_budget {
                if let Some(price) = bid.price {
                    if price > max_budget {
                        return Err(NegotiationError::BidExceedsBudget {
                            price,
                            budget: max_budget,
                        });
                    }
                }
            }

            // 检查竞标者是否已竞标
            let bids = self.bids.read().await;
            if let Some(existing) = bids.get(&bid.proposal_id) {
                if existing.iter().any(|b| b.bidder == bid.bidder) {
                    return Err(NegotiationError::AlreadyBid(bid.bidder.clone(), bid.proposal_id.clone()));
                }
            }
        }

        let id = bid.id.clone();
        tracing::info!(
            bid_id = %id,
            proposal_id = %bid.proposal_id,
            bidder = %bid.bidder,
            confidence = bid.confidence,
            "Bid submitted"
        );

        let mut bids = self.bids.write().await;
        bids.get_mut(&bid.proposal_id).unwrap().push(bid);

        Ok(id)
    }

    /// 选择中标者
    ///
    /// 评分策略：综合自信度、价格、能力匹配度。
    pub async fn award_proposal(&self, proposal_id: &str) -> Result<TaskAssignment, NegotiationError> {
        let (_proposal, winning_bid) = {
            let proposals = self.proposals.read().await;
            let proposal = proposals.get(proposal_id)
                .ok_or_else(|| NegotiationError::ProposalNotFound(proposal_id.to_string()))?;

            if proposal.status != ProposalStatus::Open {
                return Err(NegotiationError::ProposalNotOpen(proposal_id.to_string()));
            }

            let bids = self.bids.read().await;
            let proposal_bids = bids.get(proposal_id)
                .ok_or_else(|| NegotiationError::NoBids(proposal_id.to_string()))?;

            if proposal_bids.is_empty() {
                return Err(NegotiationError::NoBids(proposal_id.to_string()));
            }

            // 选择最佳竞标：按自信度排序，同等自信度选价格低的
            let mut sorted_bids: Vec<&TaskBid> = proposal_bids.iter().collect();
            sorted_bids.sort_by(|a, b| {
                // Higher confidence first
                let conf_cmp = b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal);
                if conf_cmp != std::cmp::Ordering::Equal {
                    return conf_cmp;
                }
                // Lower price first
                match (a.price, b.price) {
                    (Some(ap), Some(bp)) => ap.partial_cmp(&bp).unwrap_or(std::cmp::Ordering::Equal),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });

            (proposal.clone(), sorted_bids[0].clone())
        };

        // 更新提案状态
        {
            let mut proposals = self.proposals.write().await;
            if let Some(p) = proposals.get_mut(proposal_id) {
                p.status = ProposalStatus::Awarded;
            }
        }

        let assignment = TaskAssignment {
            id: uuid::Uuid::new_v4().to_string(),
            proposal_id: proposal_id.to_string(),
            assignee: winning_bid.bidder.clone(),
            winning_bid_id: winning_bid.id.clone(),
            assigned_at: chrono::Utc::now().timestamp(),
            status: AssignmentStatus::Assigned,
            retry_count: 0,
            max_retries: self.config.max_retries,
        };

        tracing::info!(
            proposal_id = %proposal_id,
            assignee = %assignment.assignee,
            bid_id = %assignment.winning_bid_id,
            "Proposal awarded"
        );

        let mut assignments = self.assignments.write().await;
        assignments.insert(proposal_id.to_string(), assignment.clone());

        Ok(assignment)
    }

    /// 取消提案
    pub async fn cancel_proposal(&self, proposal_id: &str, canceller: &str) -> Result<(), NegotiationError> {
        let mut proposals = self.proposals.write().await;
        let proposal = proposals.get_mut(proposal_id)
            .ok_or_else(|| NegotiationError::ProposalNotFound(proposal_id.to_string()))?;

        if proposal.publisher != canceller {
            return Err(NegotiationError::NotOwner(proposal_id.to_string()));
        }

        if proposal.status != ProposalStatus::Open {
            return Err(NegotiationError::InvalidStatus("Can only cancel Open proposals".to_string()));
        }

        proposal.status = ProposalStatus::Cancelled;
        tracing::info!(proposal_id = %proposal_id, "Proposal cancelled");
        Ok(())
    }

    /// 处理过期提案
    pub async fn expire_proposals(&self) -> Vec<ProposalId> {
        let now = chrono::Utc::now().timestamp();
        let mut proposals = self.proposals.write().await;
        let mut expired = vec![];

        for (id, proposal) in proposals.iter_mut() {
            if proposal.status == ProposalStatus::Open && now > proposal.deadline {
                proposal.status = ProposalStatus::Expired;
                expired.push(id.clone());
                tracing::info!(proposal_id = %id, "Proposal expired");
            }
        }

        expired
    }

    /// 标记任务完成
    pub async fn complete_assignment(&self, proposal_id: &str) -> Result<TaskAssignment, NegotiationError> {
        let mut assignments = self.assignments.write().await;
        let assignment = assignments.get_mut(proposal_id)
            .ok_or_else(|| NegotiationError::AssignmentNotFound(proposal_id.to_string()))?;

        assignment.status = AssignmentStatus::Completed;

        // 更新提案状态
        let mut proposals = self.proposals.write().await;
        if let Some(proposal) = proposals.get_mut(proposal_id) {
            proposal.status = ProposalStatus::Completed;
        }

        tracing::info!(proposal_id = %proposal_id, "Task assignment completed");
        Ok(assignment.clone())
    }

    /// 标记任务失败（支持重试）
    pub async fn fail_assignment(&self, proposal_id: &str) -> Result<TaskAssignment, NegotiationError> {
        let mut assignments = self.assignments.write().await;
        let assignment = assignments.get_mut(proposal_id)
            .ok_or_else(|| NegotiationError::AssignmentNotFound(proposal_id.to_string()))?;

        if assignment.retry_count < assignment.max_retries {
            assignment.retry_count += 1;
            assignment.status = AssignmentStatus::Retrying;
            tracing::info!(
                proposal_id = %proposal_id,
                retry = assignment.retry_count,
                max_retries = assignment.max_retries,
                "Task failed, retrying"
            );
        } else {
            assignment.status = AssignmentStatus::Failed;
            tracing::warn!(
                proposal_id = %proposal_id,
                retries = assignment.max_retries,
                "Task failed permanently"
            );
        }

        Ok(assignment.clone())
    }

    /// 获取提案
    pub async fn get_proposal(&self, proposal_id: &str) -> Option<TaskProposal> {
        let proposals = self.proposals.read().await;
        proposals.get(proposal_id).cloned()
    }

    /// 获取提案的所有竞标
    pub async fn get_bids(&self, proposal_id: &str) -> Vec<TaskBid> {
        let bids = self.bids.read().await;
        bids.get(proposal_id).cloned().unwrap_or_default()
    }

    /// 获取分配
    pub async fn get_assignment(&self, proposal_id: &str) -> Option<TaskAssignment> {
        let assignments = self.assignments.read().await;
        assignments.get(proposal_id).cloned()
    }
}

impl Default for NegotiationEngine {
    fn default() -> Self {
        Self::new(NegotiationConfig::default())
    }
}

/// 协商错误
#[derive(Debug, thiserror::Error)]
pub enum NegotiationError {
    #[error("Proposal not found: {0}")]
    ProposalNotFound(ProposalId),

    #[error("Proposal not open: {0}")]
    ProposalNotOpen(ProposalId),

    #[error("No bids for proposal: {0}")]
    NoBids(ProposalId),

    #[error("Agent cannot bid on own proposal: {0}")]
    SelfBid(ProposalId),

    #[error("Agent {0} already bid on proposal {1}")]
    AlreadyBid(AgentId, ProposalId),

    #[error("Bid price {price} exceeds budget {budget}")]
    BidExceedsBudget { price: f64, budget: f64 },

    #[error("Not proposal owner: {0}")]
    NotOwner(ProposalId),

    #[error("Invalid status: {0}")]
    InvalidStatus(String),

    #[error("Assignment not found: {0}")]
    AssignmentNotFound(ProposalId),
}

// ─── 单元测试 ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_proposal(publisher: &str, deadline_offset_secs: i64) -> TaskProposal {
        TaskProposal {
            id: uuid::Uuid::new_v4().to_string(),
            publisher: publisher.to_string(),
            description: "Test task".to_string(),
            required_capabilities: vec!["text-gen".to_string()],
            input_description: String::new(),
            output_description: String::new(),
            deadline: chrono::Utc::now().timestamp() + deadline_offset_secs,
            max_budget: Some(100.0),
            priority: 5,
            created_at: chrono::Utc::now().timestamp(),
            status: ProposalStatus::Open,
        }
    }

    fn make_bid(proposal_id: &str, bidder: &str, confidence: f64, price: Option<f64>) -> TaskBid {
        TaskBid {
            id: uuid::Uuid::new_v4().to_string(),
            proposal_id: proposal_id.to_string(),
            bidder: bidder.to_string(),
            price,
            estimated_duration_secs: 60,
            confidence,
            note: String::new(),
            capability_proof: vec![],
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    #[tokio::test]
    async fn test_publish_proposal() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let id = engine.publish_proposal(proposal).await.unwrap();
        let retrieved = engine.get_proposal(&id).await.unwrap();
        assert_eq!(retrieved.publisher, "agent-1");
        assert_eq!(retrieved.status, ProposalStatus::Open);
    }

    #[tokio::test]
    async fn test_submit_bid() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        let bid = make_bid(&proposal_id, "agent-2", 0.9, Some(50.0));
        let bid_id = engine.submit_bid(bid).await.unwrap();

        let bids = engine.get_bids(&proposal_id).await;
        assert_eq!(bids.len(), 1);
        assert_eq!(bids[0].id, bid_id);
    }

    #[tokio::test]
    async fn test_self_bid_rejected() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        let bid = make_bid(&proposal_id, "agent-1", 0.9, Some(50.0));
        assert!(engine.submit_bid(bid).await.is_err());
    }

    #[tokio::test]
    async fn test_duplicate_bid_rejected() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        let bid1 = make_bid(&proposal_id, "agent-2", 0.9, Some(50.0));
        let bid2 = make_bid(&proposal_id, "agent-2", 0.8, Some(40.0));

        engine.submit_bid(bid1).await.unwrap();
        assert!(engine.submit_bid(bid2).await.is_err());
    }

    #[tokio::test]
    async fn test_award_proposal() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        engine.submit_bid(make_bid(&proposal_id, "agent-2", 0.8, Some(60.0))).await.unwrap();
        engine.submit_bid(make_bid(&proposal_id, "agent-3", 0.95, Some(55.0))).await.unwrap();

        let assignment = engine.award_proposal(&proposal_id).await.unwrap();
        // agent-3 should win (higher confidence)
        assert_eq!(assignment.assignee, "agent-3");
        assert_eq!(assignment.status, AssignmentStatus::Assigned);
    }

    #[tokio::test]
    async fn test_award_no_bids_fails() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        assert!(engine.award_proposal(&proposal_id).await.is_err());
    }

    #[tokio::test]
    async fn test_cancel_proposal() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        engine.cancel_proposal(&proposal_id, "agent-1").await.unwrap();
        let p = engine.get_proposal(&proposal_id).await.unwrap();
        assert_eq!(p.status, ProposalStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_cancel_not_owner_fails() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        assert!(engine.cancel_proposal(&proposal_id, "agent-2").await.is_err());
    }

    #[tokio::test]
    async fn test_expire_proposals() {
        let engine = NegotiationEngine::default();
        // Proposal with deadline in the past
        let proposal = make_proposal("agent-1", -3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        let expired = engine.expire_proposals().await;
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], proposal_id);
    }

    #[tokio::test]
    async fn test_complete_and_fail_assignment() {
        let engine = NegotiationEngine::default();
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        engine.submit_bid(make_bid(&proposal_id, "agent-2", 0.9, None)).await.unwrap();
        engine.award_proposal(&proposal_id).await.unwrap();

        // Complete
        let assignment = engine.complete_assignment(&proposal_id).await.unwrap();
        assert_eq!(assignment.status, AssignmentStatus::Completed);
    }

    #[tokio::test]
    async fn test_fail_assignment_with_retry() {
        let engine = NegotiationEngine::new(NegotiationConfig {
            max_retries: 2,
            ..Default::default()
        });
        let proposal = make_proposal("agent-1", 3600);
        let proposal_id = engine.publish_proposal(proposal).await.unwrap();

        engine.submit_bid(make_bid(&proposal_id, "agent-2", 0.9, None)).await.unwrap();
        engine.award_proposal(&proposal_id).await.unwrap();

        // First failure -> retry
        let a1 = engine.fail_assignment(&proposal_id).await.unwrap();
        assert_eq!(a1.status, AssignmentStatus::Retrying);
        assert_eq!(a1.retry_count, 1);

        // Second failure -> retry
        let a2 = engine.fail_assignment(&proposal_id).await.unwrap();
        assert_eq!(a2.status, AssignmentStatus::Retrying);
        assert_eq!(a2.retry_count, 2);

        // Third failure -> permanent failure
        let a3 = engine.fail_assignment(&proposal_id).await.unwrap();
        assert_eq!(a3.status, AssignmentStatus::Failed);
    }
}
