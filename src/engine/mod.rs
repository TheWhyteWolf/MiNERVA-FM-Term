pub mod gme;
mod gme_ffi;
pub mod openmpt;
pub mod sid;
pub mod symphonia;

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

/// What a render call produced. `Ended` means the buffer may be partially
/// filled (the rest is zeroed) and the engine has nothing more to give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStatus {
    Continue,
    Ended,
}

/// Tag metadata reported by an engine after loading a file.
#[derive(Debug, Clone, Default)]
pub struct TrackTags {
    pub title: Option<String>,
    pub game: Option<String>,
    pub system: Option<String>,
    pub author: Option<String>,
    /// Engine-reported natural duration in seconds, if the format carries one.
    pub duration_secs: Option<f64>,
    pub subtune: Option<(u32, u32)>, // (chosen 1-based, total)
}

/// A loaded, playing music engine. `render` fills interleaved stereo f32
/// frames at the sample rate the engine was created with.
pub trait Engine: Send {
    fn render(&mut self, out: &mut [f32]) -> RenderStatus;
    fn tags(&self) -> &TrackTags;
}

/// Playback format families, keyed off the file extension. The distinction
/// matters for engine dispatch and for the duration policy (SPC floor, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Spc,
    Vgm, // .vgm and .vgz
    GmeOther, // nsf/nsfe/gbs/ay/kss/hes/gym/sap
    Tracker,  // mod/xm/s3m/it
    Stream,   // mp3/flac/wav/ogg
    Sid,      // .sid via libsidplayfp / reSIDfp
}

impl Format {
    pub fn from_ext(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "spc" => Format::Spc,
            "vgm" | "vgz" => Format::Vgm,
            "nsf" | "nsfe" | "gbs" | "ay" | "kss" | "hes" | "gym" | "sap" => Format::GmeOther,
            "mod" | "xm" | "s3m" | "it" => Format::Tracker,
            "mp3" | "flac" | "wav" | "ogg" => Format::Stream,
            "sid" => Format::Sid,
            _ => return None,
        })
    }

    pub fn from_path(path: &Path) -> Option<Format> {
        Format::from_ext(path.extension()?.to_str()?)
    }

    /// Display name used when the file carries no system tag.
    pub fn default_system(self) -> &'static str {
        match self {
            Format::Spc => "Super Nintendo",
            Format::Vgm => "VGM",
            Format::GmeOther => "Chiptune",
            Format::Tracker => "Tracker",
            Format::Stream => "Audio",
            Format::Sid => "Commodore 64",
        }
    }
}

/// How to choose among subtunes in multi-track files (NSF, GBS, KSS...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtuneMode {
    Random,
    First,
}

pub struct EngineParams {
    pub sample_rate: u32,
    pub subtune_mode: SubtuneMode,
    /// HVSC Songlengths database, when one was found or given.
    pub sid_db: Option<Arc<sid::SidDb>>,
}

/// Construct the right engine for `format` from raw file bytes.
pub fn create(data: Vec<u8>, format: Format, params: &EngineParams) -> Result<Box<dyn Engine>> {
    match format {
        Format::Spc | Format::Vgm | Format::GmeOther => {
            Ok(Box::new(gme::GmeEngine::new(data, params)?))
        }
        Format::Tracker => Ok(Box::new(openmpt::OpenMptEngine::new(data, params.sample_rate)?)),
        Format::Stream => Ok(Box::new(symphonia::SymphoniaEngine::new(data, params.sample_rate)?)),
        Format::Sid => Ok(Box::new(sid::SidEngine::new(
            data,
            params.sample_rate,
            params.subtune_mode,
            params.sid_db.as_ref(),
        )?)),
    }
}
