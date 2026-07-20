mod app;
mod audio;
mod cli;
mod engine;
mod library;
mod playlist;
mod sidlen;
mod ui;

use anyhow::{bail, Context, Result};
use clap::Parser;
use cli::{Args, SubtunesArg};
use engine::{Format, SubtuneMode};

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(out) = &args.render {
        return render(&args, out);
    }

    let library = if args.paths.is_empty() {
        if args.no_samples {
            bail!("no music paths given and --no-samples set");
        }
        let lib = library::builtin();
        if lib.tracks.is_empty() {
            bail!("built without the \"samples\" feature — pass music paths");
        }
        lib
    } else {
        let lib = library::scan(&args.paths);
        if lib.tracks.is_empty() {
            bail!("no supported music files found under the given paths");
        }
        lib
    };

    if args.list {
        for t in &library.tracks {
            let flag = if t.format.playable() { ' ' } else { '!' };
            println!(
                "{flag} {:10} [{:?}] {} — {}",
                t.id,
                t.format,
                t.platform.as_deref().unwrap_or("-"),
                t.track
            );
        }
        println!(
            "{} track(s), {} unplayable SID (v0.2)",
            library.tracks.len(),
            library.sid_count
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
