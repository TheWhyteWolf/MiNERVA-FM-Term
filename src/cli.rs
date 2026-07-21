use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OrderArg {
    Shuffle,
    Sequential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SubtunesArg {
    /// Pick a random subtune of multi-track files (NSF, GBS, ...) per play.
    Random,
    /// Always play the first subtune.
    First,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorsArg {
    /// Truecolor when COLORTERM advertises it, 256-colour otherwise.
    Auto,
    True,
    #[value(name = "256")]
    C256,
}

/// MiNERVA-FM — retro video game music radio for your terminal.
///
/// Point it at directories or files of game music (.spc .vgm .vgz .nsf .nsfe
/// .gbs .ay .kss .hes .gym .sap .mod .xm .s3m .it .mp3 .flac .wav .ogg) and
/// let it play. With no arguments the built-in demo tracks go on air.
#[derive(Parser, Debug)]
#[command(name = "minerva-fm", version, about)]
pub struct Args {
    /// Music directories and/or files to scan.
    pub paths: Vec<PathBuf>,

    /// Master volume, 0.0–1.0.
    #[arg(long, default_value_t = 0.7)]
    pub volume: f32,

    /// Track selection order.
    #[arg(long, value_enum, default_value_t = OrderArg::Shuffle)]
    pub order: OrderArg,

    /// Subtune selection for multi-track files.
    #[arg(long, value_enum, default_value_t = SubtunesArg::Random)]
    pub subtunes: SubtunesArg,

    /// Lock the colour scheme (gameboy, gameboy_pocket, bbc_micro, pico8,
    /// cga, nes, minerva, c64, zx_spectrum) instead of rerolling per track.
    #[arg(long)]
    pub scheme: Option<String>,

    /// Lock the spectrum character instead of rerolling per track.
    #[arg(long = "char")]
    pub spectrum_char: Option<char>,

    /// Colour depth.
    #[arg(long, value_enum, default_value_t = ColorsArg::Auto)]
    pub colors: ColorsArg,

    /// Hard cap on any track, in seconds (guards infinite loop flags).
    #[arg(long, default_value_t = 900)]
    pub max_track: u32,

    /// Minimum play time for .spc tracks, in seconds (many rips carry tiny
    /// ID666 length tags; the emulated audio loops, so play more of it).
    #[arg(long, default_value_t = 60)]
    pub spc_min: u32,

    /// HVSC Songlengths.md5 database for SID durations (auto-discovered
    /// during the scan when a file with that name is present).
    #[arg(long, value_name = "FILE")]
    pub songlengths: Option<PathBuf>,

    /// Disable the calibrated per-engine gain trims (raw engine levels).
    #[arg(long)]
    pub no_trim: bool,

    /// Do not fall back to the built-in demo tracks.
    #[arg(long)]
    pub no_samples: bool,

    /// Headless: render one input file to a WAV instead of playing (debug).
    #[arg(long, value_name = "OUT.wav")]
    pub render: Option<PathBuf>,

    /// Limit --render output length, in seconds.
    #[arg(long)]
    pub seconds: Option<f64>,

    /// Print the scanned library and exit.
    #[arg(long)]
    pub list: bool,
}
