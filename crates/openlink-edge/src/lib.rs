//! # OpenLink Edge — 边缘计算版本
//!
//! Phase 5 增强：
//! - WASM 边缘重定向逻辑（轻量级重定向决策，无需查数据库）
//! - 热链缓存：高频短链的边缘缓存策略
//! - 地理路由：根据请求来源选择最近节点（简化版，基于IP段）
//! - WASM 沙箱执行环境（trait + Mock）
//! - 健康检查：节点和缓存健康状态监测
//!
//! Phase 9 增强：
//! - `edge_runtime`: 边缘运行时（请求管道/优先级队列/并发限制/超时）
//! - `edge_metrics`: 边缘指标（请求量/延迟/错误率/缓存命中率/资源使用）
//!
//! ## 精简策略
//! - 仅保留核心路由 + 文件传输功能
//! - 去掉 API Server（独立部署）
//! - 去掉数据库依赖（配置从文件读取）
//! - 使用轻量 HTTP 服务器 (tiny_http)
//!
//! ## 适用场景
//! - IoT 设备上的文件共享
//! - 路由器上的直连传输
//! - 边缘节点的最小化路由

pub mod cache;
pub mod config;
pub mod edge_metrics;
pub mod edge_runtime;
pub mod file_transfer;
pub mod geo;
pub mod health_check;
pub mod router;
pub mod sandbox;
pub mod wasm_redirect;

pub use cache::EdgeCache;
pub use config::EdgeConfig;
pub use edge_metrics::{
    EdgeMetricsCollector, EdgeMetricsSnapshot, LatencyStats, RequestTimer, ResourceUsage,
};
pub use edge_runtime::{
    EdgeResponse, EdgeRuntime, PipelineStage, RequestPriority, RuntimeConfig, RuntimeRequest,
    RuntimeStats,
};
pub use file_transfer::FileTransferService;
pub use geo::{GeoRouteConfig, GeoRouter, NodeEndpoint};
pub use health_check::{
    HealthCheckConfig, HealthChecker, HealthReport, HealthStatus, NodeHealthInfo,
};
pub use router::EdgeRouter;
pub use sandbox::{MockSandbox, SandboxConfig, SandboxError, WasmModuleInfo, WasmSandbox};
pub use wasm_redirect::{EdgeRedirectEngine, EdgeRedirectRule, EdgeRequest, RedirectDecision};
