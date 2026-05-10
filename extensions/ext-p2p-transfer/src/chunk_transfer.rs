//! # 分块传输 (Phase 9)
//!
//! 大文件分块传输、断点续传、多源并行下载、SHA256 校验。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 默认 chunk 大小：1MB
pub const DEFAULT_CHUNK_SIZE: u64 = 1024 * 1024;

/// Chunk 传输状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkState {
    /// 待传输
    Pending,
    /// 传输中
    InProgress,
    /// 已完成
    Completed,
    /// 校验失败
    Failed,
}

/// 单个 Chunk 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    /// Chunk 索引（从 0 开始）
    pub index: u32,
    /// Chunk 在文件中的偏移
    pub offset: u64,
    /// Chunk 大小（最后一个 chunk 可能小于 chunk_size）
    pub size: u64,
    /// SHA256 校验和
    pub checksum: Option<String>,
    /// 传输状态
    pub state: ChunkState,
    /// 正在传输的 peer ID
    pub assigned_peer: Option<String>,
}

/// 分块传输任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkTransferTask {
    /// 传输任务 ID
    pub task_id: String,
    /// 文件 ID
    pub file_id: String,
    /// 文件总大小
    pub total_size: u64,
    /// Chunk 大小
    pub chunk_size: u64,
    /// 总 chunk 数
    pub total_chunks: u32,
    /// 文件整体 SHA256
    pub file_checksum: Option<String>,
    /// Chunk 信息列表
    pub chunks: Vec<ChunkInfo>,
    /// 参与传输的 peer 列表
    pub peers: Vec<String>,
}

impl ChunkTransferTask {
    /// 创建新的分块传输任务
    pub fn new(task_id: String, file_id: String, total_size: u64, chunk_size: u64) -> Self {
        let total_chunks = if total_size == 0 {
            0
        } else {
            ((total_size + chunk_size - 1) / chunk_size) as u32
        };

        let chunks: Vec<ChunkInfo> = (0..total_chunks)
            .map(|i| {
                let offset = i as u64 * chunk_size;
                let size = if i == total_chunks - 1 {
                    // 最后一个 chunk 的大小
                    total_size - offset
                } else {
                    chunk_size
                };
                ChunkInfo {
                    index: i,
                    offset,
                    size,
                    checksum: None,
                    state: ChunkState::Pending,
                    assigned_peer: None,
                }
            })
            .collect();

        Self {
            task_id,
            file_id,
            total_size,
            chunk_size,
            total_chunks,
            file_checksum: None,
            chunks,
            peers: Vec::new(),
        }
    }

    /// 创建使用默认 chunk 大小的传输任务
    pub fn with_default_chunk_size(task_id: String, file_id: String, total_size: u64) -> Self {
        Self::new(task_id, file_id, total_size, DEFAULT_CHUNK_SIZE)
    }

    /// 获取已完成的 chunk 数
    pub fn completed_chunks(&self) -> u32 {
        self.chunks.iter().filter(|c| c.state == ChunkState::Completed).count() as u32
    }

    /// 获取进度百分比 (0.0 - 1.0)
    pub fn progress(&self) -> f64 {
        if self.total_chunks == 0 {
            return 1.0;
        }
        self.completed_chunks() as f64 / self.total_chunks as f64
    }

    /// 获取下一个待传输的 chunk
    pub fn next_pending_chunk(&mut self) -> Option<&ChunkInfo> {
        self.chunks.iter().find(|c| c.state == ChunkState::Pending)
    }

    /// 标记 chunk 为传输中
    pub fn start_chunk(&mut self, index: u32, peer_id: &str) -> bool {
        if let Some(chunk) = self.chunks.get_mut(index as usize) {
            if chunk.state == ChunkState::Pending {
                chunk.state = ChunkState::InProgress;
                chunk.assigned_peer = Some(peer_id.to_string());
                return true;
            }
        }
        false
    }

