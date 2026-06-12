//! # 应用状态

use crate::config::AppConfig;
use crate::monitoring::AppMetrics;
use openlink_core::extension_search::LazyExtensionRegistry;
use openlink_core::knowledge_sync::{InMemoryKnowledgeStore, KnowledgeSyncService};
use openlink_core::RoutingEngine;
use openlink_store::Store;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// 应用状态（共享给所有 handler）
pub struct AppState {
    /// 存储层
    pub store: Arc<dyn Store>,

    /// 路由引擎
    pub engine: Arc<RoutingEngine>,

    /// 配置
    pub config: Arc<AppConfig>,

    /// 指标
    pub metrics: Arc<AppMetrics>,

    /// 启动时间
    start_time: Instant,

    // Phase 10: Tool Search (三桥模式)
    /// Extension 搜索注册表
    pub search_registry: Arc<RwLock<LazyExtensionRegistry>>,

    // Phase 10: KnowledgeSync (ADR-009)
    /// KnowledgeSync 服务
    pub knowledge_sync: Arc<RwLock<KnowledgeSyncService>>,

}

impl AppState {
    pub fn new(store: Arc<dyn Store>, engine: Arc<RoutingEngine>, config: Arc<AppConfig>) -> Self {
        let knowledge_store = Box::new(InMemoryKnowledgeStore::new());
        let knowledge_sync = KnowledgeSyncService::new(knowledge_store);

        Self {
            store,
            engine,
            config,
            metrics: Arc::new(AppMetrics::new()),
            start_time: Instant::now(),
            search_registry: Arc::new(RwLock::new(LazyExtensionRegistry::new_empty())),
            knowledge_sync: Arc::new(RwLock::new(knowledge_sync)),
        }
    }

    /// 获取运行时间（秒）
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}
