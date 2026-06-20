# SessionWeave — Architecture Plan

**Mission**: Local, private, unified memory for all your AI coding sessions across Claude Code, Cursor, Windsurf, Codex, and custom exports. Search and "weave" coherent context instantly.

**Design Principles** (for shadcn-level quality)
- Local-first, zero cloud, full privacy.
- Single fast binary (`sw`).
- Speed + correctness > features.
- Extensible parsers + intelligence layer.
- Beautiful, useful CLI output (rich Markdown + structured).
- Config over magic. TOML everywhere.
- Graceful degradation (works with or without Ollama).

---

## 1. High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         sw CLI (clap)                           │
│  index | watch | search | weave/resume | timeline | export | mcp │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Core Orchestrator                         │
│  • Config loader (TOML)                                         │
│  • Project resolver (cwd + explicit)                            │
└─────────────────────────────────────────────────────────────────┘
           │                     │                     │
           ▼                     ▼                     ▼
   ┌───────────────┐    ┌────────────────┐   ┌────────────────┐
   │   Indexer     │    │    Search      │   │    Weave       │
   │  - Walk dirs  │    │  Hybrid FTS+   │   │  - Retrieve    │
   │  - Parsers    │    │    Vector      │   │  - Synthesize  │
   │  - Extract    │    │                │   │  - Format MD   │
   └───────┬───────┘    └───────┬────────┘   └────────────────┘
           │                    │
           ▼                    ▼
┌────────────────────┐   ┌────────────────────────────┐
│   Storage Layer    │   │    Intelligence (Ollama)   │
│  • SQLite (meta+FTS)│   │  • Embeddings (nomic)     │
│  • LanceDB (vec)   │   │  • Decision extraction    │
│  • Graph tables    │   │  • Auto-tagging / cluster │
│                    │   │  • Session summarizer     │
└────────────────────┘   └────────────────────────────┘

Watcher (notify + tokio)  ←→  Indexer
MCP Server (stdio / rmcp) ←→  Query layer
```

---

## 2. Data Model (Normalized)

**Core Tables (SQLite)**

- `projects(id, name, root_path, created_at)`
- `sessions(id, project_id, source_path, harness, session_key, started_at, ended_at, title, summary, raw_path)`
- `messages(id, session_id, role, content, timestamp, turn_index, embedding_ref)`
- `artifacts(id, session_id, message_id, file_path, action, snippet)`
- `decisions(id, session_id, message_id, text, confidence, feature_tags)`
- `tags(id, name, kind, description)`
- `session_tags(session_id, tag_id)`
- `relations(from_session, to_session, relation_type, strength)` — light graph

**FTS5**: virtual table on `messages(content)` + `sessions(summary, title)`.

**LanceDB**:
- Table `message_embeddings`: id (ref to sqlite), vector (768), metadata (harness, session_id, role, files...)
- Table `session_embeddings`

Embeddings stored with back-ref to canonical SQLite row.

**Graph**: Use simple SQLite edge table + in-memory traversal for "related threads".

---

## 3. Supported Harnesses & Parsing Strategy

| Harness     | Location (typical)                              | Format          | Parser     | Notes |
|-------------|-------------------------------------------------|-----------------|------------|-------|
| Claude Code | `~/.claude/projects/<proj>/*.jsonl` or `sessions/*.jsonl` | JSONL (one event/line) | `claude`   | Rich: tools, files, thoughts |
| Cursor      | `~/Library/Application Support/Cursor/User/{global,workspace}Storage/*/state.vscdb` | SQLite (JSON blobs) | `cursor`   | Parse ItemTable / chat state |
| Windsurf    | Project local + exports                         | MD / JSON       | `generic`  | Memories + Cascade sessions |
| Codex       | `~/.codex/...`                                  | JSON / logs     | `generic`  | Treat as generic |
| Generic     | Any dir                                         | *.md, *.json    | `generic`  | User-provided exports |

**Parsing Pipeline**:
1. Walk + filter (date, size, known names).
2. Type detection.
3. Stream parse → normalized `Turn` structs.
4. **Rule-based extraction** first (regex for file paths `[\w./-]+\.(rs|ts|tsx|py|...))`, `diff --git`, etc.
5. **LLM extraction** (optional, opt-in, batched): structured JSON for:
   - `key_decisions: []`
   - `files_affected: []`
   - `feature_tags: []`
   - `short_summary`
6. Store + embed.

**Idempotency**: Use content hash + source path + mtime to skip reindex. Store `last_indexed`.

---

## 4. CLI Commands (Must Work Perfectly)

```bash
# Indexing
sw index [PATH] --force              # manual one-shot
sw watch                             # daemon, watches configured sources + cwd
sw index --all                       # reindex everything

# Query
sw search "auth system" --limit 20   # hybrid ranked results
sw search "where did we implement payments" --json

sw weave "building the auth system"  # full coherent recap
sw resume "auth"                     # alias of weave (most used)
sw timeline auth                     # chronological MD

