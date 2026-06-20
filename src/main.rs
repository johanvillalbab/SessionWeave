//! SessionWeave (sw)
//!
//! Local, private, unified memory for AI coding sessions.
//! Index • Search • Weave context across Claude Code, Cursor, Windsurf, Codex & more.

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize minimal tracing for logs (can be controlled via RUST_LOG)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("sessionweave=info".parse().unwrap_or_default()),
        )
        .with_target(false)
        .init();

    let cli = sessionweave::cli::Cli::parse();
    cli.run().await
}
