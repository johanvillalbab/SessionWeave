//! Session model: represents one conversation thread from any harness.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub id: String,                    // uuid or stable hash
    pub project_id: Option<String>,
    pub source_path: PathBuf,          // original file or db location
    pub harness: String,               // "claude" | "cursor" | "windsurf" | "codex" | "generic"
    pub session_key: Option<String>,   // harness-native id if any
    pub title: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub message_count: usize,
    pub files_touched: Vec<String>,    // de-duplicated
    /// SHA256 of core session content (for true dedup beyond source path)
    pub content_hash: Option<String>,
}

impl Session {
    pub fn new(id: String, harness: &str, source: PathBuf) -> Self {
        Self {
            id,
            project_id: None,
            source_path: source,
            harness: harness.to_string(),
            session_key: None,
            title: None,
            started_at: Utc::now(),
            ended_at: None,
            summary: None,
            message_count: 0,
            files_touched: vec![],
            content_hash: None,
        }
    }
}
