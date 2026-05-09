//! # OpenLink Core \u2014 \u6838\u5fc3\u539f\u8bed + \u8def\u7531\u5f15\u64ce
//!
//! \u672c crate \u5b9a\u4e49\u4e86 OpenLink \u7684\u4e94\u4e2a\u6838\u5fc3\u539f\u8bed\uff08Link / Route / Action / Context / Hook\uff09\uff0c
//! \u4ee5\u53ca\u57fa\u4e8e\u8fd9\u4e9b\u539f\u8bed\u6784\u5efa\u7684\u8def\u7531\u5f15\u64ce\u548c\u6269\u5c55\u6ce8\u518c\u8868\u3002
//!
//! ## \u8bbe\u8ba1\u94c1\u5f8b
//! - \u6838\u5fc3\u5c42\u96f6\u4e1a\u52a1\u903b\u8f91\uff1a\u8def\u7531\u5f15\u64ce\u4e0d\u77e5\u9053\u201c\u77ed\u94fe\u201d\u662f\u4ec0\u4e48\uff0c\u53ea\u77e5\u9053 Context \u2192 Action
//! - \u65b0\u529f\u80fd = \u6ce8\u518c\u6269\u5c55\uff1a\u4efb\u4f55\u65b0\u573a\u666f\u90fd\u4e0d\u6539\u6838\u5fc3\u4ee3\u7801
//! - \u53ef\u89c2\u6d4b\u5185\u7f6e\uff1a\u6bcf\u6b21\u8def\u7531\u51b3\u7b56\u90fd\u6709\u5b8c\u6574\u4e0a\u4e0b\u6587\u8bb0\u5f55

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
