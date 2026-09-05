//! Persistent settings: currently just the default music directory, stored
//! as a single line in $XDG_CONFIG_HOME/minerva-fm/default_dir.
//!
//! Paths are read and written as raw bytes rather than text. A directory name
//! is not required to be UTF-8, and round-tripping one through `Display` or
//! `to_string_lossy` would replace the offending bytes with U+FFFD — saving a
//! path that no longer exists and re-prompting on every launch.

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

fn config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("minerva-fm"));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("minerva-fm"))
}

/// The saved path is everything before the first newline, byte for byte —
/// a directory name may legally begin or end with a space, so nothing is
/// trimmed beyond the line ending we wrote ourselves.
fn parse_saved(bytes: &[u8]) -> Option<PathBuf> {
    let line = bytes.split(|&b| b == b'\n').next().unwrap_or_default();
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    (!line.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(line)))
}

pub fn load_default_dir() -> Option<PathBuf> {
    let path = config_dir()?.join("default_dir");
    parse_saved(&std::fs::read(path).ok()?)
}

pub fn save_default_dir(dir: &Path) -> Result<()> {
    let cfg = config_dir().context("no HOME/XDG_CONFIG_HOME set")?;
    std::fs::create_dir_all(&cfg)?;
    let mut line = dir.as_os_str().as_bytes().to_vec();
    line.push(b'\n');
    std::fs::write(cfg.join("default_dir"), line)?;
    Ok(())
}

/// Expand a leading `~/` using $HOME (people will type it at the prompt).
pub fn expand_tilde(input: &OsStr) -> PathBuf {
    if let Some(rest) = input.as_bytes().strip_prefix(b"~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(OsStr::from_bytes(rest));
        }
    }
    PathBuf::from(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn saved_path_survives_non_utf8_and_spaces() {
        // Latin-1 "café" plus a trailing space, as a directory name would be
        // stored on disk. Neither may be mangled or trimmed away.
        let raw = b"/music/caf\xe9 ";
        let mut stored = raw.to_vec();
        stored.push(b'\n');
        let parsed = parse_saved(&stored).expect("non-empty");
        assert_eq!(parsed.as_os_str().as_bytes(), raw);
    }

    #[test]
    fn saved_path_stops_at_first_newline() {
        assert_eq!(parse_saved(b"/a/b\n/c/d\n"), Some(PathBuf::from("/a/b")));
        assert_eq!(parse_saved(b"/a/b\r\n"), Some(PathBuf::from("/a/b")));
        assert_eq!(parse_saved(b"\n"), None);
        assert_eq!(parse_saved(b""), None);
    }

    #[test]
    fn expand_tilde_keeps_non_utf8_bytes() {
        let raw = OsString::from_vec(b"/music/caf\xe9".to_vec());
        assert_eq!(expand_tilde(&raw), PathBuf::from(&raw));
    }
}
