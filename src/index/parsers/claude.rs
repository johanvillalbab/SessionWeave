//! Claude Code parser (.jsonl format).
//!
//! Each line is a JSON record with "type", "message", "timestamp", etc.
//! We extract user/assistant turns + tool activity (files touched).

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::Path;

use crate::index::parsers::ParserResult;
use crate::models::{Message, Role};

pub async fn parse_claude_jsonl(path: &Path) -> Result<ParserResult> {
    let content = std::fs::read_to_string(path)?;
    let mut messages: Vec<Message> = vec![];
    let mut files: Vec<String> = vec![];
    let mut turn = 0i32;
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("claude-unknown")
        .to_string();

    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let ts_str = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
        let timestamp = DateTime::parse_from_rfc3339(ts_str)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        // Extract message content
        let (role, text) = match typ {
            "user" => {
                let role = Role::User;
                let txt = extract_user_text(&v);
                (role, txt)
            }
            "assistant" => {
                let role = Role::Assistant;
                let txt = extract_assistant_text(&v);
                (role, txt)
            }
            _ => continue,
        };

        if text.trim().is_empty() {
            continue;
        }

        // Collect file mentions using regex (very useful signal)
        extract_file_paths(&text, &mut files);

        // Also detect tool file activity
        if let Some(tool_input) = extract_tool_file_paths(&v) {
            for f in tool_input {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
        }

        let mut msg = Message::new(session_id.clone(), role, text, timestamp, turn);
        msg.files = files.clone(); // snapshot for this turn
        messages.push(msg);
        turn += 1;
    }

    let title = messages
        .first()
        .map(|m| m.content.chars().take(80).collect::<String>())
        .map(|s| s.trim_end_matches(|c| c == '.' || c == ' ' || c == '\n').to_string());

    Ok(ParserResult {
        session_id,
        harness: "claude".to_string(),
        source: path.to_path_buf(),
        title,
        messages,
        files_touched: files,
    })
}

fn extract_user_text(v: &Value) -> String {
    if let Some(msg) = v.get("message") {
        if let Some(content) = msg.get("content") {
            if let Some(s) = content.as_str() {
                return s.to_string();
            }
            if let Some(arr) = content.as_array() {
                return arr
                    .iter()
                    .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
    }
    // Fallbacks for different shapes
    v.get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_assistant_text(v: &Value) -> String {
    // assistant messages often have content as array of blocks
    if let Some(msg) = v.get("message") {
        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            let mut out = String::new();
            for block in content {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    out.push_str(t);
                    out.push('\n');
                }
                // Tool use blocks are captured elsewhere
            }
            return out.trim().to_string();
        }
    }
    v.get("content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string()
}

pub fn extract_file_paths(text: &str, out: &mut Vec<String>) {
    // Simple but effective regex for common file paths in code discussions
    let re = regex::Regex::new(r#"(?i)([\w./-]+\.(rs|ts|tsx|js|jsx|py|go|java|rb|php|swift|kt|scala|sql|md|toml|json|yaml|yml|sh|bash|html|css|scss))"#).unwrap();
    for cap in re.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let p = m.as_str().to_string();
            if !out.contains(&p) && p.len() > 3 {
                out.push(p);
            }
        }
    }
}

fn extract_tool_file_paths(v: &Value) -> Option<Vec<String>> {
    let mut files = vec![];
    // Look for tool_use blocks inside message.content
    if let Some(msg) = v.get("message") {
        if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
            for item in arr {
                if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    if let Some(input) = item.get("input") {
                        // Common Claude Code tools: Read, Write, Edit, MultiEdit, etc.
                        if let Some(path) = input.get("file_path").and_then(|p| p.as_str()) {
                            files.push(path.to_string());
                        }
                        if let Some(paths) = input.get("file_paths").and_then(|p| p.as_array()) {
                            for p in paths {
                                if let Some(s) = p.as_str() {
                                    files.push(s.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if files.is_empty() {
        None
    } else {
        Some(files)
    }
}
