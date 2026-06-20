# SessionWeave

> Local. Private. Unified memory for your AI coding sessions.

**SessionWeave** (`sw`) indexes conversations from **Claude Code**, **Cursor**, **Windsurf**, **Codex** and other AI coding tools. It lets you search with natural language and instantly "weave" coherent, copy-paste-ready context about how you built features.

Everything runs **100% locally** using SQLite + LanceDB + Ollama. No data ever leaves your machine.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-macOS%20|%20Linux%20|%20Windows-lightgrey)](#installation)

---

## Why SessionWeave?

Modern developers (especially power users) switch between multiple AI coding environments:

- Claude Code
- Cursor
- Windsurf (Codeium Cascade)
- Codex / Continue / Aider / etc.

Each tool has its own fragmented history. Finding "where we discussed the auth changes" or "how did we decide on the database schema?" becomes painful.

**SessionWeave** solves this by giving you a single, private, searchable memory layer across all your AI coding sessions.

## Features

- **Intelligent Indexing** — Automatically parses Claude Code JSONL, Cursor workspaces, Markdown exports, and generic JSON.
- **Hybrid Search** — Combines full-text search (FTS5) with semantic vector search (LanceDB + nomic-embed-text).
- **Weave & Resume** — The killer feature. Generate rich, chronological recaps of any topic or feature (`sw resume "jwt auth flow"`).
- **LLM-Powered Insights** — Extracts key decisions and feature tags using a local LLM during indexing.
- **MCP Server** — Expose SessionWeave as a Model Context Protocol server so your agents (Cursor, Claude, etc.) can query your past sessions directly.
- **Privacy First** — Runs completely offline. Your data never leaves your computer.
- **Single Binary** — Written in Rust. One fast, portable executable.

## Quick Start

### 1. Install

**From source (recommended today):**

```bash
git clone https://github.com/johanvillalba/sessionweave.git
cd sessionweave
cargo build --release
sudo ln -s $(pwd)/target/release/sw /usr/local/bin/sw
```

**With Ollama (for full semantic power):**

```bash
ollama pull nomic-embed-text
ollama pull qwen2.5-coder:7b   # or any good local model
```

### 2. Index your sessions

```bash
# Claude Code
sw index ~/.claude/projects

# Cursor
sw index ~/Library/Application\ Support/Cursor/User/workspaceStorage

# Or watch for changes
sw watch
```

### 3. Use it

```bash
sw search "authentication"                    # hybrid search
sw resume "how we implemented JWT auth"       # the magic command
sw stats                                      # see what you have indexed
sw mcp                                        # start MCP server for agents
```

## Usage Examples

### Resume a feature discussion

```bash
sw resume "building the auth system"
```

**Output includes:**
- Timeline of relevant turns
- Key files touched
- Extracted decisions ("We decided to use RS256...")
- Ready-to-paste context block for your next AI session

### Search with natural language

```bash
sw search "where did we talk about rate limiting for payments"
```

When using Ollama embeddings you’ll see hybrid results with a `[vector hit]` indicator.

## Commands

| Command          | Description                                      |
|------------------|--------------------------------------------------|
| `sw index [path]`| Index a directory or file                        |
| `sw watch`       | Continuously watch and index new sessions        |
| `sw search <q>`  | Hybrid search across all indexed content         |
| `sw resume <q>`  | Generate rich recap + copy-paste context         |
| `sw weave <q>`   | Alias for resume                                 |
| `sw timeline [feature]` | Chronological view of a feature           |
| `sw stats`       | Show database statistics and health              |
| `sw export`      | Export unified memory to Markdown/JSON           |
| `sw mcp`         | Start MCP stdio server                           |
| `sw config`      | View or edit configuration                       |

## Configuration

SessionWeave looks for configuration in this order:

1. `--config /path/to/config.toml`
2. `.sessionweave/config.toml` (project-local)
3. `~/.config/sessionweave/config.toml` (global)

Example `config.toml`:

```toml
[general]
data_dir = "~/.sessionweave"

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

## How It Works

1. **Discovery & Parsing** — Walks configured paths and intelligently parses each harness format.
2. **Extraction** — Rule-based + optional LLM extraction of decisions and tags.
3. **Storage** — Normalized data in SQLite (with FTS5) + vectors in LanceDB.
4. **Search & Synthesis** — Hybrid retrieval + optional synthesis for high-quality recaps.
5. **MCP Exposure** — Agents can call tools like `search` and `weave` directly.

## Supported Tools (Current)

| Tool          | Format Support      | Notes                          |
|---------------|---------------------|--------------------------------|
| Claude Code   | Excellent (`*.jsonl`) | Full tool calls, files, turns |
| Cursor        | Basic               | workspaceStorage support       |
| Generic       | Markdown + JSON     | Any exported chat              |
| Windsurf / Codex | Partial          | Via generic parser             |

## Privacy & Philosophy

- **Zero telemetry**
- **No cloud sync**
- **Your data, your machine**
- Designed for developers who treat conversation history as first-class engineering artifacts

## Roadmap

- Full deep Cursor history support
- Better multi-project and multi-harness clustering
- Optional Tauri GUI (future)
- Prebuilt binaries + package managers (Homebrew, apt, etc.)
- Shell completions

See [PLAN.md](PLAN.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for more details.

## Contributing

Contributions are very welcome!

1. Fork the repo
2. Create a feature branch (`git checkout -b feat/amazing-thing`)
3. Make your changes + tests
4. Open a Pull Request

Please keep changes focused and add tests when possible.

## License

MIT License — see [LICENSE](LICENSE) for details.

## Acknowledgments

Built for developers who live at the intersection of human and AI coding.

Special thanks to the Rust, SQLite, LanceDB, and Ollama communities.

---

**SessionWeave** — Never lose context again.

```bash
sw resume "that one decision we made about..."
```