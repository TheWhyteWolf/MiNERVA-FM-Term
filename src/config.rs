//! Persistent settings: currently just the default music directory, stored
//! as a single line in $XDG_CONFIG_HOME/minerva-fm/default_dir.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("minerva-fm"));
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config").join("minerva-fm"))
}

pub fn load_default_dir() -> Option<PathBuf> {
    let path = config_dir()?.join("default_dir");
    let line = std::fs::read_to_string(path).ok()?;
    let dir = PathBuf::from(line.trim());
    (!dir.as_os_str().is_empty()).then_some(dir)
}

pub fn save_default_dir(dir: &Path) -> Result<()> {
    let cfg = config_dir().context("no HOME/XDG_CONFIG_HOME set")?;
    std::fs::create_dir_all(&cfg)?;
    std::fs::write(cfg.join("default_dir"), format!("{}\n", dir.display()))?;
    Ok(())
}

/// Expand a leading `~/` using $HOME (people will type it at the prompt).
pub fn expand_tilde(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(input)
}
