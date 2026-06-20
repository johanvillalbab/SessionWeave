//! Background filesystem watcher using `notify`.
//! On changes, triggers incremental indexing.

use anyhow::Result;
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::time::sleep;

use crate::config::Config;
use crate::index::Indexer;

pub async fn start_watcher(config: Config, specific_path: Option<PathBuf>) -> Result<()> {
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        NotifyConfig::default(),
    )?;

    let paths_to_watch: Vec<PathBuf> = if let Some(p) = specific_path {
        vec![crate::config::expand_tilde(&p.to_string_lossy())]
    } else {
        config
            .sources
            .iter()
            .map(|s| crate::config::expand_tilde(&s.path.to_string_lossy()))
            .collect()
    };

    for p in &paths_to_watch {
        if p.exists() {
            println!("Watching: {}", p.display());
            let _ = watcher.watch(p, RecursiveMode::Recursive);
        } else {
            println!("(skip non-existing) {}", p.display());
        }
    }

    let indexer = Indexer::new(config.clone())?;

    println!("Watcher active. Waiting for changes...\n");

    loop {
        // Drain any events (debounced)
        let mut changed = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                Ok(evt) => {
                    if matches!(evt.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) {
                        for p in &evt.paths {
                            if p.extension().map_or(false, |e| matches!(e.to_str(), Some("jsonl" | "json" | "md"))) {
                                println!("Change detected: {}", p.display());
                                changed = true;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Watcher error: {:?}", e),
            }
        }

        if changed {
            println!("Re-indexing changed sources...");
            let _ = indexer.index_all(false, false).await;
            println!("Done.\n");
        }

        // Sleep to debounce
        sleep(Duration::from_millis(1200)).await;
    }
}
