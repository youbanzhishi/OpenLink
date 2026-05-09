//! # OpenLink Core — 核心原语 + 路由引擎
//!
//! 本 crate 定义了 OpenLink 的五个核心原语（Link / Route / Action / Context / Hook），
//! 以及基于这些原语构建的路由引擎和扩展注册表。
//!
//! ## 设计铁律
//! - 核心层零业务逻辑：路由引擎不知道"短链"是什么，只知道 Context → Action
//! - 新功能 = 注册扩展：任何新场景都不改核心代码
//! - 可观测内置：每次路由决策都有完整上下文记录

pub mod primitives;
pub mod engine;
pub mod registry;
pub mod error;
pub mod shortcode;

pub use primitives::*;
pub use engine::RoutingEngine;
pub use registry::ExtensionRegistry;
pub use registry::{ActionHandler, ConditionHandler, HookHandler};
pub use error::CoreError;
pub use shortcode::{generate, generate_default, is_valid};
