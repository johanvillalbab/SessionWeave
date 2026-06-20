//! Search engine: hybrid (FTS5 + vector) + weave / timeline logic.

use anyhow::Result;

use crate::config::Config;
use crate::db::{LanceStore, SqliteStore};
use crate::ollama::OllamaClient;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub session_id: String,
    pub harness: String,
    pub role: String,
    pub timestamp: String,
    pub snippet: String,
    pub files: Option<Vec<String>>,
    /// True when this hit was boosted or included thanks to vector similarity (LanceDB).
    /// Used by CLI to show "[vector hit contributed to ranking]".
    #[serde(default)]
    pub via_vector: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct WeaveRecap {
    pub summary: String,
    pub timeline: String,
    pub key_points: String,
    pub excerpts: String,
    pub paste_ready_block: String,
}

pub struct SearchEngine {
    _config: Config,
    store: SqliteStore,
    /// Optional LanceDB store (created if vectors/ dir usable). None => pure FTS fallback.
    lance: Option<LanceStore>,
    /// Optional Ollama client (only if enabled + /api/tags succeeds). None => no embeddings.
    ollama: Option<OllamaClient>,
}

impl SearchEngine {
    pub async fn new(config: Config) -> Result<Self> {
        let db_path = config.resolve_data_path("db/sessionweave.db");
        let store = SqliteStore::new(&db_path)?;

        // LanceDB for vectors (graceful: absent => pure FTS)
        let vectors_path = config.resolve_data_path("vectors");
        let lance = LanceStore::new(&vectors_path).await.ok();

        // Ollama only if enabled and reachable (checked once at construction)
        let ollama = if config.ollama.enabled {
            let client = OllamaClient::new(
                &config.ollama.url,
                &config.ollama.embedding_model,
                &config.ollama.chat_model,
            );
            if client.is_available().await {
                Some(client)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            _config: config,
            store,
            lance,
            ollama,
        })
    }

    /// Returns true if both Lance vectors and Ollama are available for hybrid search.
    pub fn is_hybrid_ready(&self) -> bool {
        self.lance.is_some() && self.ollama.is_some()
    }

    pub async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
        _project: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        // 1. FTS5 is always available and fast (primary candidate source).
        let tolerant = make_fts_query(query);
        let fts_msgs = self.store.fts_search_messages(&tolerant, limit * 3)?;

        // 2. Vector nearest neighbors via Lance + Ollama (if available).
        //    We embed the *query* (not the docs at search time).
        let mut vec_ids: Vec<String> = vec![];
        if let (Some(ref lance), Some(ref ollama)) = (&self.lance, &self.ollama) {
            if let Ok(qvec) = ollama.embed(query).await {
                if let Ok(vhits) = lance.search_vectors(&qvec, limit * 2).await {
                    vec_ids = vhits.into_iter().map(|(id, _dist)| id).collect();
                }
            }
            // Any error (Ollama down, bad dim, no table) => vec_ids stays empty => pure FTS path.
        }
        let vector_hit_ids: std::collections::HashSet<String> = vec_ids.iter().cloned().collect();

        // 3. Simple reciprocal rank fusion (RRF) over the two result sets.
        //    This is a lightweight, effective hybrid without needing Lance hybrid path.
        use std::collections::HashMap;
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();
        const K: f32 = 60.0;

        for (rank, m) in fts_msgs.iter().enumerate() {
            let rank = (rank + 1) as f32;
            *rrf_scores.entry(m.id.clone()).or_insert(0.0) += 1.0 / (K + rank);
        }
        for (rank, vid) in vec_ids.iter().enumerate() {
            let rank = (rank + 1) as f32;
            *rrf_scores.entry(vid.clone()).or_insert(0.0) += 1.0 / (K + rank);
        }

        // Rank by fused score (higher better).
        let mut ranked: Vec<(String, f32)> = rrf_scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 4. Materialize SearchHits. Prefer FTS rows we already fetched; backfill via get_message for pure-vector hits.
        let mut fts_by_id: HashMap<String, crate::models::Message> =
            fts_msgs.into_iter().map(|m| (m.id.clone(), m)).collect();

        let mut hits = vec![];
        let mut seen = std::collections::HashSet::new();
        for (id, _score) in ranked {
            if !seen.insert(id.clone()) {
                continue;
            }

            let m = if let Some(m) = fts_by_id.remove(&id) {
                m
            } else if let Ok(Some(m)) = self.store.get_message(&id) {
                m
            } else {
                continue;
            };

            let session = self.store.get_session(&m.session_id)?;
            let harness = session.as_ref().map(|s| s.harness.clone()).unwrap_or_default();

            let files = if m.files.is_empty() { None } else { Some(m.files.clone()) };
            let via_vector = vector_hit_ids.contains(&m.id);
            hits.push(SearchHit {
                session_id: m.session_id.clone(),
                harness,
                role: m.role.to_string(),
                timestamp: m.timestamp.to_rfc3339(),
                snippet: m.preview(220),
                files,
                via_vector,
            });

            if hits.len() >= limit {
                break;
            }
        }

