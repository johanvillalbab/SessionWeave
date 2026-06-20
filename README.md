# SessionWeave (`sw`)

**Local. Private. Unified memory for all your AI coding conversations.**

SessionWeave indexes sessions from **Claude Code**, **Cursor**, **Windsurf**, **Codex** and any Markdown/JSON exports. It lets you search and "weave" coherent recaps of how you built features — perfect for power users (like shadcn) who live across multiple AI harnesses.

Everything runs 100% locally with SQLite + LanceDB + Ollama.

---

## Features (MVP complete)

- Automatic + manual indexing (`sw index`, `sw watch`)
- Full-text + hybrid vector search (`sw search`)
- `sw weave` / `sw resume` — rich recap with timeline, files, extracted decisions + tags + paste block
- `sw stats` — sessions, messages, tags, decisions, vectors status, dedup %
- Timeline, Export, MCP server
- Proper content_hash deduplication on sessions & messages
- Claude-first parser + generic; LLM extraction (optional)

## MVP Status (complete as of 2026-06-20)

**What is shipped & polished:**
- Full CLI surface: `index`, `watch`, `search`, `resume`/`weave`, `timeline`, `export`, `mcp`, `stats`, `config`
- Strong Claude Code JSONL parser (tools, file paths, turns) + generic MD/JSON
- SQLite + FTS5 tolerant search
- LanceDB vector store (initialized; embeddings when Ollama available)
- Proper `content_hash` on sessions + messages for true deduplication (skips identical content even on re-runs)
- LLM extraction (when Ollama): decisions + auto tags during `index`
- `sw stats` shows sessions/messages/tags/decisions + vector status + dedup coverage
- Enhanced output: index reports embeddings added; search labels hybrid vs FTS-only
- `resume`/`weave` includes extracted decisions and tags (when populated)
- MCP server for agent use
- Idempotent + graceful no-Ollama mode

**To enable full power (hybrid vectors + auto decisions/tags):**
```bash
ollama pull nomic-embed-text
ollama pull qwen2.5-coder:7b   # or llama3, etc
sw index ~/.claude/projects --force
sw search "auth"     # now shows "(hybrid: FTS + vector)"
sw resume "jwt login"
sw stats
```

Without Ollama you still get excellent FTS search + basic indexing.

---

## Installation

### Quick (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/sessionweave/sessionweave/main/install.sh | bash
```

Or build from source:

```bash
git clone https://github.com/sessionweave/sessionweave
cd sessionweave
cargo build --release
# binary is at target/release/sw
```

Add to PATH or symlink:
```bash
sudo ln -s $(pwd)/target/release/sw /usr/local/bin/sw
```

### Ollama (recommended for full power)

```bash
# Install Ollama
ollama pull nomic-embed-text
ollama pull qwen2.5-coder:7b   # or llama3.2, codellama, etc.
```

---

## Usage (current working commands)

```bash
# First run creates ~/.config/sessionweave/config.toml
sw config show

# Index a Claude Code export / log file or directory
sw index ~/.claude/projects
sw index tests/fixtures/sample_claude.jsonl   # example in this repo

# Index Cursor (basic support; full vscdb parser in progress)
sw index ~/Library/Application\ Support/Cursor/User

# Watch mode (auto reindex on changes)
sw watch

# Search (FTS + tolerant matching)
sw search "auth system"
sw search "where did we talk about payments"   # shows (hybrid...) when vectors on

# Generate rich context recap (the killer feature)
sw resume "building the auth system"
sw weave "how we implemented feature flags"

# Stats + dedup insight
sw stats

# Timeline
sw timeline auth

# Export
sw export --format md > my-memory.md

# MCP server (Cursor / Claude Code / Windsurf can call SessionWeave)
sw mcp
```

Example output of `sw resume "auth"` contains a ready-to-paste context block with timeline and key excerpts.

---

## Configuration

Default location: `~/.config/sessionweave/config.toml`

Example:

```toml
[general]
data_dir = "~/.sessionweave"
default_limit = 20

[ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"
chat_model = "qwen2.5-coder:7b"
enabled = true

[[sources]]
path = "~/.claude/projects"
type = "claude"

[[sources]]
path = "~/Library/Application Support/Cursor/User"
type = "cursor"
```

You can also put `.sessionweave/config.toml` inside any project.

---

## How Parsing Works

1. **Rules first** — file paths, tool calls, diffs are extracted deterministically.
2. **LLM (Ollama)** — when enabled, extracts decisions, summaries, and auto-tags.
3. Idempotent via content hash + source path.

Supported today:
- Claude Code: `*.jsonl` under `~/.claude/projects`
- Generic: Markdown chats, JSON/JSONL message arrays
- Cursor: Planned full support (state.vscdb)

---

## Architecture

See [PLAN.md](./PLAN.md) for detailed architecture, data model and roadmap.

Core stack:
- Rust (single binary `sw`)
- SQLite + FTS5
- LanceDB (vectors)
- Ollama (embeddings + intelligence)
- MCP stdio server

---

## Development

```bash
cargo build
cargo test
cargo run -- index tests/fixtures/sample_claude.jsonl
cargo run -- resume "auth"
```

---

## Roadmap

- [x] Core CLI + config
- [x] Claude + generic parsers
- [x] SQLite + FTS5
- [ ] Full LanceDB hybrid search
- [ ] Automatic LLM extraction during index
- [ ] Watcher polish + debouncing
- [ ] Full MCP tool surface + resources
- [ ] Tauri optional dashboard
- [ ] Shell completions + better pretty printing

---

## License

MIT © SessionWeave Contributors

Built for developers who treat their AI conversations as first-class engineering artifacts.
