//! SQLite store with FTS5 support.
//! Stores sessions, messages, artifacts, decisions and provides full-text search.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

use crate::models::{Artifact, Message, Role, Session};

pub struct SqliteStore {
    conn: Connection,
}

/// Aggregated database statistics returned by `stats()`.
#[derive(Debug, Clone, Default)]
pub struct DbStats {
    pub sessions: i64,
    pub messages: i64,
    pub decisions: i64,
    pub tags: i64,
    pub tagged_sessions: i64,
    pub messages_with_decisions: i64,
    pub messages_with_vectors: i64,
    pub sessions_with_content_hash: i64,
}

impl SqliteStore {
    pub fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open SQLite at {:?}", db_path))?;

        // Performance & integrity pragmas
        conn.pragma_update(None, "journal_mode", &"WAL")?;
        conn.pragma_update(None, "synchronous", &"NORMAL")?;
        conn.pragma_update(None, "foreign_keys", &"ON")?;

        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        // Core tables
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                root_path TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                source_path TEXT NOT NULL,
                harness TEXT NOT NULL,
                session_key TEXT,
                title TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                summary TEXT,
                message_count INTEGER DEFAULT 0,
                files_touched TEXT,           -- JSON array
                content_hash TEXT,
                last_indexed TEXT,
                FOREIGN KEY(project_id) REFERENCES projects(id)
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                turn_index INTEGER NOT NULL,
                embedding_id TEXT,
                files TEXT,                   -- JSON
                decisions TEXT,               -- JSON
                content_hash TEXT,            -- for dedup of duplicate content
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                message_id TEXT,
                file_path TEXT NOT NULL,
                action TEXT NOT NULL,
                snippet TEXT,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS decisions (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                message_id TEXT,
                text TEXT NOT NULL,
                confidence REAL DEFAULT 0.8,
                tags TEXT,                    -- JSON
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                kind TEXT,
                description TEXT
            );

            CREATE TABLE IF NOT EXISTS session_tags (
                session_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                PRIMARY KEY (session_id, tag_id),
                FOREIGN KEY(session_id) REFERENCES sessions(id),
                FOREIGN KEY(tag_id) REFERENCES tags(id)
            );

