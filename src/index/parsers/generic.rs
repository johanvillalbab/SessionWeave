//! Generic parser for Markdown chat dumps and simple JSON/JSONL exports.
//! Looks for common patterns:
//!   ## User / ## Assistant
//!   **User:** / **Assistant:**
//!   Or raw array of {"role": "...", "content": "..."}

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use std::path::Path;

use crate::index::parsers::ParserResult;
use crate::models::{Message, Role};

pub async fn parse_generic(path: &Path) -> Result<ParserResult> {
    let content = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let mut messages = vec![];
    let session_id = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("generic")
        .to_string();

    if ext == "json" || ext == "jsonl" {
        // Try JSON array or JSONL of messages
        if let Ok(val) = serde_json::from_str::<Value>(&content) {
            if let Some(arr) = val.as_array() {
                for (i, item) in arr.iter().enumerate() {
                    if let Some(m) = parse_json_message(item, i as i32, &session_id) {
                        messages.push(m);
                    }
                }
            }
        } else {
            // JSONL
            for (i, line) in content.lines().enumerate() {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    if let Some(m) = parse_json_message(&v, i as i32, &session_id) {
                        messages.push(m);
                    }
                }
            }
        }
    } else {
        // Markdown style
        messages = parse_markdown_chat(&content, &session_id);
    }

    let files = messages.iter().flat_map(|m| m.files.clone()).collect();

    let title = messages.first().map(|m| m.preview(70));

    Ok(ParserResult {
        session_id,
        harness: "generic".into(),
        source: path.to_path_buf(),
        title,
        messages,
        files_touched: files,
    })
}

fn parse_json_message(v: &Value, idx: i32, session_id: &str) -> Option<Message> {
    let role_str = v.get("role").and_then(|r| r.as_str()).unwrap_or("user");
    let role = match role_str {
        "assistant" => Role::Assistant,
        "system" => Role::System,
        _ => Role::User,
    };
    let content = v
        .get("content")
        .and_then(|c| c.as_str())
        .or_else(|| v.get("text").and_then(|t| t.as_str()))
        .unwrap_or("")
        .to_string();

    if content.trim().is_empty() {
        return None;
    }

    Some(Message::new(
        session_id.to_string(),
        role,
        content,
        Utc::now(),
        idx,
    ))
}

fn parse_markdown_chat(text: &str, session_id: &str) -> Vec<Message> {
    let mut messages = vec![];
    let mut current_role = Role::User;
    let mut current_buf = String::new();
    let mut idx = 0i32;

    for line in text.lines() {
        let lower = line.to_lowercase();
        let is_user = lower.starts_with("## user")
            || lower.starts_with("**user:**")
            || lower.starts_with("user:");
        let is_assistant = lower.starts_with("## assistant")
            || lower.starts_with("## claude")
            || lower.starts_with("**assistant:**")
            || lower.starts_with("assistant:");

        if is_user || is_assistant {
            // flush previous
            if !current_buf.trim().is_empty() {
                let mut msg = Message::new(
                    session_id.to_string(),
                    current_role.clone(),
                    current_buf.trim().to_string(),
                    Utc::now(),
                    idx,
                );
                // crude file extraction
                crate::index::parsers::claude::extract_file_paths(&msg.content, &mut msg.files); // reuse helper
                messages.push(msg);
                idx += 1;
            }
            current_buf.clear();
            current_role = if is_user { Role::User } else { Role::Assistant };
            continue;
        }

        current_buf.push_str(line);
        current_buf.push('\n');
    }

    // last message
    if !current_buf.trim().is_empty() {
        let mut msg = Message::new(
            session_id.to_string(),
            current_role,
            current_buf.trim().to_string(),
            Utc::now(),
            idx,
        );
        crate::index::parsers::claude::extract_file_paths(&msg.content, &mut msg.files);
        messages.push(msg);
    }

    messages
}
