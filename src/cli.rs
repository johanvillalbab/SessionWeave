//! CLI definition using clap.
//! All top-level commands for SessionWeave.

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

use crate::config::Config;
use crate::db::{DbStats, SqliteStore};
use crate::index::Indexer;
use crate::search::SearchEngine;
use crate::watcher::start_watcher;

#[derive(Parser, Debug)]
#[command(
    name = "sw",
    version,
    author = "SessionWeave Contributors",
    about = "SessionWeave — Weave your AI coding sessions into coherent, searchable memory. Local. Private. Fast.",
    long_about = "SessionWeave indexes conversations from Claude Code, Cursor, Windsurf, Codex and more.\n\
                  Search with natural language, generate context-rich recaps (weave), and keep everything local."
)]
pub struct Cli {
    /// Override config file location
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Index sessions from a path (or configured sources)
    Index {
        /// Path to index (file or directory). If omitted, uses configured sources.
        path: Option<PathBuf>,

        /// Force re-index even if unchanged
        #[arg(long, short)]
        force: bool,

        /// Only show what would be indexed (dry run)
        #[arg(long)]
        dry_run: bool,
    },

    /// Start background watcher that auto-indexes on file changes
    Watch {
        /// Watch only specific path instead of all configured
        path: Option<PathBuf>,
    },

    /// Hybrid search across all indexed content (FTS + semantic)
    Search {
        /// Natural language query
        query: String,

        /// Max number of results
        #[arg(long, short, default_value_t = 15)]
        limit: usize,

        /// Output as JSON (for scripting / agents)
        #[arg(long)]
        json: bool,

        /// Filter by project name
        #[arg(long)]
        project: Option<String>,
    },

    /// Generate a complete, coherent recap + ready-to-paste context ("weave" sessions)
    Weave {
        /// What to weave (e.g. "building the auth system", "payments flow")
        query: String,

        /// Include full excerpts of relevant messages
        #[arg(long)]
        full: bool,

        /// Max messages to pull into context
        #[arg(long, default_value_t = 40)]
        context: usize,
    },

    /// Alias for `weave` — more conversational
    Resume {
        query: String,

        #[arg(long)]
        full: bool,

        #[arg(long, default_value_t = 40)]
        context: usize,
    },

    /// Show chronological timeline for a feature or session
    Timeline {
        /// Feature name or partial match (e.g. "auth", "payments")
        feature: Option<String>,

        /// Limit number of entries
        #[arg(long, short, default_value_t = 30)]
        limit: usize,
    },

    /// Export all (or filtered) memory to unified Markdown / JSON
    Export {
        /// Output file (stdout if omitted)
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Format: md | json
        #[arg(long, default_value = "md")]
        format: String,

        /// Only a specific project
        #[arg(long)]
        project: Option<String>,
    },

    /// Start MCP stdio server (for Cursor, Claude, Windsurf to query you directly)
    Mcp {},

    /// Show database & index statistics (sessions, messages, tags, decisions, vectors, dedup hashes)
    Stats {},

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Show resolved config path and values
    Show,

    /// Open config file in $EDITOR
    Edit,

    /// Print path to config file
    Path,
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        // Load or initialize config
        let config = if let Some(path) = &self.config {
            Config::load_from(path)?
        } else {
            Config::load_or_init()?
        };

