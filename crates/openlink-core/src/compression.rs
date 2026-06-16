//! Context Compression - 滑动窗口与摘要压缩
//!
//! 长对话场景下Context持续膨胀，实现滑动窗口+摘要压缩
//! 保留关键信息丢弃冗余

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 压缩上下文配置
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// 滑动窗口大小（保留最近N轮对话）
    pub window_size: usize,
    /// Context大小阈值（字节）
    pub size_threshold: usize,
    /// 对话轮数阈值
    pub turns_threshold: usize,
    /// 摘要压缩比
    pub compression_ratio: f32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            window_size: 10,         // 保留最近10轮
            size_threshold: 100_000, // 100KB
            turns_threshold: 20,     // 20轮对话
            compression_ratio: 0.3,  // 压缩到30%
        }
    }
}

/// 对话单元
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationTurn {
    /// 角色
    pub role: String,
    /// 内容
    pub content: String,
    /// 时间戳
    pub timestamp: i64,
    /// 元数据
    pub metadata: Option<serde_json::Value>,
    /// 是否是关键信息
    pub is_critical: bool,
}

impl ConversationTurn {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            metadata: None,
            is_critical: false,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn mark_critical(mut self) -> Self {
        self.is_critical = true;
        self
    }
}

/// Context摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSummary {
    /// 摘要ID
    pub summary_id: String,
    /// 摘要内容
    pub content: String,
    /// 保留的关键信息
    pub key_points: Vec<String>,
    /// 压缩前大小（字节）
    pub original_size: usize,
    /// 压缩后大小（字节）
    pub compressed_size: usize,
    /// 摘要轮数范围
    pub turn_range: (usize, usize),
}

/// 滑动窗口
pub struct SlidingWindow {
    config: CompressionConfig,
    turns: VecDeque<ConversationTurn>,
    summaries: Vec<ContextSummary>,
}

impl SlidingWindow {
    pub fn new(config: CompressionConfig) -> Self {
        let window_size = config.window_size;
        Self {
            config,
            turns: VecDeque::with_capacity(window_size * 2),
            summaries: vec![],
        }
    }

    /// 添加对话轮
    pub fn push(&mut self, turn: ConversationTurn) {
        self.turns.push_back(turn);

        // 超过窗口大小时触发压缩
        while self.turns.len() > self.config.window_size {
            self.compress_oldest_turn();
        }
    }

    /// 压缩最老的对话轮为摘要
    fn compress_oldest_turn(&mut self) {
        if let Some(turn) = self.turns.pop_front() {
            // 如果是关键信息，保留在摘要中
            if turn.is_critical {
                let summary = ContextSummary {
                    summary_id: uuid::Uuid::new_v4().to_string(),
                    content: turn.content.clone(),
                    key_points: vec![turn.content.clone()],
                    original_size: turn.content.len(),
                    compressed_size: turn.content.len(),
                    turn_range: (0, 0), // 需要跟踪
                };
                self.summaries.push(summary);
            }
        }
    }

    /// 获取当前所有对话
    pub fn get_all(&self) -> Vec<&ConversationTurn> {
        self.turns.iter().collect()
    }

    /// 获取摘要列表
    pub fn get_summaries(&self) -> &[ContextSummary] {
        &self.summaries
    }

    /// 检查是否需要触发压缩
    pub fn should_compress(&self) -> bool {
        self.turns.len() > self.config.window_size
    }

    /// 获取当前状态
    pub fn stats(&self) -> CompressionStats {
        let total_size: usize = self.turns.iter().map(|t| t.content.len()).sum();

        let summary_size: usize = self.summaries.iter().map(|s| s.compressed_size).sum();

        CompressionStats {
            turn_count: self.turns.len(),
            summary_count: self.summaries.len(),
            total_size,
            summary_size,
            compression_ratio: if total_size > 0 {
                summary_size as f32 / total_size as f32
            } else {
                1.0
            },
        }
    }
}

/// 摘要压缩器
pub struct SummaryCompressor {
    config: CompressionConfig,
}