        // If everything failed we still return [] (consistent with old pure-FTS empty case).
        Ok(hits)
    }

    pub async fn weave(&self, query: &str, _full: bool, limit: usize) -> Result<WeaveRecap> {
        let hits = self.hybrid_search(query, limit, None).await?;

        if hits.is_empty() {
            return Ok(WeaveRecap {
                summary: format!("No relevant sessions found for \"{}\"", query),
                timeline: "-".into(),
                key_points: "-".into(),
                excerpts: "".into(),
                paste_ready_block: format!("Query: {}\n(No context found in SessionWeave)", query),
            });
        }

        // Build a simple recap without LLM for MVP
        let mut timeline_lines = vec![];
        let mut key_files = std::collections::HashSet::new();
        let mut excerpts = String::new();

        // Collect LLM-extracted decisions and tags (from decisions/session_tags tables populated at index time)
        let mut extracted_decisions: Vec<String> = vec![];
        let mut extracted_tags: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (i, h) in hits.iter().take(8).enumerate() {
            timeline_lines.push(format!(
                "- **{}** ({}): {}",
                h.timestamp.chars().take(10).collect::<String>(),
                h.harness,
                h.snippet.chars().take(90).collect::<String>()
            ));
            if let Some(fs) = &h.files {
                for f in fs {
                    key_files.insert(f.clone());
                }
            }
            // Pull LLM extracted (populated only if indexing ran with Ollama enabled)
            if let Ok(decs) = self.store.get_decisions_for_session(&h.session_id) {
                for d in decs {
                    if !extracted_decisions.contains(&d) {
                        extracted_decisions.push(d);
                    }
                }
            }
            if let Ok(tags) = self.store.get_tags_for_session(&h.session_id) {
                for t in tags {
                    extracted_tags.insert(t);
                }
            }
            if i < 3 {
                excerpts.push_str(&format!("\n**Excerpt {}** ({}):\n> {}\n", i + 1, h.role, h.snippet));
            }
        }

        if key_files.is_empty() {
            // Fallback: try to extract from snippets if parser didn't populate files
            let mut tmp: Vec<String> = vec![];
            for h in hits.iter().take(5) {
                crate::index::parsers::extract_file_paths(&h.snippet, &mut tmp);
            }
            for f in tmp {
                key_files.insert(f);
            }
        }

        let summary = format!(
            "Found {} relevant fragments across {} sessions for \"{}\".\n\
             This is the synthesized picture from your local AI sessions.",
            hits.len(),
            hits.iter().map(|h| &h.session_id).collect::<std::collections::HashSet<_>>().len(),
            query
        );

        let files_md = if key_files.is_empty() {
            "*(no specific files extracted yet)*".to_string()
        } else {
            key_files.iter().take(12).map(|f| format!("- `{}`", f)).collect::<Vec<_>>().join("\n")
        };

        let decs_md = if extracted_decisions.is_empty() {
            "*(no LLM-extracted decisions/tags — index with Ollama to populate)*".to_string()
        } else {
            extracted_decisions.iter().take(6).map(|d| format!("- {}", d)).collect::<Vec<_>>().join("\n")
        };

        let tags_md = if extracted_tags.is_empty() {
            String::new()
        } else {
            format!("\n**Feature tags:** {}", extracted_tags.iter().cloned().collect::<Vec<_>>().join(", "))
        };

        let key_points = format!(
            "**Key files mentioned:**\n{}\n\n**Extracted decisions:**\n{}{}",
            files_md, decs_md, tags_md
        );

        let paste_block = format!(
            r#"# SessionWeave Context — {}
{}
## Timeline
{}

## Key Context
{}
## Extracted (from prior indexing)
Decisions: {}
Tags: {}
"#,
            query,
            summary,
            timeline_lines.join("\n"),
            key_points,
            if extracted_decisions.is_empty() { "(none)".to_string() } else { extracted_decisions.iter().take(4).cloned().collect::<Vec<_>>().join(" ; ") },
            if extracted_tags.is_empty() { "(none)".to_string() } else { extracted_tags.iter().cloned().collect::<Vec<_>>().join(", ") }
        );

        Ok(WeaveRecap {
            summary,
            timeline: timeline_lines.join("\n"),
            key_points,
            excerpts,
            paste_ready_block: paste_block,
        })
    }

    pub async fn timeline(&self, feature: Option<&str>, limit: usize) -> Result<String> {
        let q = feature.unwrap_or(".*");
        let hits = self.hybrid_search(q, limit, None).await?;

        let mut out = String::from("# Timeline\n\n");
        for h in hits {
            out.push_str(&format!(
                "- **{}** — *{}* — {}\n  {}\n\n",
                h.timestamp.chars().take(19).collect::<String>(),
                h.harness,
                h.role,
                h.snippet
            ));
        }
        Ok(out)
    }
}

/// Turn a natural language query into a tolerant FTS5 query.
/// Example: "building auth system" → "building OR auth OR system*"
fn make_fts_query(q: &str) -> String {
    let tokens: Vec<String> = q
        .split_whitespace()
        .filter(|t| t.len() > 1)
        .map(|t| {
            if t.ends_with('*') {
                t.to_string()
            } else {
                format!("{}*", t)
            }
        })
        .collect();

    if tokens.is_empty() {
        q.to_string()
    } else {
        tokens.join(" OR ")
    }
}
