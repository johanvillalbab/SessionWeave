//! Parser registry and implementations.
//! Each parser turns raw files into normalized Session + Message data.

mod claude;
mod generic;

pub use claude::{extract_file_paths, parse_claude_jsonl};
pub use generic::parse_generic;

use crate::models::Message;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ParserResult {
    pub session_id: String,
    pub harness: String,
    pub source: PathBuf,
    pub title: Option<String>,
    pub messages: Vec<Message>,
    pub files_touched: Vec<String>,
}

pub trait SessionParser: Send + Sync {
    fn can_parse(&self, path: &Path) -> bool;
    fn parse(&self, path: &Path) -> anyhow::Result<ParserResult>;
}
