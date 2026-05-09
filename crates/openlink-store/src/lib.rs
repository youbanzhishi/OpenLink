//! # OpenLink Store — 存储抽象层
//!
//! 通过 trait 抽象存储操作，不绑定具体数据库。
//! 初期使用 SQLite，后期可切换到 PostgreSQL。
//!
//! 设计铁律：存储层可替换 — 核心逻辑通过 trait 抽象，不绑定具体数据库。

pub mod traits;
pub mod sqlite;
pub mod error;

pub use traits::Store;
pub use sqlite::SqliteStore;
pub use error::StoreError;
