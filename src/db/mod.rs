//! Database layer.
//! SQLite (primary metadata + FTS5) + LanceDB (vectors).

pub mod lancedb_store;
pub mod sqlite;

pub use lancedb_store::LanceStore;
pub use sqlite::{DbStats, SqliteStore};
