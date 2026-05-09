//! # OpenLink Cache — 缓存抽象层
//!
//! 支持多种缓存后端：
//! - **MemoryCache**: 内存缓存（默认，无需外部依赖）
//! - **RedisCache**: Redis 缓存（可选，需要启用 redis feature）
//!
//! ## 设计原则
//! - Trait 抽象，不绑定具体实现
//! - 热链缓存：高频访问的 Link 路由信息缓存
//! - TTL 支持：自动过期

pub mod traits;
pub mod memory;

#[cfg(feature = "redis")]
pub mod redis_impl;

pub use traits::{Cache, CacheEntry, CacheStats};
pub use memory::MemoryCache;
#[cfg(feature = "redis")]
pub use redis_impl::RedisCache;
