//! The interactive radio: terminal lifecycle, key handling, and the glue
//! between the mixer thread and the UI.

use crate::audio::mixer::{self, Control, Event as MixerEvent, MixerConfig, PlayRequest};
use crate::audio::output::AudioOutput;
use crate::cli::{Args, ColorsArg, OrderArg, SubtunesArg};
use crate::engine::SubtuneMode;
use crate::library::Library;
use crate::playlist::{Order, Playlist};
use crate::ui::nowplaying::{HeaderState, NowPlaying};
use crate::ui::schemes::{self, ColorMode, CHARS, SCHEMES};
use crate::ui::spectrum::Spectrum;
use crate::ui::ticker::Ticker;
use crate::ui::{self, View};
use anyhow::{bail, Result};
use crossbeam_channel::unbounded;
use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use rand::Rng;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use ringbuf::traits::{Consumer, Split};
use ringbuf::HeapRb;
use std::io::stdout;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(33);

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = crossterm::execute!(stdout(), LeaveAlternateScreen);
}

pub fn run(args: &Args, library: Library) -> Result<()> {
    let mut playlist = Playlist::new(
        &library,
        match args.order {
            OrderArg::Shuffle => Order::Shuffle,
            OrderArg::Sequential => Order::Sequential,
        },
    );
    if playlist.is_empty() {
        bail!("no playable tracks found");
    }

    // SID durations: explicit --songlengths wins over the scan's discovery.
    let sid_db = args
        .songlengths
        .clone()
        .or_else(|| library.songlengths.clone())
        .and_then(|p| crate::engine::sid::SidDb::open(&p));

    // Audio ring (~1/3 s at 48 kHz stereo) and the spectrum tap.
    let (audio_prod, audio_cons) = HeapRb::<f32>::new(32768).split();
    let (fft_prod, mut fft_cons) = HeapRb::<f32>::new(16384).split();
    let output = AudioOutput::new(audio_cons)?;

    let cfg = MixerConfig {
        sample_rate: output.sample_rate,
        subtune_mode: match args.subtunes {
            SubtunesArg::Random => SubtuneMode::Random,
            SubtunesArg::First => SubtuneMode::First,
        },
        spc_min_secs: args.spc_min,
        max_track_secs: args.max_track,
        volume: args.volume.clamp(0.0, 1.0),
        sid_db,
    };
    let (ctl_tx, ctl_rx) = unbounded::<Control>();
    let (ev_tx, ev_rx) = unbounded::<MixerEvent>();
    let mixer_handle = std::thread::spawn(move || mixer::run(cfg, ctl_rx, ev_tx, audio_prod, fft_prod));

    // Terminal up — restore on panic as well as on clean exit.
    enable_raw_mode()?;
    crossterm::execute!(stdout(), EnterAlternateScreen)?;
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        orig_hook(info);
    }));
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // UI state.
    let mode = match args.colors {
        ColorsArg::True => ColorMode::True,
        ColorsArg::C256 => ColorMode::C256,
        ColorsArg::Auto => ColorMode::detect(),
    };
    let locked_scheme = args.scheme.as_deref().and_then(schemes::scheme_index);
    if args.scheme.is_some() && locked_scheme.is_none() {
        restore_terminal();
        bail!("unknown scheme {:?}", args.scheme.as_deref().unwrap());
    }
    let locked_char = args.spectrum_char;
    let mut scheme_idx = locked_scheme.unwrap_or(0);
    let mut char_idx = 0usize;
    let mut spectrum = Spectrum::new();
    let mut ticker = Ticker::new();
    let mut now_playing: Option<NowPlaying> = None;
    let mut paused = false;
    let mut volume = args.volume.clamp(0.0, 1.0);
    let mut status: Option<String> = None;
    let mut consecutive_errors = 0usize;
    let mut fft_buf = vec![0f32; 4096];

    // Kick off the first track; `pending` is the entry the mixer is loading.
    let mut pending = playlist.next().map(|i| library.tracks[i].clone());
    let mut next_play_at: Option<Instant> = None;
    if let Some(entry) = &pending {
        ctl_tx.send(Control::Play(PlayRequest {
            data: entry.data.clone(),
            format: entry.format,
        }))?;
    }

    let result: Result<()> = (|| {
        loop {
            // ---- input -------------------------------------------------
            if event::poll(TICK)? {
                if let TermEvent::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
                        continue;
                    }
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            break;
                        }
                        (KeyCode::Char('n'), _) => {
                            next_play_at = Some(Instant::now());
                        }
                        (KeyCode::Char(' '), _) => {
                            paused = !paused;
                            ctl_tx.send(Control::TogglePause)?;
                        }
                        (KeyCode::Char('c'), _) => {
                            if locked_scheme.is_none() {
                                scheme_idx = (scheme_idx + 1) % SCHEMES.len();
                                if let Some(np) = &mut now_playing {
                                    np.scheme_name = SCHEMES[scheme_idx].name.into();
                                }
                            }
                        }
                        (KeyCode::Char('x'), _) => {
                            if locked_char.is_none() {
                                char_idx = (char_idx + 1) % CHARS.len();
                            }
                        }
                        (KeyCode::Up, _) => {
                            volume = (volume + 0.05).min(1.0);
                            ctl_tx.send(Control::SetVolume(volume))?;
                        }
                        (KeyCode::Down, _) => {
                            volume = (volume - 0.05).max(0.0);
                            ctl_tx.send(Control::SetVolume(volume))?;
                        }
                        _ => {}
                    }
                }
            }

            // ---- mixer events -------------------------------------------
            while let Ok(ev) = ev_rx.try_recv() {
                match ev {
                    MixerEvent::Started { tags, policy: _ } => {
                        consecutive_errors = 0;
                        status = None;
                        if locked_scheme.is_none() {
                            scheme_idx = rand::rng().random_range(0..SCHEMES.len());
                        }
                        if locked_char.is_none() {
                            char_idx = rand::rng().random_range(0..CHARS.len());
                        }
                        if let Some(entry) = &pending {
                            now_playing = Some(NowPlaying {
                                id: entry.id.clone(),
                                source: match args.order {
                                    OrderArg::Shuffle => "RANDOM".into(),
                                    OrderArg::Sequential => "LOCAL".into(),
                                },
                                system: tags
                                    .system
                                    .clone()
                                    .or_else(|| entry.platform.clone())
                                    .or_else(|| Some(entry.format.default_system().into())),
                                game: tags.game.clone().unwrap_or_else(|| entry.game.clone()),
                                track: tags.title.clone().unwrap_or_else(|| entry.track.clone()),
                                author: tags.author.clone(),
                                scheme_name: SCHEMES[scheme_idx].name.into(),
                                subtune: tags.subtune,
                            });
                        }
                    }
                    MixerEvent::Finished => {
                        next_play_at = Some(Instant::now());
                    }
                    MixerEvent::Error(e) => {
                        consecutive_errors += 1;
                        status = Some(format!("skipped: {e}"));
                        if consecutive_errors > 50 {
                            status = Some("too many consecutive failures — stopped".into());
                        } else {
                            next_play_at = Some(Instant::now() + Duration::from_secs(1));
                        }
                    }
                }
            }

            // ---- advance to the next track ------------------------------
            if next_play_at.is_some_and(|t| Instant::now() >= t) {
                next_play_at = None;
                if let Some(idx) = playlist.next() {
                    let entry = library.tracks[idx].clone();
                    ctl_tx.send(Control::Play(PlayRequest {
                        data: entry.data.clone(),
                        format: entry.format,
                    }))?;
                    pending = Some(entry);
                }
            }

            // ---- spectrum + ticker --------------------------------------
            loop {
                let n = fft_cons.pop_slice(&mut fft_buf);
                if n == 0 {
                    break;
                }
                spectrum.feed(&fft_buf[..n]);
            }
            spectrum.update();
            ticker.update(Instant::now());

            // ---- draw ---------------------------------------------------
            let width = terminal.size()?.width as usize;
            let view = View {
                header: HeaderState {
                    now: now_playing.as_ref(),
                    paused,
                    volume,
                    status: status.as_deref(),
                },
                bars: spectrum.bars(),
                scheme_idx,
                spectrum_char: locked_char.unwrap_or(CHARS[char_idx]),
                ticker_line: ticker.line(width),
                ticker_colour: ticker.colour,
                mode,
            };
            terminal.draw(|f| ui::draw(f, &view))?;
        }
        Ok(())
    })();

    let _ = ctl_tx.send(Control::Shutdown);
    let _ = mixer_handle.join();
    drop(output);
    restore_terminal();
    result
}
