//! # OpenLink Cache — 缓存抽象层（Phase 5 增强）
//!
//! 支持多种缓存后端：
//! - **MemoryCache**: 内存缓存（默认，无需外部依赖）
//! - **RedisCache**: Redis 缓存（可选，需要启用 redis feature）
//! - **LayeredCache**: 层叠缓存（L1 内存 + L2 Redis）
//!
//! Phase 5 增强：
//! - 缓存预热接口 + 预热数据源
//! - 主动失效策略（TTL + 批量删除 + 前缀删除）
//! - 层叠缓存
//! - 后台驱逐任务
//! - 缓存预加载器

pub mod traits;
pub mod memory;
pub mod eviction;
pub mod preload;

#[cfg(feature = "redis")]
pub mod redis_impl;

pub use traits::{Cache, CacheEntry, CacheStats, CacheError};
pub use memory::MemoryCache;
pub use memory::LayeredCache;
pub use eviction::{BackgroundEviction, EvictionConfig, EvictionResult};
pub use preload::{
    CachePreloader, PreloadEntry, PreloadResult,
    PreloadSource, FilePreloadSource, StaticPreloadSource, PreloadError,
};

#[cfg(feature = "redis")]
pub use redis_impl::RedisCache;
