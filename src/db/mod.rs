//! Database layer.
//! SQLite (primary metadata + FTS5) + LanceDB (vectors).

pub mod sqlite;
pub mod lancedb_store;

pub use sqlite::{DbStats, SqliteStore};
pub use lancedb_store::LanceStore;
