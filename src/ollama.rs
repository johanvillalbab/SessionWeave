//! Ollama integration.
//! - Embeddings via /api/embeddings (nomic-embed-text recommended)
//! - Chat / structured extraction via /api/chat or /api/generate

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    client: Client,
    pub embedding_model: String,
    pub chat_model: String,
}

impl OllamaClient {
    pub fn new(base_url: &str, embedding_model: &str, chat_model: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            embedding_model: embedding_model.to_string(),
            chat_model: chat_model.to_string(),
        }
    }

    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }

    /// Get embedding vector (768 dims for nomic-embed-text)
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            prompt: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            embedding: Vec<f32>,
        }

        let url = format!("{}/api/embeddings", self.base_url);
        let body = Req {
            model: &self.embedding_model,
            prompt: text,
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json::<Resp>()
            .await
            .with_context(|| "Failed to call Ollama embeddings")?;

        Ok(resp.embedding)
    }

    /// Ask model for JSON structured extraction.
    /// Used for decisions, tags, summaries.
    pub async fn extract_structured(&self, prompt: &str, schema_hint: &str) -> Result<String> {
        // Simple non-streaming call.
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            prompt: String,
            stream: bool,
        }
        #[derive(Deserialize)]
        struct Resp {
            response: Option<String>,
            message: Option<serde_json::Value>,
        }

        let full_prompt = format!(
            "{}\n\nRespond ONLY with valid minified JSON matching this schema:\n{}",
            prompt, schema_hint
        );

        let url = format!("{}/api/generate", self.base_url);
        let body = Req {
            model: &self.chat_model,
            prompt: full_prompt,
            stream: false,
        };

        let r = self.client.post(&url).json(&body).send().await?;
        let resp: Resp = r.json().await?;

        if let Some(m) = resp.message {
            if let Some(content) = m.get("content").and_then(|c| c.as_str()) {
                return Ok(content.to_string());
            }
        }
        Ok(resp.response.unwrap_or_default())
    }
}
