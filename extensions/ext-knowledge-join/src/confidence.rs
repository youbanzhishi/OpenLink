//! KnowledgeSync置信度元数据
//!
//! 为知识条目增加来源+置信度+衰减元数据
//! - 来源分类：用户输入/工具返回/推理产出/历史缓存
//! - 置信度计算：来源权重 × 时间衰减 × 交叉验证
//! - 同步时自动附加置信度元数据

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 知识来源类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// 用户直接输入
    UserInput,
    /// 工具/Extension返回
    ToolOutput,
    /// AI推理产出
    InferenceResult,
    /// 历史缓存
    HistoricalCache,
}

impl SourceType {
    /// 来源权重（0.0-1.0）
    pub fn weight(&self) -> f64 {
        match self {
            SourceType::UserInput => 1.0,
            SourceType::ToolOutput => 0.85,
            SourceType::InferenceResult => 0.7,
            SourceType::HistoricalCache => 0.5,
        }
    }
}

/// 置信度元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceMetadata {
    /// 知识条目ID
    pub knowledge_id: String,

    /// 来源类型
    pub source_type: SourceType,

    /// 初始置信度分数（0.0-1.0）
    pub initial_score: f64,

    /// 当前置信度（经过衰减计算）
    pub current_score: f64,

    /// 来源溯源信息
    pub provenance: String,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 最后验证时间
    pub last_verified_at: Option<DateTime<Utc>>,

    /// 衰减率（每天衰减比例）
    pub decay_rate: f64,

    /// 交叉验证次数
    pub cross_validation_count: u32,

    /// 交叉验证通过次数
    pub cross_validation_passes: u32,
}

impl ConfidenceMetadata {
    /// 创建新的置信度元数据
    pub fn new(knowledge_id: String, source_type: SourceType, initial_score: f64, provenance: String) -> Self {
        let now = Utc::now();
        Self {
            knowledge_id,
            source_type: source_type.clone(),
            initial_score,
            current_score: initial_score * source_type.weight(),
            provenance,
            created_at: now,
            last_verified_at: None,
            decay_rate: 0.01, // 默认每天衰减1%
            cross_validation_count: 0,
            cross_validation_passes: 0,
        }
    }

    /// 计算当前置信度（考虑时间衰减）
    pub fn compute_current_score(&mut self) -> f64 {
        let now = Utc::now();
        let days_since_creation = (now - self.created_at).num_days().max(0) as f64;

        // 时间衰减：score * (1 - decay_rate)^days
        let decay_factor = (1.0 - self.decay_rate).powf(days_since_creation);

        // 交叉验证加成：每次通过+5%，上限+30%
        let validation_bonus = (self.cross_validation_passes as f64 * 0.05).min(0.3);

        self.current_score = (self.initial_score * self.source_type.weight() * decay_factor + validation_bonus).min(1.0);
        self.current_score
    }

    /// 交叉验证
    pub fn cross_validate(&mut self, passed: bool) {
        self.cross_validation_count += 1;
        if passed {
            self.cross_validation_passes += 1;
        }
        self.last_verified_at = Some(Utc::now());
        self.compute_current_score();
    }

    /// 是否可信（置信度高于阈值）
    pub fn is_trusted(&self, threshold: f64) -> bool {
        self.current_score >= threshold
    }

    /// 交叉验证通过率
    pub fn validation_rate(&self) -> f64 {
        if self.cross_validation_count == 0 {
            1.0 // 未经验证，默认可信
        } else {
            self.cross_validation_passes as f64 / self.cross_validation_count as f64
        }
    }
}

/// 置信度计算引擎
#[derive(Debug, Clone)]
pub struct ConfidenceEngine {
    /// 可信阈值
    trust_threshold: f64,
}

impl ConfidenceEngine {
    pub fn new(trust_threshold: f64) -> Self {
        Self { trust_threshold }
    }

    /// 计算知识条目置信度
    pub fn evaluate(&self, metadata: &mut ConfidenceMetadata) -> ConfidenceResult {
        let score = metadata.compute_current_score();
        let trusted = metadata.is_trusted(self.trust_threshold);

        ConfidenceResult {
            knowledge_id: metadata.knowledge_id.clone(),
            score,
            trusted,
            source_weight: metadata.source_type.weight(),
            validation_rate: metadata.validation_rate(),
            recommendation: if trusted {
                "use_with_confidence".into()
            } else if score > self.trust_threshold * 0.8 {
                "use_with_caution".into()
            } else {
                "verify_before_use".into()
            },
        }
    }

    /// 批量评估
    pub fn evaluate_batch(&self, entries: &mut [ConfidenceMetadata]) -> Vec<ConfidenceResult> {
        entries.iter_mut().map(|e| self.evaluate(e)).collect()
    }
}

/// 置信度评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceResult {
    pub knowledge_id: String,
    pub score: f64,
    pub trusted: bool,
    pub source_weight: f64,
    pub validation_rate: f64,
    pub recommendation: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_creation() {
        let meta = ConfidenceMetadata::new(
            "k001".into(), SourceType::UserInput, 0.9, "user_direct".into(),
        );
        assert_eq!(meta.knowledge_id, "k001");
        assert!(meta.current_score > 0.0);
        assert_eq!(meta.cross_validation_count, 0);
    }

    #[test]
    fn test_source_type_weights() {
        assert_eq!(SourceType::UserInput.weight(), 1.0);
        assert_eq!(SourceType::ToolOutput.weight(), 0.85);
        assert_eq!(SourceType::InferenceResult.weight(), 0.7);
        assert_eq!(SourceType::HistoricalCache.weight(), 0.5);
    }

    #[test]
    fn test_cross_validation() {
        let mut meta = ConfidenceMetadata::new(
            "k001".into(), SourceType::ToolOutput, 0.8, "ext-search".into(),
        );

        meta.cross_validate(true);
        meta.cross_validate(true);
        meta.cross_validate(false);

        assert_eq!(meta.cross_validation_count, 3);
        assert_eq!(meta.cross_validation_passes, 2);
        assert!((meta.validation_rate() - 0.667).abs() < 0.01);
    }

    #[test]
    fn test_confidence_engine() {
        let engine = ConfidenceEngine::new(0.6);
        let mut meta = ConfidenceMetadata::new(
            "k001".into(), SourceType::UserInput, 0.9, "user_direct".into(),
        );

        let result = engine.evaluate(&mut meta);
        assert!(result.trusted);
        assert!(result.score > 0.6);
    }

    #[test]
    fn test_low_confidence_not_trusted() {
        let engine = ConfidenceEngine::new(0.6);
        let mut meta = ConfidenceMetadata::new(
            "k001".into(), SourceType::HistoricalCache, 0.4, "old_cache".into(),
        );

        let result = engine.evaluate(&mut meta);
        assert!(!result.trusted);
    }
}
