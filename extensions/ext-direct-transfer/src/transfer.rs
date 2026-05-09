//! # P2P 传输模块
//!
//! 实现节点间的 P2P 文件传输逻辑。

use serde::{Deserialize, Serialize};

/// 传输会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferSession {
    /// 会话 ID
    pub session_id: String,
    /// 文件 ID
    pub file_id: String,
    /// 发送方节点
    pub sender: String,
    /// 接收方节点
    pub receiver: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 已传输字节
    pub transferred: u64,
    /// 状态
    pub status: TransferStatus,
    /// 创建时间
    pub started_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl TransferSession {
    /// 进度百分比
    pub fn progress_percent(&self) -> f64 {
        if self.file_size == 0 {
            return 100.0;
        }
        (self.transferred as f64 / self.file_size as f64) * 100.0
    }

    /// 是否完成
    pub fn is_done(&self) -> bool {
        matches!(
            self.status,
            TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled
        )
    }
}

/// 传输统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferStats {
    pub total_sessions: u64,
    pub active_sessions: u64,
    pub completed_sessions: u64,
    pub failed_sessions: u64,
    pub total_bytes_transferred: u64,
    pub average_speed_mbps: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_session_progress() {
        let session = TransferSession {
            session_id: "sess-1".to_string(),
            file_id: "file-1".to_string(),
            sender: "node-a".to_string(),
            receiver: "node-b".to_string(),
            file_size: 1000,
            transferred: 500,
            status: TransferStatus::InProgress,
            started_at: chrono::Utc::now(),
        };
        assert!((session.progress_percent() - 50.0).abs() < 0.01);
        assert!(!session.is_done());
    }

    #[test]
    fn test_transfer_session_completed() {
        let session = TransferSession {
            session_id: "sess-1".to_string(),
            file_id: "file-1".to_string(),
            sender: "node-a".to_string(),
            receiver: "node-b".to_string(),
            file_size: 1000,
            transferred: 1000,
            status: TransferStatus::Completed,
            started_at: chrono::Utc::now(),
        };
        assert_eq!(session.progress_percent(), 100.0);
        assert!(session.is_done());
    }
}
