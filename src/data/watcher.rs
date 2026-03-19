use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use tokio::sync::mpsc;

pub enum FsEvent {
    Changed(String), // path that changed
}

/// Watch a directory for file changes, send events on the channel.
pub fn watch_directory(
    dir: &Path,
    tx: mpsc::UnboundedSender<FsEvent>,
) -> Result<RecommendedWatcher> {
    let mut watcher = RecommendedWatcher::new(
        move |result: std::result::Result<Event, notify::Error>| {
            if let Ok(event) = result {
                for path in event.paths {
                    let ext = path.extension().and_then(|e| e.to_str());
                    if ext == Some("jsonl") || ext == Some("json") {
                        let _ = tx.send(FsEvent::Changed(path.to_string_lossy().to_string()));
                    }
                }
            }
        },
        Config::default(),
    )?;

    watcher.watch(dir, RecursiveMode::Recursive)?;
    Ok(watcher)
}
