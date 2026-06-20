//! Intelligent extraction using rules + Ollama LLM.
//! Extracts key decisions, feature tags, short session summaries.
//!
//! All LLM calls are optional/graceful. If Ollama disabled or unreachable,
//! functions return empty/defaults. Calls are batched (1-2 per session in indexer).

use crate::models::Message;
use crate::ollama::OllamaClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExtractedInsights {
    /// Key decisions extracted (e.g. "switched to using sqlx for DB layer")
    pub decisions: Vec<String>,
    /// Feature/component tags (e.g. "auth", "api", "database")
    pub tags: Vec<String>,
    /// Short 1-sentence summary
    pub summary: Option<String>,
}

/// Rule-based extraction (no LLM). Always runs. Populates decisions in Message.
pub fn extract_from_message(msg: &mut Message) {
    // Heuristics for common decision language in AI coding chats.
    let lower = msg.content.to_lowercase();
    if (lower.contains("decided") || lower.contains("we will use") || lower.contains("switch to")
        || lower.contains("going with") || lower.contains("chose ")) && msg.decisions.is_empty()
    {
        msg.decisions.push(msg.preview(160));
    }
}

/// Extract structured insights from a single important turn/message using LLM.
/// Call sparingly (e.g. 0-1 times per session).
pub async fn extract_from_turn(ollama: &OllamaClient, content: &str) -> ExtractedInsights {
    if content.trim().len() < 30 {
        return ExtractedInsights::default();
    }

    let prompt = format!(
        r#"You are an expert software engineering context extractor.

Analyze this turn from an AI coding conversation.

Extract:
1. decisions: explicit decisions, choices, "we decided", "use X", "switched to", "going with" (max 3 short strings).
2. tags: 2-5 concise feature or component tags (e.g. "auth", "jwt", "database", "api", "ui").
3. summary: one short sentence of what was accomplished in the turn.

Respond ONLY with compact minified JSON (no markdown, no extra text):
{{"decisions": ["..."], "tags": ["..."], "summary": "..." }}

Conversation turn:
{content}"#,
        content = content
    );

    let schema_hint = r#"{"decisions": ["string"], "tags": ["string"], "summary": "string"}"#;

    match ollama.extract_structured(&prompt, schema_hint).await {
        Ok(json_str) => {
            let cleaned = json_str.trim().trim_matches('`').trim();
            if let Ok(mut ins) = serde_json::from_str::<ExtractedInsights>(cleaned) {
                // sanitize
                ins.decisions.retain(|d| d.len() > 3 && d.len() < 200);
                ins.tags.retain(|t| t.len() > 1 && t.len() < 30);
                ins.tags.truncate(6);
                ins.decisions.truncate(4);
                return ins;
            }
            // lenient fallback
            ExtractedInsights {
                decisions: vec![],
                tags: extract_simple_tags(cleaned),
                summary: Some(cleaned.chars().take(140).collect()),
            }
        }
        Err(_) => ExtractedInsights::default(),
    }
}

/// Extract high-level insights for the entire session (preferred for MVP: 1 call).
/// Aggregates messages summary + files.
pub async fn extract_for_session(
    ollama: &OllamaClient,
    title: Option<&str>,
    messages_summary: &str,
    files: &[String],
) -> ExtractedInsights {
    let files_str = if files.is_empty() {
        "none".to_string()
    } else {
        files.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
    };

    let prompt = format!(
        r#"Extract high-level structured insights from this AI coding session transcript.

Title: {title}
Files touched: {files}

Key conversation content / turns:
{messages}

Return ONLY compact minified JSON:
{{"decisions": ["key architectural or implementation decisions"], "tags": ["feature tags"], "summary": "one sentence overview of the session goal and outcome"}}"#,
        title = title.unwrap_or("Untitled session"),
        files = files_str,
        messages = messages_summary.chars().take(2200).collect::<String>()
    );

    let schema = r#"{"decisions":["string"],"tags":["string"],"summary":"string"}"#;

    if let Ok(text) = ollama.extract_structured(&prompt, schema).await {
        let cleaned = text.trim().trim_matches('`').trim();
        if let Ok(mut parsed) = serde_json::from_str::<ExtractedInsights>(cleaned) {
            parsed.decisions.retain(|d| !d.is_empty());
            parsed.tags.retain(|t| !t.is_empty());
            parsed.decisions.truncate(5);
            parsed.tags.truncate(8);
            return parsed;
        }
    }

    ExtractedInsights::default()
}

fn extract_simple_tags(text: &str) -> Vec<String> {
    let mut tags = vec![];
    for w in text.split(|c: char| !c.is_alphanumeric() && c != '-') {
        let w = w.to_lowercase();
        if w.len() > 2 && w.len() < 25 && !tags.contains(&w) {
            tags.push(w);
            if tags.len() >= 5 {
                break;
            }
        }
    }
    tags
}
