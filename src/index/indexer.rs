//! Main Indexer orchestrator.
//! Responsible for discovering files, choosing parsers, and persisting.

use anyhow::Result;
use std::path::Path;
use walkdir::WalkDir;

use crate::config::{Config, Source};
use crate::db::{LanceStore, SqliteStore};
use crate::index::parsers::{parse_claude_jsonl, parse_generic, ParserResult};
use crate::index::extractor::{extract_for_session, extract_from_message, extract_from_turn};
use crate::models::{Role, Session};
use crate::ollama::OllamaClient;
use crate::utils::content_hash;

#[derive(Debug, Default, Clone)]
pub struct IndexStats {
    pub sessions_indexed: usize,
    pub messages_indexed: usize,
    pub files_scanned: usize,
    pub skipped: usize,
    pub embeddings_added: usize,
}

pub struct Indexer {
    config: Config,
    store: SqliteStore,
    // ollama + lance obtained via lightweight build_* helpers (no upfront ping for index path).
}

impl Indexer {
    pub fn new(config: Config) -> Result<Self> {
        let db_path = config.resolve_data_path("db/sessionweave.db");
        let store = SqliteStore::new(&db_path)?;
        Ok(Self { config, store })
    }

    /// Index everything in configured sources
    pub async fn index_all(&self, force: bool, dry_run: bool) -> Result<IndexStats> {
        let mut total = IndexStats::default();

        // Obtain optional vector components once for the whole run.
        let ollama = Self::build_ollama(&self.config);
        let lance = Self::build_lance(&self.config).await;
        if let Some(ref l) = lance {
            let _ = l.ensure_message_table().await;
            let _ = l.ensure_session_table().await;
        }
        if lance.is_some() {
            println!("   (LanceDB vector backend initialized; embeddings will be added if Ollama reachable)");
        }

        for source in &self.config.sources {
            let stats = self.index_source(source, force, dry_run, &ollama, &lance).await?;
            total.sessions_indexed += stats.sessions_indexed;
            total.messages_indexed += stats.messages_indexed;
            total.files_scanned += stats.files_scanned;
            total.embeddings_added += stats.embeddings_added;
        }
        Ok(total)
    }

    pub async fn index_path(&self, path: &Path, force: bool, dry_run: bool) -> Result<IndexStats> {
        let inferred_type = infer_source_type(path);
        let source = Source {
            path: path.to_path_buf(),
            source_type: inferred_type,
            include: None,
            recursive: path.is_dir(),
        };
        let ollama = Self::build_ollama(&self.config);
        let lance = Self::build_lance(&self.config).await;
        if let Some(ref l) = lance {
            let _ = l.ensure_message_table().await;
            let _ = l.ensure_session_table().await;
        }
        if lance.is_some() {
            println!("   (LanceDB vector backend initialized; embeddings will be added if Ollama reachable)");
        }
        self.index_source(&source, force, dry_run, &ollama, &lance).await
    }

    fn build_ollama(config: &Config) -> Option<OllamaClient> {
        if !config.ollama.enabled {
            return None;
        }
        Some(OllamaClient::new(
            &config.ollama.url,
            &config.ollama.embedding_model,
            &config.ollama.chat_model,
        ))
    }

    async fn build_lance(config: &Config) -> Option<LanceStore> {
        let path = config.resolve_data_path("vectors");
        LanceStore::new(&path).await.ok()
    }
}

fn infer_source_type(path: &Path) -> String {
    let p = path.to_string_lossy().to_lowercase();
    if p.contains(".claude") || p.contains("claude") {
        "claude".to_string()
    } else if p.contains("cursor") || p.contains("state.vscdb") {
        "cursor".to_string()
    } else if p.contains("windsurf") || p.contains("codeium") {
        "windsurf".to_string()
    } else if p.contains("codex") {
        "codex".to_string()
    } else if path.extension().map_or(false, |e| e == "jsonl") {
        // Default .jsonl files coming from Claude Code style
        "claude".to_string()
    } else {
        "generic".to_string()
    }
}

