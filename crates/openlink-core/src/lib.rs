//! # OpenLink Core — 核心原语 + 路由引擎
//!
//! 本 crate 定义了 OpenLink 的五个核心原语（Link / Route / Action / Context / Hook），
//! 以及基于这些原语构建的路由引擎和扩展注册表。

//! ## 设计铁律
//! - 核心层零业务逻辑：路由引擎不知道"短链"是什么，只知道 Context → Action
//! - 新功能 = 注册扩展：任何新场景都不改核心代码
//! - 可观测内置：每次路由决策都有完整上下文记录

//! ## Phase 7 模块
//! - `metrics`: 统一指标收集与 Prometheus 导出
//! - `rate_limit`: 限流器（令牌桶/滑动窗口）
//! - `auth`: 认证增强（API Key/JWT）
//! - `health`: 组件级健康检查（Readiness/Liveness）

//! ## Phase 9 模块
//! - `gossip`: Gossip 协议（节点发现/链路状态/成员管理/故障检测）
//! - `decentralized`: 去中心化路由引擎（最短路径/多路径冗余/降级策略）

//! ## Phase 10 模块
//! - `extension_search`: 扩展搜索（三桥模式）
//! - `knowledge_sync`: 知识同步

//! ## Phase 3.5 模块
//! - `context_filter`: 条件化Extension暴露引擎

pub mod engine;
pub mod error;
pub mod primitives;
pub mod registry;
pub mod shortcode;

// Phase 3.5: ContextFilter 条件化Extension暴露
pub mod context_filter;

// Phase 7: Monitoring & Security
pub mod auth;
pub mod health;
pub mod metrics;
pub mod rate_limit;

// Phase 9: Decentralized Routing
pub mod decentralized;
pub mod gossip;

// Phase 10: Tool Search (三桥模式) + KnowledgeSync (ADR-009)
pub mod extension_search;
pub mod knowledge_sync;

pub mod hooks;
pub mod memory;
pub mod compression;  // WO-082: ContextCompression滑动窗口

pub use engine::RoutingEngine;
pub use error::CoreError;
pub use primitives::*;
pub use registry::ExtensionRegistry;
pub use registry::{ActionHandler, ConditionHandler, HookHandler};
pub use shortcode::{generate, generate_default, is_valid};

// Phase 3.5: Re-export ContextFilter types
pub use context_filter::{
    ContextFilterEngine, ExtensionFilter, ExtensionFilterTarget, FilterContext, FilterStats, TaskPhase,
};

// Phase 7: Re-export key types
pub use auth::{
    ApiKeyAuth, ApiKeyConfig, AuthMiddleware, AuthProvider, AuthResult, Credentials, JwtAlgorithm, JwtAuth, JwtConfig,
};
pub use health::{
    CacheHealthCheck, ComponentHealth, ComponentStatus, DatabaseHealthCheck, HealthCheck, HealthChecker,
    HealthEndpoint, LivenessProbe, LivenessResult, OverallHealth, ReadinessProbe, ReadinessResult, UpstreamHealthCheck,
};
pub use metrics::{
    CacheMetrics, InMemoryMetrics, LatencyTracker, MetricsCollector, MetricsMiddleware, MetricsSnapshot,
    PrometheusExporter, RequestMetricsTimer,
};
pub use rate_limit::{
    CompositeRateLimiter, RateLimitAlgorithm, RateLimitConfig, RateLimitMiddleware, RateLimitResult, RateLimitStatus,
    RateLimitStrategy, RateLimiter, SlidingWindowLimiter, TokenBucketLimiter,
};

// Phase 9: Re-export decentralized routing types
pub use decentralized::{
    DecentralizedRouter, DegradationStrategy, RoutePath, RouteResult, RouteStrategy, RoutingTable, RoutingTableEntry,
};
pub use gossip::{GossipConfig, GossipMembership, GossipMessage, LinkStateEntry, NodeId, NodeInfo, NodeStatus};

// Phase 10: Re-export Tool Search types
pub use extension_search::{
    Bm25Searcher, ExtensionExecuteRequest, ExtensionExecuteResponse, ExtensionIndex, ExtensionSchema,
    ExtensionSearchRequest, ExtensionSearchResponse, ExtensionType, LazyExtensionRegistry,
};

// Phase 10: Re-export KnowledgeSync types
pub use knowledge_sync::{
    ApiKeyManager, ApiKeyRecord, InMemoryKnowledgeStore, KnowledgeAuthRequest, KnowledgeAuthResponse,
    KnowledgeCallbackNotification, KnowledgeCallbackRequest, KnowledgeCallbackResponse, KnowledgeEventType,
    KnowledgeGrantType, KnowledgeMetadata, KnowledgeQueryRequest, KnowledgeQueryResponse, KnowledgeQueryResult,
    KnowledgeReadRequest, KnowledgeReadResponse, KnowledgeScope, KnowledgeStore, KnowledgeSyncCapability,
    KnowledgeSyncEndpoints, KnowledgeSyncService, KnowledgeWriteRequest, KnowledgeWriteResponse, KnowledgeWriteStatus,
};

// Phase 3.5: Re-export Compression types
pub use compression::{
    CompressionConfig, CompressionResult, CompressionStats, ContextSummary, ConversationTurn, SlidingWindow,
    SummaryCompressor,
};