            CREATE TABLE IF NOT EXISTS relations (
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                strength REAL DEFAULT 0.5,
                PRIMARY KEY (from_id, to_id, relation_type)
            );
            "#,
        )?;

        // FTS5 virtual tables for fast text search
        self.conn.execute_batch(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                content,
                session_id UNINDEXED,
                role UNINDEXED,
                timestamp UNINDEXED,
                content='messages',
                content_rowid='rowid'
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
                title,
                summary,
                harness,
                content='sessions',
                content_rowid='rowid'
            );
            "#,
        )?;

        // Triggers to keep FTS in sync (simple version)
        self.conn.execute_batch(
            r#"
            CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
                INSERT INTO messages_fts(rowid, content, session_id, role, timestamp)
                VALUES (new.rowid, new.content, new.session_id, new.role, new.timestamp);
            END;

            CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
                INSERT INTO messages_fts(messages_fts, rowid, content, session_id, role, timestamp)
                VALUES ('delete', old.rowid, old.content, old.session_id, old.role, old.timestamp);
            END;

            CREATE TRIGGER IF NOT EXISTS sessions_ai AFTER INSERT ON sessions BEGIN
                INSERT INTO sessions_fts(rowid, title, summary, harness)
                VALUES (new.rowid, IFNULL(new.title, ''), IFNULL(new.summary, ''), new.harness);
            END;
            "#,
        )?;

        Ok(())
    }

    // ---------------------- Session ops ----------------------

    pub fn upsert_session(&self, session: &Session) -> Result<()> {
        let files_json = serde_json::to_string(&session.files_touched)?;
        let started = session.started_at.to_rfc3339();
        let ended = session.ended_at.map(|e| e.to_rfc3339());

        self.conn.execute(
            r#"
            INSERT INTO sessions (id, project_id, source_path, harness, session_key, title,
                                  started_at, ended_at, summary, message_count, files_touched, content_hash, last_indexed)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                ended_at = excluded.ended_at,
                summary = excluded.summary,
                message_count = excluded.message_count,
                files_touched = excluded.files_touched,
                content_hash = COALESCE(excluded.content_hash, content_hash),
                last_indexed = excluded.last_indexed
            "#,
            params![
                session.id,
                session.project_id,
                session.source_path.to_string_lossy().to_string(),
                session.harness,
                session.session_key,
                session.title,
                started,
                ended,
                session.summary,
                session.message_count as i64,
                files_json,
                session.content_hash,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, source_path, harness, session_key, title, started_at, ended_at, summary, message_count, files_touched, content_hash
             FROM sessions WHERE id = ?1"
        )?;

        let session = stmt.query_row(params![id], |row| {
            let files: String = row.get(10)?;
            let files_touched: Vec<String> = serde_json::from_str(&files).unwrap_or_default();
            let content_hash: Option<String> = row.get(11).ok();

            Ok(Session {
                id: row.get(0)?,
                project_id: row.get(1)?,
                source_path: Path::new(&row.get::<_, String>(2)?).to_path_buf(),
                harness: row.get(3)?,
                session_key: row.get(4)?,
                title: row.get(5)?,
                started_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?).unwrap().with_timezone(&Utc),
                ended_at: row.get::<_, Option<String>>(7)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                summary: row.get(8)?,
                message_count: row.get::<_, i64>(9)? as usize,
                files_touched,
                content_hash,
            })
        }).optional()?;

        Ok(session)
    }

    // ---------------------- Message ops ----------------------

    pub fn insert_message(&self, msg: &Message) -> Result<()> {
        let files = serde_json::to_string(&msg.files)?;
        let decisions = serde_json::to_string(&msg.decisions)?;

        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO messages
            (id, session_id, role, content, timestamp, turn_index, embedding_id, files, decisions, content_hash)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                msg.id,
                msg.session_id,
                msg.role.to_string(),
                msg.content,
                msg.timestamp.to_rfc3339(),
                msg.turn_index,
                msg.embedding_id,
                files,
                decisions,
                msg.content_hash,
            ],
        )?;
        Ok(())
    }

    pub fn get_messages_for_session(&self, session_id: &str) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, timestamp, turn_index, embedding_id, files, decisions, content_hash
             FROM messages WHERE session_id = ?1 ORDER BY turn_index ASC"
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            let role_str: String = row.get(2)?;
            let role = match role_str.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "system" => Role::System,
                _ => Role::Tool,
            };

            let files: String = row.get(7).unwrap_or_default();
            let decisions: String = row.get(8).unwrap_or_default();
            let content_hash: Option<String> = row.get(9).ok();

            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?).unwrap().with_timezone(&Utc),
                turn_index: row.get(5)?,
                embedding_id: row.get(6)?,
                files: serde_json::from_str(&files).unwrap_or_default(),
                decisions: serde_json::from_str(&decisions).unwrap_or_default(),
                content_hash,
            })
        })?;

        let mut out = vec![];
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ---------------------- FTS Search ----------------------

    /// Lookup a single message by its id (used for vector-hit backfill in hybrid search).
    pub fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, timestamp, turn_index, embedding_id, files, decisions, content_hash
             FROM messages WHERE id = ?1"
        )?;

        let msg = stmt
            .query_row(params![id], |row| {
                let role_str: String = row.get(2)?;
                let role = match role_str.as_str() {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "system" => Role::System,
                    _ => Role::Tool,
                };
                let files: String = row.get(7).unwrap_or_default();
                let decisions: String = row.get(8).unwrap_or_default();
                let content_hash: Option<String> = row.get(9).ok();

                Ok(Message {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role,
                    content: row.get(3)?,
                    timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    turn_index: row.get(5)?,
                    embedding_id: row.get(6)?,
                    files: serde_json::from_str(&files).unwrap_or_default(),
                    decisions: serde_json::from_str(&decisions).unwrap_or_default(),
                    content_hash,
                })
            })
            .optional()?;
        Ok(msg)
    }

    pub fn fts_search_messages(&self, query: &str, limit: usize) -> Result<Vec<Message>> {
        // Escape simple FTS query (basic)
        let q = query.replace('"', "\"\"");
        let sql = format!(
            "SELECT m.id, m.session_id, m.role, m.content, m.timestamp, m.turn_index, m.embedding_id, m.files, m.decisions
             FROM messages_fts f
             JOIN messages m ON m.rowid = f.rowid
             WHERE f.content MATCH ?
             ORDER BY rank
             LIMIT {}",
            limit
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![q], |row| {
            // same deserialization as above
            let role_str: String = row.get(2)?;
            let role = match role_str.as_str() {
                "user" => Role::User, "assistant" => Role::Assistant, "system" => Role::System, _ => Role::Tool,
            };
            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?).unwrap().with_timezone(&Utc),
                turn_index: row.get(5)?,
                embedding_id: row.get(6)?,
                files: vec![],
                decisions: vec![],
                content_hash: None,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_artifact(&self, art: &Artifact) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO artifacts (id, session_id, message_id, file_path, action, snippet) VALUES (?1,?2,?3,?4,?5,?6)",
            params![art.id, art.session_id, art.message_id, art.file_path, art.action, art.snippet],
        )?;
        Ok(())
    }

    /// Rich stats for `sw stats` command.
    pub fn stats(&self) -> Result<DbStats> {
        let sessions: i64 = self.conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        let messages: i64 = self.conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
        let decisions: i64 = self.conn.query_row("SELECT COUNT(*) FROM decisions", [], |r| r.get(0)).unwrap_or(0);
        let tags: i64 = self.conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0)).unwrap_or(0);
        let tagged_sessions: i64 = self.conn.query_row("SELECT COUNT(DISTINCT session_id) FROM session_tags", [], |r| r.get(0)).unwrap_or(0);
        let messages_with_decisions: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE decisions IS NOT NULL AND decisions != '[]' AND decisions != ''",
            [], |r| r.get(0)
        ).unwrap_or(0);
        let vectors_populated: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE embedding_id IS NOT NULL AND embedding_id != ''",
            [], |r| r.get(0)
        ).unwrap_or(0);
        let sessions_with_hash: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE content_hash IS NOT NULL AND content_hash != ''",
            [], |r| r.get(0)
        ).unwrap_or(0);

        Ok(DbStats {
            sessions,
            messages,
            decisions,
            tags,
            tagged_sessions,
            messages_with_decisions,
            messages_with_vectors: vectors_populated,
            sessions_with_content_hash: sessions_with_hash,
        })
    }

    // ---------------------- Decisions & Tags (LLM extracted) ----------------------

    /// Insert a decision (from LLM extraction) into the dedicated decisions table.
    /// message_id optional (null for session-level decisions).
    pub fn insert_decision(&self, session_id: &str, message_id: Option<&str>, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        self.conn.execute(
            r#"INSERT OR IGNORE INTO decisions (id, session_id, message_id, text, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5)"#,
            params![id, session_id, message_id, text.trim(), created_at],
        )?;
        Ok(())
    }

    /// Return all decision texts for a session (newest-ish via insert order).
    pub fn get_decisions_for_session(&self, session_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT text FROM decisions WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 20"
        )?;
        let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<String>, _>>().map_err(Into::into)
    }

    /// Ensure a tag exists (id derived from normalized name). Returns the tag id.
    pub fn ensure_tag(&self, name: &str) -> Result<String> {
        let norm = name.trim().to_lowercase();
        if norm.is_empty() || norm.len() < 2 {
            return Ok(String::new());
        }
        let id: String = norm
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        if id.is_empty() {
            return Ok(String::new());
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO tags (id, name, kind) VALUES (?1, ?2, 'feature')",
            params![&id, &norm],
        )?;
        Ok(id)
    }

    /// Associate a tag with a session (creates tag if needed).
    pub fn tag_session(&self, session_id: &str, tag_name: &str) -> Result<()> {
        let tag_id = self.ensure_tag(tag_name)?;
        if tag_id.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO session_tags (session_id, tag_id) VALUES (?1, ?2)",
            params![session_id, &tag_id],
        )?;
        Ok(())
    }

    /// Get tags for a session.
    pub fn get_tags_for_session(&self, session_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT t.name FROM session_tags st
               JOIN tags t ON t.id = st.tag_id
               WHERE st.session_id = ?1
               ORDER BY t.name"#,
        )?;
        let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<String>, _>>().map_err(Into::into)
    }

    /// Mark that this message has a corresponding vector in LanceDB.
    pub fn mark_message_embedding(&self, message_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE messages SET embedding_id = ?1 WHERE id = ?1",
            params![message_id],
        )?;
        Ok(())
    }
}