        match self.command {
            Commands::Index { path, force, dry_run } => {
                handle_index(config, path, force, dry_run).await
            }
            Commands::Watch { path } => {
                handle_watch(config, path).await
            }
            Commands::Search { query, limit, json, project } => {
                handle_search(config, query, limit, json, project).await
            }
            Commands::Weave { query, full, context } => {
                handle_weave(config, query, full, context).await
            }
            Commands::Resume { query, full, context } => {
                handle_weave(config, query, full, context).await
            }
            Commands::Timeline { feature, limit } => {
                handle_timeline(config, feature, limit).await
            }
            Commands::Export { output, format, project } => {
                handle_export(config, output, format, project).await
            }
            Commands::Mcp {} => {
                handle_mcp(config).await
            }
            Commands::Stats {} => {
                handle_stats(config).await
            }
            Commands::Config { action } => {
                handle_config(config, action)
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Command handlers (stubs that will be wired to real modules)
// -----------------------------------------------------------------------------

async fn handle_index(
    config: Config,
    path: Option<PathBuf>,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    println!("{}", "SessionWeave Indexer".bold().cyan());
    println!("Config loaded from: {:?}", config.config_path);

    let indexer = Indexer::new(config)?;

    if let Some(p) = path {
        println!("Indexing path: {}", p.display());
        let stats = indexer.index_path(&p, force, dry_run).await?;
        println!("  sessions: {}  messages: {}  files: {}  skipped: {}  embeddings: {}",
            stats.sessions_indexed, stats.messages_indexed, stats.files_scanned, stats.skipped,
            if stats.embeddings_added > 0 { format!("{}", stats.embeddings_added) } else { "n/a (or 0)".to_string() }
        );
    } else {
        println!("Indexing all configured sources...");
        let stats = indexer.index_all(force, dry_run).await?;
        println!("  sessions: {}  messages: {}  files: {}  skipped: {}  embeddings: {}",
            stats.sessions_indexed, stats.messages_indexed, stats.files_scanned, stats.skipped,
            if stats.embeddings_added > 0 { format!("{}", stats.embeddings_added) } else { "n/a (or 0)".to_string() }
        );
    }
    if !dry_run {
        println!("   (use `sw stats` for full picture; embeddings shown only when Ollama + Lance active)");
    }
    Ok(())
}

async fn handle_watch(config: Config, path: Option<PathBuf>) -> Result<()> {
    println!("{}", "SessionWeave Watcher (daemon mode)".bold().green());
    println!("Press Ctrl-C to stop.\n");

    start_watcher(config, path).await
}

async fn handle_search(
    config: Config,
    query: String,
    limit: usize,
    json: bool,
    project: Option<String>,
) -> Result<()> {
    let engine = SearchEngine::new(config).await?;

    let results = engine.hybrid_search(&query, limit, project.as_deref()).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    let hybrid = engine.is_hybrid_ready();
    println!(
        "{} {} results for: \"{}\" {}",
        "Found".green(),
        results.len(),
        query.cyan(),
        if hybrid { "(hybrid: FTS + vector)".dimmed() } else { "(FTS only)".dimmed() }
    );

    for (i, r) in results.iter().enumerate() {
        println!("\n{}. {} {}", i + 1, r.harness.cyan(), r.timestamp);
        println!("   {}", r.snippet.replace('\n', " ").chars().take(160).collect::<String>());
        if let Some(f) = &r.files {
            println!("   files: {}", f.join(", ").dimmed());
        }
        if r.via_vector {
            println!("   {}", "[vector hit contributed to ranking]".yellow());
        }
    }
    Ok(())
}

async fn handle_weave(
    config: Config,
    query: String,
    full: bool,
    context_limit: usize,
) -> Result<()> {
    let engine = SearchEngine::new(config).await?;
    let recap = engine.weave(&query, full, context_limit).await?;

    // Beautiful output
    println!("{}", "════════════════════════════════════════".dimmed());
    println!("{}", "SESSIONWEAVE — CONTEXT RECAP".bold().magenta());
    println!("{}", "════════════════════════════════════════".dimmed());
    println!("\n{}", recap.summary);
    println!("\n{}", "Timeline:".bold());
    println!("{}", recap.timeline);
    println!("\n{}", "Key Files & Decisions:".bold());
    println!("{}", recap.key_points);

    if full {
        println!("\n{}", "Relevant Excerpts:".bold());
        println!("{}", recap.excerpts);
    }

    println!("\n{}", "════════════════════════════════════════".dimmed());
    println!("{}", "Copy the block below into your next session:".dimmed());
    println!("\n{}\n", recap.paste_ready_block);
    Ok(())
}

async fn handle_timeline(
    config: Config,
    feature: Option<String>,
    limit: usize,
) -> Result<()> {
    let engine = SearchEngine::new(config).await?;
    let timeline = engine.timeline(feature.as_deref(), limit).await?;
    println!("{}", timeline);
    Ok(())
}

async fn handle_export(
    _config: Config,
    output: Option<PathBuf>,
    format: String,
    project: Option<String>,
) -> Result<()> {
    println!("Exporting (format={})...", format);
    // TODO: real implementation
    let content = format!("# SessionWeave Export\n\nProject: {:?}\n\n(Full export coming soon)\n", project);
    if let Some(path) = output {
        std::fs::write(&path, content)?;
        println!("Exported to {}", path.display());
    } else {
        println!("{}", content);
    }
    Ok(())
}

async fn handle_mcp(_config: Config) -> Result<()> {
    println!("{}", "Starting SessionWeave MCP server on stdio...".green());
    println!("(Connect this from Cursor / Claude Code as an MCP server)\n");
    crate::mcp::server::run_stdio_server(_config).await
}

fn handle_config(config: Config, action: ConfigCommands) -> Result<()> {
    match action {
        ConfigCommands::Show => {
            println!("Config file: {:?}", config.config_path);
            println!("{}", toml::to_string_pretty(&config)?);
        }
        ConfigCommands::Edit => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
            let path = config.config_path.clone();
            println!("Opening {} with {}", path.display(), editor);
            std::process::Command::new(editor).arg(path).status()?;
        }
        ConfigCommands::Path => {
            println!("{}", config.config_path.display());
        }
    }
    Ok(())
}

async fn handle_stats(config: Config) -> Result<()> {
    println!("{}", "SessionWeave Stats".bold().cyan());
    println!("Config: {:?}", config.config_path);

    let db_path = config.resolve_data_path("db/sessionweave.db");
    let store = SqliteStore::new(&db_path)?;
    let stats: DbStats = store.stats()?;

    println!("\n{}", "Database".bold());
    println!("  sessions:               {}", stats.sessions);
    println!("  messages:               {}", stats.messages);
    println!("  decisions (extracted):  {}", stats.decisions);
    println!("  tags:                   {}", stats.tags);
    println!("  sessions with tags:     {}", stats.tagged_sessions);
    println!("  messages w/ decisions:  {}", stats.messages_with_decisions);
    println!("  sessions w/ content_hash: {}", stats.sessions_with_content_hash);

    // Vector info (from sqlite embedding refs + presence of vectors dir)
    println!("\n{}", "Vectors / Embeddings".bold());
    println!("  messages with vector ref: {}", stats.messages_with_vectors);
    let vectors_dir = config.resolve_data_path("vectors");
    let has_vectors_dir = vectors_dir.exists();
    println!("  vectors dir present:    {}", if has_vectors_dir { "yes" } else { "no" });

    // Heuristic for "vectors populated"
    let vectors_populated = stats.messages_with_vectors > 0 || (has_vectors_dir && stats.messages > 0);
    println!("  hybrid vectors ready:   {}", if vectors_populated { "yes (hybrid search enabled)" } else { "no (FTS-only; run index with Ollama for embeddings)" });

    println!("\n{}", "Deduplication".bold());
    let dedup_ratio = if stats.sessions > 0 {
        (stats.sessions_with_content_hash as f64 / stats.sessions as f64) * 100.0
    } else { 0.0 };
    println!("  hashed sessions:        {}/{} ({:.0}%)", stats.sessions_with_content_hash, stats.sessions, dedup_ratio);

    println!();
    Ok(())
}
