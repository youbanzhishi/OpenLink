//! # OpenLink Edge — 边缘计算版本
//!
//! Phase 5 增强：
//! - WASM 边缘重定向逻辑（轻量级重定向决策，无需查数据库）
//! - 热链缓存：高频短链的边缘缓存策略
//! - 地理路由：根据请求来源选择最近节点（简化版，基于IP段）
//! - WASM 沙箱执行环境（trait + Mock）
//! - 健康检查：节点和缓存健康状态监测
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

pub mod router;
pub mod config;
pub mod cache;
pub mod file_transfer;
pub mod geo;
pub mod wasm_redirect;
pub mod sandbox;
pub mod health_check;

pub use router::EdgeRouter;
pub use config::EdgeConfig;
pub use cache::EdgeCache;
pub use file_transfer::FileTransferService;
pub use geo::{GeoRouter, GeoRouteConfig, NodeEndpoint};
pub use wasm_redirect::{EdgeRedirectEngine, EdgeRedirectRule, EdgeRequest, RedirectDecision};
pub use sandbox::{WasmSandbox, MockSandbox, SandboxConfig, SandboxError, WasmModuleInfo};
pub use health_check::{HealthChecker, HealthCheckConfig, HealthReport, HealthStatus, NodeHealthInfo};