    /// 完成 chunk 传输
    pub fn complete_chunk(&mut self, index: u32, checksum: Option<String>) -> bool {
        if let Some(chunk) = self.chunks.get_mut(index as usize) {
            if chunk.state == ChunkState::InProgress {
                chunk.state = ChunkState::Completed;
                chunk.checksum = checksum;
                return true;
            }
        }
        false
    }

    /// 标记 chunk 失败（可重试）
    pub fn fail_chunk(&mut self, index: u32) -> bool {
        if let Some(chunk) = self.chunks.get_mut(index as usize) {
            if chunk.state == ChunkState::InProgress {
                chunk.state = ChunkState::Pending; // 重置为待传输，允许重试
                chunk.assigned_peer = None;
                return true;
            }
        }
        false
    }

    /// 检查是否全部完成
    pub fn is_complete(&self) -> bool {
        self.chunks.iter().all(|c| c.state == ChunkState::Completed)
    }

    /// 校验所有已完成的 chunk
    pub fn verify_chunks(&self) -> ChunkVerifyResult {
        let mut failed_indices = Vec::new();
        let mut verified_count = 0u32;

        for chunk in &self.chunks {
            if chunk.state == ChunkState::Completed {
                if chunk.checksum.is_none() {
                    failed_indices.push(chunk.index);
                } else {
                    verified_count += 1;
                }
            }
        }

        ChunkVerifyResult {
            verified_count,
            failed_count: failed_indices.len() as u32,
            failed_indices,
        }
    }
}

/// 校验结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkVerifyResult {
    pub verified_count: u32,
    pub failed_count: u32,
    pub failed_indices: Vec<u32>,
}

/// 多源并行下载调度器
pub struct ParallelDownloadScheduler {
    /// 传输任务
    task: ChunkTransferTask,
    /// 每个 peer 当前正在下载的 chunk 数
    peer_loads: HashMap<String, u32>,
    /// 每个 peer 的最大并发下载数
    max_concurrent_per_peer: u32,
}

impl ParallelDownloadScheduler {
    /// 创建调度器
    pub fn new(task: ChunkTransferTask, max_concurrent_per_peer: u32) -> Self {
        let mut peer_loads = HashMap::new();
        for peer in &task.peers {
            peer_loads.insert(peer.clone(), 0);
        }
        Self {
            task,
            peer_loads,
            max_concurrent_per_peer,
        }
    }

    /// 分配 chunk 给 peer
    pub fn assign_chunks(&mut self) -> Vec<(u32, String)> {
        let mut assignments = Vec::new();

        // 找到所有待传输的 chunk
        let pending_indices: Vec<u32> = self.task.chunks.iter()
            .filter(|c| c.state == ChunkState::Pending)
            .map(|c| c.index)
            .collect();

        for chunk_index in pending_indices {
            // 找到负载最低的 peer
            let best_peer = self.peer_loads.iter()
                .filter(|(_, &load)| load < self.max_concurrent_per_peer)
                .min_by_key(|(_, &load)| load)
                .map(|(peer, _)| peer.clone());

            if let Some(peer) = best_peer {
                if self.task.start_chunk(chunk_index, &peer) {
                    *self.peer_loads.get_mut(&peer).unwrap() += 1;
                    assignments.push((chunk_index, peer));
                }
            }
        }

        assignments
    }

    /// chunk 完成回调
    pub fn on_chunk_complete(&mut self, index: u32, checksum: Option<String>) {
        if let Some(chunk) = self.task.chunks.get(index as usize) {
            if let Some(ref peer) = chunk.assigned_peer {
                if let Some(load) = self.peer_loads.get_mut(peer) {
                    *load = load.saturating_sub(1);
                }
            }
        }
        self.task.complete_chunk(index, checksum);
    }

    /// 获取任务引用
    pub fn task(&self) -> &ChunkTransferTask {
        &self.task
    }

