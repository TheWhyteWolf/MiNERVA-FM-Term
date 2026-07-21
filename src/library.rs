//! Track discovery: recursive directory scan plus the optional built-in
//! sample tracks, with path-derived display metadata (the same heuristics as
//! the web edition's entriesFromFiles: parent dir = game, underscores→spaces).

use crate::engine::Format;
use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Clone)]
pub enum TrackData {
    File(PathBuf),
    Builtin(&'static [u8]),
}

impl TrackData {
    pub fn load(&self) -> Result<Vec<u8>> {
        match self {
            TrackData::File(p) => Ok(std::fs::read(p)?),
            TrackData::Builtin(b) => Ok(b.to_vec()),
        }
    }
}

#[derive(Clone)]
pub struct TrackEntry {
    pub id: String,
    pub data: TrackData,
    pub format: Format,
    pub game: String,
    pub track: String,
    pub platform: Option<String>,
}

pub struct Library {
    pub tracks: Vec<TrackEntry>,
    pub sid_count: usize,
    /// HVSC Songlengths.md5 spotted during the scan, if any.
    pub songlengths: Option<PathBuf>,
}

fn prettify(s: &str) -> String {
    s.replace('_', " ").trim().to_string()
}

fn dir_name(p: &Path) -> Option<String> {
    p.file_name().map(|n| prettify(&n.to_string_lossy()))
}

fn entry_for(path: &Path, root: &Path, format: Format) -> TrackEntry {
    let track = path
        .file_stem()
        .map(|s| prettify(&s.to_string_lossy()))
        .unwrap_or_else(|| "unknown".into());
    let parent = path.parent();
    let game = parent
        .filter(|p| *p != root)
        .and_then(dir_name)
        .unwrap_or_else(|| track.clone());
    // Grandparent directory as the platform, when it sits inside the scan
    // root (matches the catalogue layout TLD/PLATFORM/GAME/track).
    let platform = parent
        .and_then(|p| p.parent())
        .filter(|gp| gp.starts_with(root) && *gp != root)
        .and_then(dir_name);
    TrackEntry {
        id: String::new(), // assigned after the full scan
        data: TrackData::File(path.to_path_buf()),
        format,
        game,
        track,
        platform,
    }
}

pub fn scan(paths: &[PathBuf]) -> Library {
    let mut tracks = Vec::new();
    let mut songlengths = None;
    for arg in paths {
        if arg.is_file() {
            if let Some(fmt) = Format::from_path(arg) {
                let root = arg.parent().unwrap_or(Path::new("."));
                tracks.push(entry_for(arg, root, fmt));
            }
            continue;
        }
        for e in WalkDir::new(arg).follow_links(true).into_iter().flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            if let Some(fmt) = Format::from_path(e.path()) {
                tracks.push(entry_for(e.path(), arg, fmt));
            } else if songlengths.is_none()
                && e.file_name().eq_ignore_ascii_case("Songlengths.md5")
            {
                songlengths = Some(e.path().to_path_buf());
            }
        }
    }
    for (i, t) in tracks.iter_mut().enumerate() {
        t.id = format!("#{:04}", i + 1);
    }
    let sid_count = tracks.iter().filter(|t| t.format == Format::Sid).count();
    Library { tracks, sid_count, songlengths }
}

/// The nine tracks bundled with the web edition, embedded so the radio plays
/// out of the box when launched with no arguments.
#[cfg(feature = "samples")]
pub fn builtin() -> Library {
    macro_rules! sample {
        ($file:literal, $fmt:expr, $id:literal, $platform:literal, $game:literal, $track:literal) => {
            TrackEntry {
                id: $id.into(),
                data: TrackData::Builtin(include_bytes!(concat!("../assets/samples/", $file))),
                format: $fmt,
                game: $game.into(),
                track: $track.into(),
                platform: Some($platform.into()),
            }
        };
    }
    let tracks = vec![
        sample!("101-2780-1_Final_Fantasy_7_Racing_Chocobos.sid", Format::Sid, "101-2780-1", "DEMOS", "Final Fantasy 7 Racing Chocobos", "Racing Chocobos"),
        sample!("101-3212-1_Last_Ninja_Solution_tune_1.sid", Format::Sid, "101-3212-1", "DEMOS", "Last Ninja Solution", "tune 1"),
        sample!("101-3427-1_Monty_Python_Demo.sid", Format::Sid, "101-3427-1", "DEMOS", "Monty Python Demo", "Monty Python Demo"),
        sample!("116-9061-1_Africa.spc", Format::Spc, "116-9061-1", "SNES", "Aerobiz Supersonic", "Africa"),
        sample!("116-9207-1_Confronting_Ganon.spc", Format::Spc, "116-9207-1", "SNES", "BS Zelda", "Confronting Ganon"),
        sample!("116-9221-1_Aquarium.spc", Format::Spc, "116-9221-1", "SNES", "Captain Commando", "Aquarium"),
        sample!("vgzsmp0_01_-_Title.vgz", Format::Vgm, "vgz-0", "MegaDrive", "Sonic 3D Blast", "01 - Title"),
        sample!("vgzsmp1_01_-_Title_Theme.vgz", Format::Vgm, "vgz-1", "MegaDrive", "16t", "01 - Title Theme"),
        sample!("vgzsmp2_01_Game_Start.vgz", Format::Vgm, "vgz-2", "NES", "10-Yard Fight", "01 Game Start"),
    ];
    let sid_count = tracks.iter().filter(|t| t.format == Format::Sid).count();
    Library { tracks, sid_count, songlengths: None }
}

#[cfg(not(feature = "samples"))]
pub fn builtin() -> Library {
    Library { tracks: Vec::new(), sid_count: 0, songlengths: None }
}
