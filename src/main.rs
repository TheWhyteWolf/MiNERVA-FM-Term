mod app;
mod audio;
mod cli;
mod config;
mod engine;
mod library;
mod playlist;
mod sidlen;
mod ui;

use anyhow::{bail, Context, Result};
use clap::Parser;
use cli::{Args, SubtunesArg};
use engine::{Format, SubtuneMode};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

/// Where to play from this run. Order: explicit paths (+ --set-dir, which
/// also persists) > saved default > interactive prompt (first run) > the
/// built-in demo tracks (empty list).
fn resolve_paths(args: &Args) -> Result<Vec<PathBuf>> {
    let mut paths = args.paths.clone();
    if let Some(dir) = &args.set_dir {
        if !dir.is_dir() {
            bail!("--set-dir: {} is not a directory", dir.display());
        }
        config::save_default_dir(dir)?;
        eprintln!("saved default music directory: {}", dir.display());
        paths.push(dir.clone());
    }
    if !paths.is_empty() {
        return Ok(paths);
    }
    if let Some(saved) = config::load_default_dir() {
        if saved.is_dir() {
            return Ok(vec![saved]);
        }
        eprintln!(
            "saved music directory {} no longer exists — let's pick a new one.",
            saved.display()
        );
    }
    prompt_for_dir()
}

/// First-run prompt (TTY only). Empty answer = built-in demo tracks.
fn prompt_for_dir() -> Result<Vec<PathBuf>> {
    if !std::io::stdin().is_terminal() {
        return Ok(Vec::new()); // non-interactive: demo tracks
    }
    eprintln!("MiNERVA-FM — going on air for the first time.");
    loop {
        eprint!("Where does your music live? Enter a directory (or press Enter for the built-in demo tracks): ");
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let answer = line.trim();
        if answer.is_empty() {
            return Ok(Vec::new());
        }
        let dir = config::expand_tilde(answer);
        if dir.is_dir() {
            config::save_default_dir(&dir)?;
            eprintln!("saved as your default music directory: {}", dir.display());
            return Ok(vec![dir]);
        }
        eprintln!("{} is not a directory, try again.", dir.display());
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(out) = &args.render {
        return render(&args, out);
    }

    let paths = resolve_paths(&args)?;
    let library = if paths.is_empty() {
        if args.no_samples {
            bail!("no music paths given and --no-samples set");
        }
        let lib = library::builtin();
        if lib.tracks.is_empty() {
            bail!("built without the \"samples\" feature — pass music paths");
        }
        lib
    } else {
        let lib = library::scan(&paths);
        if lib.tracks.is_empty() {
            bail!("no supported music files found under the given paths");
        }
        lib
    };

    if args.list {
        for t in &library.tracks {
            println!(
                "{:10} [{:?}] {} — {}",
                t.id,
                t.format,
                t.platform.as_deref().unwrap_or("-"),
                t.track
            );
        }
        println!(
            "{} track(s) ({} SID){}",
            library.tracks.len(),
            library.sid_count,
            library
                .songlengths
                .as_ref()
                .map(|p| format!(", Songlengths: {}", p.display()))
                .unwrap_or_default()
        );
        return Ok(());
    }

    app::run(&args, library)
}

/// Headless verification: decode one file to WAV and report what happened.
fn render(args: &Args, out: &std::path::Path) -> Result<()> {
    if args.paths.len() != 1 || !args.paths[0].is_file() {
        bail!("--render needs exactly one input file");
    }
    let path = &args.paths[0];
    let format = Format::from_path(path).context("unrecognised file extension")?;
    let cfg = audio::mixer::MixerConfig {
        sample_rate: 44100,
        subtune_mode: match args.subtunes {
            SubtunesArg::Random => SubtuneMode::Random,
            SubtunesArg::First => SubtuneMode::First,
        },
        spc_min_secs: args.spc_min,
        max_track_secs: args.max_track,
        volume: 1.0, // full scale so the reported peak is meaningful
        sid_db: args
            .songlengths
            .as_deref()
            .and_then(engine::sid::SidDb::open),
        apply_trim: !args.no_trim,
    };
    let (secs, peak) = audio::mixer::render_to_wav(
        library::TrackData::File(path.clone()),
        format,
        &cfg,
        args.seconds,
        out,
    )?;
    println!(
        "rendered {}: {:.1}s, peak {:.3} -> {}",
        path.display(),
        secs,
        peak,
        out.display()
    );
    if peak < 0.001 {
        bail!("output is silent — engine produced no audio");
    }
    Ok(())
}
