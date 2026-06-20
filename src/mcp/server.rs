//! Lightweight MCP stdio server.
//! Implements the minimum protocol needed so agents can call tools like `search` and `weave`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{stdin, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::Config;
use crate::search::SearchEngine;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

pub async fn run_stdio_server(config: Config) -> Result<()> {
    let mut reader = BufReader::new(stdin());
    let mut stdout = tokio::io::stdout();

    // Simple handshake (MCP uses initialize + tools/list + tools/call)
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            break; // EOF
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let err = json!({"code": -32700, "message": format!("Parse error: {}", e)});
                let resp = JsonRpcResponse { jsonrpc: "2.0".into(), id: None, result: None, error: Some(err) };
                let json = serde_json::to_string(&resp)? + "\n";
                let _ = stdout.write_all(json.as_bytes()).await;
                let _ = stdout.flush().await;
                continue;
            }
        };

        let response = match req.method.as_str() {
            "initialize" => handle_initialize(req.id),
            "tools/list" => handle_tools_list(req.id),
            "tools/call" => handle_tool_call(req.id, req.params, &config).await,
            "ping" => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: Some(json!({"ok": true})),
                error: None,
            },
            _ => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: None,
                error: Some(json!({"code":-32601, "message": "Method not found"})),
            },
        };

        let out = serde_json::to_string(&response)? + "\n";
        let _ = stdout.write_all(out.as_bytes()).await;
        let _ = stdout.flush().await;
    }

    Ok(())
}

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "sessionweave",
                "version": "0.1.0"
            }
        })),
        error: None,
    }
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(json!({
            "tools": [
                {
                    "name": "sw_search",
                    "description": "Hybrid search across your indexed AI coding sessions",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" },
                            "limit": { "type": "integer", "default": 10 }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "sw_weave",
                    "description": "Generate a rich recap + copy-pasteable context for a topic or feature",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "sw_timeline",
                    "description": "Return chronological view for a feature",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "feature": { "type": "string" }
                        }
                    }
                }
            ]
        })),
        error: None,
    }
}

async fn handle_tool_call(id: Option<Value>, params: Value, config: &Config) -> JsonRpcResponse {
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match tool_name {
        "sw_search" => {
            let query = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = arguments.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

            let engine = SearchEngine::new(config.clone()).await;
            match engine {
                Ok(e) => {
                    let hits = e.hybrid_search(query, limit, None).await.unwrap_or_default();
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&hits).unwrap() }] })
                }
                Err(e) => json_error(&format!("Search failed: {}", e)),
            }
        }
        "sw_weave" => {
            let query = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let engine = SearchEngine::new(config.clone()).await;
            match engine {
                Ok(e) => {
                    let recap = e.weave(query, true, 30).await.unwrap_or_default();
                    json!({ "content": [{ "type": "text", "text": recap.paste_ready_block }] })
                }
                Err(e) => json_error(&format!("Weave failed: {}", e)),
            }
        }
        "sw_timeline" => {
            let feature = arguments.get("feature").and_then(|v| v.as_str());
            let engine = SearchEngine::new(config.clone()).await;
            match engine {
                Ok(e) => {
                    let tl = e.timeline(feature, 25).await.unwrap_or_default();
                    json!({ "content": [{ "type": "text", "text": tl }] })
                }
                Err(e) => json_error(&format!("Timeline failed: {}", e)),
            }
        }
        _ => json_error("Unknown tool"),
    };

    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(result),
        error: None,
    }
}

fn json_error(msg: &str) -> Value {
    json!({
        "isError": true,
        "content": [{ "type": "text", "text": msg }]
    })
}