impl Indexer {
    async fn index_source(
        &self,
        source: &Source,
        force: bool,
        dry_run: bool,
        ollama: &Option<OllamaClient>,
        lance: &Option<LanceStore>,
    ) -> Result<IndexStats> {
        let expanded = crate::config::expand_tilde(&source.path.to_string_lossy());
        println!("→ Scanning {} ({})", expanded.display(), source.source_type);

        let mut stats = IndexStats::default();

        let walker = WalkDir::new(&expanded)
            .max_depth(if source.recursive { 100 } else { 1 })
            .into_iter()
            .filter_entry(|e| !is_hidden(e));

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            stats.files_scanned += 1;

            let path = entry.path();
            if let Some(parsed) = self.try_parse(path, &source.source_type).await? {
                let msg_count = parsed.messages.len();
                if dry_run {
                    println!("  [dry] would index {} turns from {}", msg_count, path.display());
                    continue;
                }
                if self.persist_parsed(parsed, force, ollama, lance).await? {
                    stats.sessions_indexed += 1;
                    stats.messages_indexed += msg_count;
                    if lance.is_some() {
                        stats.embeddings_added += msg_count; // approximate; actual adds printed inline
                    }
                } else {
                    stats.skipped += 1;
                }
            } else {
                stats.skipped += 1;
            }
        }

