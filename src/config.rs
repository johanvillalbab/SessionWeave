//! Configuration management (TOML).
//! Global + project-local config support.

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(skip)]
    pub config_path: PathBuf,

    #[serde(default)]
    pub general: General,

    #[serde(default)]
    pub ollama: Ollama,

    /// Sources to watch and index automatically
    #[serde(default)]
    pub sources: Vec<Source>,

    /// Optional project root overrides
    #[serde(default)]
    pub projects: Vec<ProjectOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    /// Base directory for all SessionWeave data (~/.sessionweave by default)
    pub data_dir: PathBuf,

    /// Default limit for search/weave results
    pub default_limit: usize,
}

impl Default for General {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            data_dir: home.join(".sessionweave"),
            default_limit: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ollama {
    pub url: String,
    pub embedding_model: String,
    pub chat_model: String,
    /// If false, skip all LLM calls (pure keyword/FTS mode)
    pub enabled: bool,
}

impl Default for Ollama {
    fn default() -> Self {
        Self {
            url: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            chat_model: "qwen2.5-coder:7b".to_string(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Filesystem path (supports ~)
    pub path: PathBuf,
    /// One of: "claude", "cursor", "windsurf", "codex", "generic"
    #[serde(rename = "type")]
    pub source_type: String,
    /// Optional glob or name filter
    #[serde(default)]
    pub include: Option<String>,
    /// Recurse?
    #[serde(default = "default_true")]
    pub recursive: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectOverride {
    pub name: String,
    pub root: PathBuf,
    /// Additional source paths for this project
    #[serde(default)]
    pub extra_sources: Vec<PathBuf>,
}

impl Config {
    /// Load from explicit path
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config at {:?}", path))?;
        let mut cfg: Config =
            toml::from_str(&content).with_context(|| "Failed to parse TOML config")?;
        cfg.config_path = path.to_path_buf();
        cfg.ensure_defaults();
        Ok(cfg)
    }

    /// Load global config. Creates default if missing.
    pub fn load_or_init() -> Result<Self> {
        let global = default_config_path();

        if !global.exists() {
            let default = Self::default_config();
            if let Some(parent) = global.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&global, toml::to_string_pretty(&default)?)?;
            println!(
                "Created default config at {}",
                global.display().to_string().green()
            );
            println!("Edit it with: {}", "sw config edit".cyan());
        }

        let mut cfg = Self::load_from(&global)?;

        // Also merge project-local config if present (./.sessionweave/config.toml)
        if let Ok(local) = find_local_config() {
            if local != global {
                let local_cfg = Self::load_from(&local)?;
                cfg = merge_configs(cfg, local_cfg);
            }
        }

        Ok(cfg)
    }

    pub fn default_config() -> Self {
        let mut c = Self {
            config_path: default_config_path(),
            general: General::default(),
            ollama: Ollama::default(),
            sources: vec![
                // Auto-detect common locations (user can edit)
                Source {
                    path: expand_tilde("~/.claude/projects"),
                    source_type: "claude".into(),
                    include: None,
                    recursive: true,
                },
                Source {
                    path: expand_tilde("~/Library/Application Support/Cursor/User"),
                    source_type: "cursor".into(),
                    include: None,
                    recursive: true,
                },
                Source {
                    path: expand_tilde("~/.codex"),
                    source_type: "codex".into(),
                    include: None,
                    recursive: true,
                },
            ],
            projects: vec![],
        };
        c.ensure_defaults();
        c
    }

    fn ensure_defaults(&mut self) {
        if self.sources.is_empty() {
            // fallback
            self.sources = Self::default_config().sources;
        }
    }

    /// Resolve a path relative to this config (useful for data files)
    pub fn resolve_data_path(&self, sub: &str) -> PathBuf {
        self.general.data_dir.join(sub)
    }
}

pub fn default_config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".config")
        .join("sessionweave")
        .join("config.toml")
}

fn find_local_config() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;
    loop {
        let candidate = current.join(".sessionweave").join("config.toml");
        if candidate.exists() {
            return Ok(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    anyhow::bail!("No local config found")
}

fn merge_configs(mut base: Config, local: Config) -> Config {
    // Simple merge strategy: local sources append, general/ollama override if set
    if !local.sources.is_empty() {
        base.sources.extend(local.sources);
    }
    if local.general.data_dir != General::default().data_dir {
        base.general.data_dir = local.general.data_dir;
    }
    if !local.ollama.url.is_empty() {
        base.ollama = local.ollama;
    }
    base.config_path = local.config_path; // last wins for display
    base
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}