sw export --project myapp --format md > memory.md

# Intelligence
sw tag --auto                        # run LLM clustering
sw mcp                               # start MCP stdio server for Cursor/Claude

# Config
sw config --edit
sw config path
```

All commands output beautiful terminal Markdown (use `termimad` or simple + colors + bat).

---

## 5. Intelligence Layer (Ollama)

- **Default models**:
  - Embed: `nomic-embed-text` (or `nomic-embed-text-v1.5`)
  - Extract/Summarize/Tag: `qwen2.5-coder:7b` or `llama3.2` or user config
- All calls are fire-and-forget where possible, cached.
- Graceful fallback: if Ollama down → index without LLM features, search still works via FTS.

**Prompts for extraction** are versioned in code (or external templates later).

---

## 6. Hybrid Search

1. FTS5 query on messages + sessions → candidate set (fast).
2. Embed query → LanceDB ANN top-K.
3. Fusion (Reciprocal Rank Fusion or simple weighted score).
4. Re-rank lightly with metadata boost (recency, same project, has file mentions).
5. Return rich context objects.

---

## 7. Weave / Resume Logic

- Take natural language query.
- Run hybrid search for top relevant messages/sessions.
- Group by feature/session chronologically.
- Optionally call LLM once for synthesis:
  - "Here is the complete picture of building X..."
  - Extract timeline bullets.
  - Pull out key code blocks (from artifacts or message content).
  - Highlight open questions / decisions.
- Output:
  - Executive summary
  - Timeline
  - Key files & changes
  - Full relevant transcript excerpts
  - Ready-to-paste "CONTEXT FOR NEW SESSION" block.

---

## 8. Watcher (Daemon)

- Uses `notify` crate (recommended recursive watcher).
- Debounce + batching.
- On change → enqueue → parse delta → index.
- Can run in foreground with nice TUI progress or background (simple pid file? or just long lived process).
- `sw watch` blocks.

---

## 9. MCP Server

Run `sw mcp`.

Implements MCP stdio server (using `modelcontextprotocol/rust-sdk`).

Exposed **Tools** (for host agents like Cursor/Claude):
- `sw_search(query, limit?)`
- `sw_weave(query)` → returns Markdown context
- `sw_timeline(feature)`
- `sw_list_sessions(project?)`
- `sw_get_session(id)`

Also **Resources** for raw data if wanted.

This is the killer feature for power users: your agents can query their own past automatically.

---

## 10. Storage Layout (on disk)

```
~/.sessionweave/
├── config.toml
├── db/
│   └── sessionweave.db          # SQLite (meta + FTS)
├── vectors/
│   └── (lancedb data dir)
├── logs/
└── cache/
    └── embeddings/...
```

Project-local override supported via `./.sessionweave/config.toml`.

---

## 11. Phased Implementation Plan (this build)

**Phase 0 (Now)**: Project scaffold + CLI skeleton + TOML config + basic SQLite + "hello index".

**Phase 1**: Claude JSONL parser + generic MD/JSON. Full Session/Message model. `sw index` works on sample.

**Phase 2**: Full SQLite + FTS5 storage + basic `sw search`.

**Phase 3**: Ollama client + embeddings + simple extraction.

**Phase 4**: LanceDB + hybrid search.

**Phase 5**: `weave` / `resume` / `timeline` / export.

**Phase 6**: Watcher daemon.

**Phase 7**: MCP server.

**Phase 8**: Polish, tests, install script, README, examples + fixtures.

**Phase 9 (stretch)**: Tauri dashboard (separate or optional).

---

## 12. Key Dependencies (Rust)

- `clap` + `clap_complete` + `colored`
- `tokio` (full)
- `rusqlite` (bundled + modern-sqlite for FTS)
- `lancedb`
- `serde`, `serde_json`, `toml`
- `ollama-rs` or `reqwest` + `serde`
- `notify`
- `walkdir`
- `regex`
- `anyhow` + `thiserror`
- `chrono`
- `indicatif` (progress)
- `rmcp` or `rust-mcp-sdk` (MCP)
- `termimad` or `bat` integration for pretty output (optional)

---

## 13. Non-Goals (for v1)

- Cloud sync
- Heavy UI (Tauri is nice-to-have)
- Multi-user
- Automatic LLM fine-tuning
- Perfect 100% parse of every proprietary binary format (focus on main + generic)

---

## 14. Success Criteria

- `sw index ~/.claude/projects` succeeds and stores 100s of sessions.
- `sw resume "how we did auth"` returns useful multi-turn summary in <3s.
- `sw mcp` starts and Cursor/Claude can successfully call tools.
- Works completely offline after models downloaded.
- Single `cargo build --release` → `target/release/sw`
- Install script puts it on PATH + adds shell completions.

---

This is the blueprint. Implementation must stay faithful to this while shipping fast and clean code.

Build for power users who live in the terminal.