        println!("   Indexed {} sessions, {} messages", stats.sessions_indexed, stats.messages_indexed);
        Ok(stats)
    }

    async fn try_parse(&self, path: &Path, source_type: &str) -> Result<Option<ParserResult>> {
        let path_str = path.to_string_lossy();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Strong detection for Claude regardless of declared source_type (very common)
        if ext == "jsonl" && (source_type == "claude" || path_str.contains("claude") || path_str.contains("chat")) {
            let res = parse_claude_jsonl(path).await?;
            return Ok(Some(res));
        }

        match source_type {
            "claude" => {
                if ext == "jsonl" {
                    let res = parse_claude_jsonl(path).await?;
                    Ok(Some(res))
                } else {
                    Ok(None)
                }
            }
            "cursor" if path.file_name().map_or(false, |n| n == "state.vscdb") => {
                Ok(None) // TODO: implement Cursor SQLite parsing
            }
            "generic" | "windsurf" | "codex" | _ => {
                if matches!(ext, "json" | "jsonl" | "md" | "txt") {
                    let res = parse_generic(path).await?;
                    if res.messages.is_empty() { Ok(None) } else { Ok(Some(res)) }
                } else {
                    Ok(None)
                }
            }
        }
    }

    async fn persist_parsed(
        &self,
        result: ParserResult,
        force: bool,
        ollama: &Option<OllamaClient>,
        lance: &Option<LanceStore>,
    ) -> Result<bool> {
        // Compute content hash from the raw file for idempotency (proper dedup)
        let raw = std::fs::read_to_string(&result.source).unwrap_or_default();
        let session_content_hash = content_hash(&raw);

        // Proper dedup: skip if same source_path + content_hash already present (unless --force)
        if !force {
            if let Ok(Some(existing)) = self.store.get_session(&result.session_id) {
                if existing.source_path == result.source
                    && existing.content_hash.as_deref() == Some(session_content_hash.as_str())
                    && existing.message_count > 0
                {
                    return Ok(false);
                }
            }
            // Also skip if exact content hash already seen anywhere (truly duplicate content)
            // (simple heuristic: query for matching hash; for MVP we can just rely on the session check above for speed)
        }

        let session_id = result.session_id.clone();

        let mut session = Session::new(
            session_id.clone(),
            &result.harness,
            result.source.clone(),
        );
        session.message_count = result.messages.len();
        session.title = result.title.clone();
        session.files_touched = result.files_touched.clone();
        session.content_hash = Some(session_content_hash.clone());

        // Take ownership of parsed messages for enrichment (LLM + rules)
        let mut msgs: Vec<crate::models::Message> = result.messages;

        // 1. Always run cheap rule-based extraction on turns. Set content_hash for dedup.
        for (idx, msg) in msgs.iter_mut().enumerate() {
            msg.turn_index = idx as i32;
            if msg.content_hash.is_none() {
                msg.content_hash = Some(content_hash(&msg.content));
            }
            extract_from_message(msg);
        }

        // 2. LLM-powered extraction (batched, at most ~2 calls per session, only if Ollama option present).
        //    - 1 call for whole-session summary/tags/decisions (preferred for cost/speed).
        //    - Up to 1 additional call on a single "important" turn (assistant messages with substance).
        if let Some(o) = ollama.as_ref() {
            // Session-level (always do this one for the session)
            let agg_summary: String = msgs
                .iter()
                .map(|m| format!("[{} turn {}]: {}", m.role, m.turn_index, m.preview(220)))
                .collect::<Vec<_>>()
                .join("\n");
            let session_insights = extract_for_session(
                o,
                session.title.as_deref(),
                &agg_summary,
                &session.files_touched,
            )
            .await;

            if let Some(s) = session_insights.summary {
                if session.summary.is_none() || session.summary.as_ref().map_or(true, |cur| cur.len() < s.len()) {
                    session.summary = Some(s);
                }
            }

            // Merge any session decisions into a representative message (and will store in decisions table below)
            if !session_insights.decisions.is_empty() {
                // attach to last message or first that is assistant (no double mutable borrow)
                let target_idx = msgs
                    .iter()
                    .rev()
                    .position(|m| m.role == Role::Assistant)
                    .map(|p| msgs.len() - 1 - p)
                    .or_else(|| if !msgs.is_empty() { Some(msgs.len() - 1) } else { None });
                if let Some(idx) = target_idx {
                    let target = &mut msgs[idx];
                    for d in &session_insights.decisions {
                        if !target.decisions.contains(d) {
                            target.decisions.push(d.clone());
                        }
                    }
                }
            }

            // Store tags early (idempotent)
            for t in &session_insights.tags {
                let _ = self.store.tag_session(&session_id, t);
            }

            // Optional 2nd LLM call: extract on one important turn (only if substantial session)
            if msgs.len() >= 2 {
                if let Some(imp_msg) = msgs.iter().rev().find(|m| {
                    m.role == Role::Assistant && m.content.len() > 120
                }) {
                    let turn_ins = extract_from_turn(o, &imp_msg.content).await;
                    let imp_id = imp_msg.id.clone();
                    // merge its output into that msg (need mutable ref - re-find)
                    if let Some(target) = msgs.iter_mut().find(|m| m.id == imp_id) {
                        for d in turn_ins.decisions {
                            if !target.decisions.contains(&d) {
                                target.decisions.push(d);
                            }
                        }
                        for t in turn_ins.tags {
                            let _ = self.store.tag_session(&session_id, &t);
                        }
                    }
                }
            }

            // Store session decisions into dedicated table (including any per-turn ones already merged)
            for d in &session_insights.decisions {
                let _ = self.store.insert_decision(&session_id, None, d);
            }
        }

        // Persist session (now may have summary) + messages (now enriched with decisions)
        self.store.upsert_session(&session)?;

        for msg in &msgs {
            self.store.insert_message(msg)?;
        }

        // Store per-message decisions to the decisions table too (for those populated by rules/LLM)
        for msg in &msgs {
            for d in &msg.decisions {
                let _ = self.store.insert_decision(&session_id, Some(&msg.id), d);
            }
        }

        // --- LanceDB embeddings (per-message + session summary) ---
        // Only happens if we were passed live Ollama + Lance (checked at higher level).
        // We refetch after writes for simplicity. Any failures here are non-fatal.
        if let (Some(o), Some(l)) = (ollama.as_ref(), lance.as_ref()) {
            let _ = l.ensure_message_table().await;

            if let Ok(msgs) = self.store.get_messages_for_session(&session_id) {
                let msg_count_for_emb = msgs.len();
                let mut emb_batch: Vec<(String, Vec<f32>, String, String, String)> = vec![];
                for m in msgs {
                    if let Ok(emb) = o.embed(&m.content).await {
                        if emb.len() == crate::db::lancedb_store::EMBED_DIM as usize {
                            emb_batch.push((
                                m.id.clone(),
                                emb,
                                m.session_id.clone(),
                                m.role.to_string(),
                                m.preview(280),
                            ));
                        }
                    }
                }
                if !emb_batch.is_empty() {
                    if l.add_message_embeddings(emb_batch.clone()).await.is_ok() {
                        for (mid, _, _, _, _) in &emb_batch {
                            let _ = self.store.mark_message_embedding(mid);
                        }
                        println!("     + embeddings added for {} messages (Ollama + LanceDB)", msg_count_for_emb);
                    }
                }
            }

            // Session-level summary embedding
            if let Ok(Some(sess)) = self.store.get_session(&session_id) {
                let mut summary_text = sess
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Session {}", &session_id));
                if !sess.files_touched.is_empty() {
                    summary_text = format!("{} | files: {}", summary_text, sess.files_touched.join(", "));
                }
                let _ = l.ensure_session_table().await;
                if let Ok(semb) = o.embed(&summary_text).await {
                    if semb.len() == crate::db::lancedb_store::EMBED_DIM as usize {
                        if l.add_session_embedding(session_id.clone(), semb, summary_text, sess.harness.clone()).await.is_ok() {
                            println!("     + session embedding added");
                        }
                    }
                }
            }
        }

        Ok(true)
    }
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}
