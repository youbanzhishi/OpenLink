//! # 应用状态

use crate::config::AppConfig;
use crate::monitoring::AppMetrics;
use openlink_core::RoutingEngine;
use openlink_store::Store;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
}

impl AppState {
    pub fn new(store: Arc<dyn Store>, engine: Arc<RoutingEngine>, config: Arc<AppConfig>) -> Self {
        Self {
            store,
            engine,
            config,
            metrics: Arc::new(AppMetrics::new()),
            start_time: Instant::now(),
        }
    }

    /// 获取运行时间（秒）
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}
