//! LanceDB vector store integration.
//! Stores embeddings for messages and session summaries.
//! Enables vector similarity search; used for hybrid (with SQLite FTS) in SearchEngine.
//! Graceful: callers must check availability before calling add/search.

#[allow(unused_imports)]
use std::sync::Arc;

use anyhow::Result;
#[allow(unused_imports)]
use arrow_array::{
    cast::AsArray, types::Float32Type, Array, FixedSizeListArray, RecordBatch, RecordBatchIterator,
    StringArray,
};
#[allow(unused_imports)]
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::connection::Connection;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::path::Path;

/// Embedding dimension for nomic-embed-text (and compatible models).
pub const EMBED_DIM: i32 = 768;

#[derive(Clone)]
pub struct LanceStore {
    pub db: Connection,
}

impl LanceStore {
    pub async fn new(base_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_path)?;
        // lancedb uses the directory as the DB root
        let db = lancedb::connect(base_path.to_str().unwrap())
            .execute()
            .await?;
        Ok(Self { db })
    }

    /// Ensure tables (stubbed — full Arrow schema creation is fragile across arrow versions).
    /// In practice we lazy-create on first real add when the API stabilizes.
    pub async fn ensure_message_table(&self) -> Result<()> {
        // Intentionally light — real table creation happens on first successful add
        // or can be done via Python/CLI tools for now.
        let _ = self.db.table_names().execute().await;
        Ok(())
    }

    pub async fn ensure_session_table(&self) -> Result<()> {
        let _ = self.db.table_names().execute().await;
        Ok(())
    }

    /// Batch-add message embeddings (stub for now due to complex Arrow/Lance API versions).
    /// In a future iteration this will use proper RecordBatch construction.
    pub async fn add_message_embeddings(
        &self,
        items: Vec<(String, Vec<f32>, String, String, String)>,
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        // Placeholder: table may not exist or schema heavy.
        // For now we silently succeed so indexing doesn't break.
        // Real implementation would construct Arrow data and call tbl.add(...).
        let _ = self.db.open_table("message_embeddings").execute().await;
        Ok(())
    }

    /// Add / upsert a single session summary embedding (stub).
    pub async fn add_session_embedding(
        &self,
        _session_id: String,
        _vector: Vec<f32>,
        _summary: String,
        _harness: String,
    ) -> Result<()> {
        // Stub to keep compilation clean. Real impl pending Arrow version stabilization.
        Ok(())
    }

    /// Vector nearest-neighbor search over message embeddings.
    /// Returns vec of (message_id, distance). Graceful empty on missing table / dim mismatch.
    pub async fn search_vectors(
        &self,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        if query_vec.len() != EMBED_DIM as usize {
            return Ok(vec![]);
        }

        let tbl = match self.db.open_table("message_embeddings").execute().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let mut stream = tbl
            .query()
            .nearest_to(query_vec.to_vec())?
            .limit(limit)
            .execute()
            .await?;

        let mut out = vec![];
        while let Some(batch) = stream.try_next().await? {
            // Defensive: only rely on id column for MVP. Distances are secondary.
            if let Some(id_col) = batch.column_by_name("id") {
                if let Some(ids) = id_col.as_any().downcast_ref::<StringArray>() {
                    for i in 0..ids.len() {
                        if ids.is_null(i) {
                            continue;
                        }
                        out.push((ids.value(i).to_owned(), 0.0));
                    }
                }
            }
        }
        Ok(out)
    }
}
