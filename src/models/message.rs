//! Individual message / turn within a session.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::System => write!(f, "system"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub turn_index: i32,
    /// Optional reference to embedding vector id in LanceDB
    pub embedding_id: Option<String>,
    /// Files mentioned or touched in this specific turn
    #[serde(default)]
    pub files: Vec<String>,
    /// Extracted key decisions (populated during indexing with LLM or rules)
    #[serde(default)]
    pub decisions: Vec<String>,
    /// SHA256(content) for deduplication of identical message bodies
    #[serde(default)]
    pub content_hash: Option<String>,
}

impl Message {
    pub fn new(
        session_id: String,
        role: Role,
        content: String,
        timestamp: DateTime<Utc>,
        turn_index: i32,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            role,
            content,
            timestamp,
            turn_index,
            embedding_id: None,
            files: vec![],
            decisions: vec![],
            content_hash: None,
        }
    }

    pub fn preview(&self, max: usize) -> String {
        let s: String = self.content.chars().take(max).collect();
        if self.content.len() > max {
            format!("{}...", s)
        } else {
            s
        }
    }
}
