//! # OpenLink Core — 核心原语 + 路由引擎
//!
//! 本 crate 定义了 OpenLink 的五个核心原语（Link / Route / Action / Context / Hook），
//! 以及基于这些原语构建的路由引擎和扩展注册表。
//!
//! ## 设计铁律
//! - 核心层零业务逻辑：路由引擎不知道"短链"是什么，只知道 Context → Action
//! - 新功能 = 注册扩展：任何新场景都不改核心代码
//! - 可观测内置：每次路由决策都有完整上下文记录
//!
//! ## Phase 7 模块
//! - `metrics`: 统一指标收集与 Prometheus 导出
//! - `rate_limit`: 限流器（令牌桶/滑动窗口）
//! - `auth`: 认证增强（API Key/JWT）
//! - `health`: 组件级健康检查（Readiness/Liveness）
//!
//! ## Phase 9 模块
//! - `gossip`: Gossip 协议（节点发现/链路状态/成员管理/故障检测）
//! - `decentralized`: 去中心化路由引擎（最短路径/多路径冗余/降级策略）

pub mod engine;
pub mod error;
pub mod primitives;
pub mod registry;
pub mod shortcode;

// Phase 7: Monitoring & Security
pub mod auth;
pub mod health;
pub mod metrics;
pub mod rate_limit;

// Phase 9: Decentralized Routing
pub mod decentralized;
pub mod gossip;

pub use engine::RoutingEngine;
pub use error::CoreError;
pub use primitives::*;
pub use registry::ExtensionRegistry;
pub use registry::{ActionHandler, ConditionHandler, HookHandler};
pub use shortcode::{generate, generate_default, is_valid};

// Phase 7: Re-export key types
pub use auth::{
    ApiKeyAuth, ApiKeyConfig, AuthMiddleware, AuthProvider, AuthResult, Credentials, JwtAlgorithm,
    JwtAuth, JwtConfig,
};
pub use health::{
    CacheHealthCheck, ComponentHealth, ComponentStatus, DatabaseHealthCheck, HealthCheck,
    HealthChecker, HealthEndpoint, LivenessProbe, LivenessResult, OverallHealth, ReadinessProbe,
    ReadinessResult, UpstreamHealthCheck,
};
pub use metrics::{
    CacheMetrics, InMemoryMetrics, LatencyTracker, MetricsCollector, MetricsMiddleware,
    MetricsSnapshot, PrometheusExporter, RequestMetricsTimer,
};
pub use rate_limit::{
    CompositeRateLimiter, RateLimitAlgorithm, RateLimitConfig, RateLimitMiddleware,
    RateLimitResult, RateLimitStatus, RateLimitStrategy, RateLimiter, SlidingWindowLimiter,
    TokenBucketLimiter,
};

// Phase 9: Re-export decentralized routing types
pub use decentralized::{
    DecentralizedRouter, DegradationStrategy, RoutePath, RouteResult, RouteStrategy, RoutingTable,
    RoutingTableEntry,
};
pub use gossip::{
    GossipConfig, GossipMembership, GossipMessage, LinkStateEntry, NodeId, NodeInfo, NodeStatus,
};