    /// 获取任务可变引用
    pub fn task_mut(&mut self) -> &mut ChunkTransferTask {
        &mut self.task
    }
}

/// 计算文件的 chunk 数
pub fn calculate_chunk_count(file_size: u64, chunk_size: u64) -> u32 {
    if file_size == 0 || chunk_size == 0 {
        return 0;
    }
    ((file_size + chunk_size - 1) / chunk_size) as u32
}

/// 生成 chunk 的 SHA256 校验和（模拟）
/// 实际实现需要读取 chunk 数据计算
pub fn compute_chunk_checksum(_chunk_index: u32, _data: &[u8]) -> String {
    // 模拟 SHA256 计算
    // 实际实现：use sha2::{Sha256, Digest};
    format!("sha256:chunk_{}_{}bytes", _chunk_index, _data.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_transfer_task_creation() {
        let task = ChunkTransferTask::new(
            "task-1".to_string(),
            "file-1".to_string(),
            5_000_000, // 5MB
            1_000_000, // 1MB chunks
        );
        assert_eq!(task.total_chunks, 5);
        assert_eq!(task.chunks[0].size, 1_000_000);
        assert_eq!(task.chunks[4].size, 1_000_000); // 5MB / 1MB = 5 chunks
    }

    #[test]
    fn test_chunk_transfer_task_uneven() {
        let task = ChunkTransferTask::new(
            "task-2".to_string(),
            "file-2".to_string(),
            3_500_000, // 3.5MB
            1_000_000, // 1MB chunks
        );
        assert_eq!(task.total_chunks, 4);
        assert_eq!(task.chunks[3].size, 500_000); // Last chunk is 500KB
    }

    #[test]
    fn test_chunk_progress() {
        let mut task = ChunkTransferTask::new(
            "task-3".to_string(),
            "file-3".to_string(),
            3_000_000,
            1_000_000,
        );
        assert!((task.progress() - 0.0).abs() < 0.01);

        task.start_chunk(0, "peer-1");
        task.complete_chunk(0, Some("sha256:abc".to_string()));
        assert!((task.progress() - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_chunk_fail_and_retry() {
        let mut task = ChunkTransferTask::new(
            "task-4".to_string(),
            "file-4".to_string(),
            2_000_000,
            1_000_000,
        );
        task.start_chunk(0, "peer-1");
        task.fail_chunk(0);
        // Chunk 0 should be back to Pending
        assert_eq!(task.chunks[0].state, ChunkState::Pending);
        assert!(task.chunks[0].assigned_peer.is_none());
    }

    #[test]
    fn test_parallel_download_scheduler() {
        let mut task = ChunkTransferTask::new(
            "task-5".to_string(),
            "file-5".to_string(),
            5_000_000,
            1_000_000,
        );
        task.peers = vec!["peer-1".to_string(), "peer-2".to_string()];

        let mut scheduler = ParallelDownloadScheduler::new(task, 2);
        let assignments = scheduler.assign_chunks();

        // Should assign chunks to peers (2 peers × 2 max = 4 chunks initially)
        assert_eq!(assignments.len(), 4);
    }

    #[test]
    fn test_verify_chunks() {
        let mut task = ChunkTransferTask::new(
            "task-6".to_string(),
            "file-6".to_string(),
            2_000_000,
            1_000_000,
        );
        task.start_chunk(0, "peer-1");
        task.complete_chunk(0, Some("sha256:abc".to_string()));
        task.start_chunk(1, "peer-1");
        task.complete_chunk(1, None); // No checksum

        let result = task.verify_chunks();
        assert_eq!(result.verified_count, 1);
        assert_eq!(result.failed_count, 1);
    }

    #[test]
    fn test_calculate_chunk_count() {
        assert_eq!(calculate_chunk_count(0, 1024), 0);
        assert_eq!(calculate_chunk_count(1024, 1024), 1);
        assert_eq!(calculate_chunk_count(1025, 1024), 2);
    }
}
