//! # OpenLink Store — 存储抽象层
//!
//! 通过 trait 抽象存储操作，不绑定具体数据库。
//! 初期使用 SQLite，后期可切换到 PostgreSQL。
//!
//! 设计铁律：存储层可替换 — 核心逻辑通过 trait 抽象，不绑定具体数据库。

pub mod error;
pub mod sqlite;
pub mod traits;

pub use error::StoreError;
pub use sqlite::SqliteStore;
pub use traits::Store;