impl SummaryCompressor {
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }

    /// 生成摘要
    pub fn compress(&self, turns: &[ConversationTurn]) -> ContextSummary {
        let original_size: usize = turns.iter().map(|t| t.content.len()).sum();
        
        // 提取关键信息
        let key_points: Vec<String> = turns
            .iter()
            .filter(|t| t.is_critical)
            .map(|t| t.content.clone())
            .collect();

        // 简化摘要生成（实际应调用LLM）
        let content = self.generate_summary(turns);
        let compressed_size = content.len();

        ContextSummary {
            summary_id: uuid::Uuid::new_v4().to_string(),
            content,
            key_points,
            original_size,
            compressed_size,
            turn_range: (0, turns.len().saturating_sub(1)),
        }
    }

    /// 生成摘要内容（简化版，实际应调用LLM）
    fn generate_summary(&self, turns: &[ConversationTurn]) -> String {
        if turns.is_empty() {
            return String::new();
        }

        let roles: Vec<&str> = turns.iter().map(|t| t.role.as_str()).collect();
        let first_role = roles.first().unwrap_or(&"unknown");
        let last_role = roles.last().unwrap_or(&"unknown");
        
        format!(
            "[对话摘要] {}轮对话, 角色: {} -> {}, 关键点: {}个",
            turns.len(),
            first_role,
            last_role,
            turns.iter().filter(|t| t.is_critical).count()
        )
    }

    /// 提取关键信息
    pub fn extract_key_points(&self, turns: &[ConversationTurn]) -> Vec<String> {
        turns
            .iter()
            .filter(|t| {
                // 关键信息判定：标记为critical、包含错误、或包含决策
                t.is_critical
                    || t.content.contains("error")
                    || t.content.contains("ERROR")
                    || t.content.contains("决策")
                    || t.content.contains("确认")
            })
            .map(|t| t.content.clone())
            .collect()
    }
}

/// 压缩统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionStats {
    pub turn_count: usize,
    pub summary_count: usize,
    pub total_size: usize,
    pub summary_size: usize,
    pub compression_ratio: f32,
}

/// 压缩结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionResult {
    pub success: bool,
    pub summary: Option<ContextSummary>,
    pub retained_turns: usize,
    pub stats: CompressionStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sliding_window_push() {
        let config = CompressionConfig {
            window_size: 3,
            ..Default::default()
        };
        let mut window = SlidingWindow::new(config);
        
        window.push(ConversationTurn::new("user", "Hello"));
        window.push(ConversationTurn::new("assistant", "Hi"));
        window.push(ConversationTurn::new("user", "How are you?"));
        
        assert_eq!(window.turns.len(), 3);
        
        // 超过窗口大小
        window.push(ConversationTurn::new("assistant", "I'm fine"));
        assert_eq!(window.turns.len(), 3); // 保持窗口大小
    }

    #[test]
    fn test_critical_turn_preserved() {
        let mut window = SlidingWindow::new(CompressionConfig::default());
        
        let critical = ConversationTurn::new("user", "Remember this important info")
            .mark_critical();
        window.push(critical);
        
        assert!(!window.summaries.is_empty() || window.turns.iter().any(|t| t.is_critical));
    }

    #[test]
    fn test_summary_compressor() {
        let compressor = SummaryCompressor::new(CompressionConfig::default());
        
        let turns = vec![
            ConversationTurn::new("user", "Hello"),
            ConversationTurn::new("assistant", "Hi there!"),
        ];
        
        let summary = compressor.compress(&turns);
        assert!(!summary.summary_id.is_empty());
        assert!(summary.original_size > 0);
    }

    #[test]
    fn test_extract_key_points() {
        let compressor = SummaryCompressor::new(CompressionConfig::default());
        
        let turns = vec![
            ConversationTurn::new("user", "Hello"),
            ConversationTurn::new("assistant", "ERROR: something failed"),
            ConversationTurn::new("user", "OK").mark_critical(),
        ];
        
        let key_points = compressor.extract_key_points(&turns);
        assert!(key_points.len() >= 2); // ERROR和critical
    }

    #[test]
    fn test_compression_stats() {
        let config = CompressionConfig::default();
        let window = SlidingWindow::new(config);
        
        let stats = window.stats();
        assert_eq!(stats.turn_count, 0);
        assert_eq!(stats.total_size, 0);
    }
}
