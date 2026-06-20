//! Artifacts: files read, written or mentioned in a session.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub session_id: String,
    pub message_id: Option<String>,
    pub file_path: String,
    /// read | write | edit | mention
    pub action: String,
    /// Optional short snippet or diff head
    pub snippet: Option<String>,
}

impl Artifact {
    pub fn new(session_id: String, file_path: String, action: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            message_id: None,
            file_path,
            action: action.to_string(),
            snippet: None,
        }
    }
}
