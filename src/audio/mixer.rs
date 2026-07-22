//! The mixer thread: owns the active engine, applies volume × fade envelope
//! and duration caps, feeds the audio ring and the FFT tap ring, and reports
//! track lifecycle events back to the app.

use super::{duration_policy, engine_trim, envelope_gain, Policy};
use crate::engine::{self, EngineParams, Format, RenderStatus, SubtuneMode, TrackTags};
use crate::library::TrackData;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use ringbuf::traits::{Observer, Producer};
use ringbuf::HeapProd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

const BLOCK_FRAMES: usize = 1024;

/// Clamp to [-1, 1], mapping any non-finite value (NaN/inf) to silence so a
/// single bad sample can't reach the device or permanently latch the spectrum.
#[inline]
fn sanitize(x: f32) -> f32 {
    if x.is_finite() {
        x.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Clamp a requested volume to [0, 1], treating a non-finite value (e.g.
/// `--volume NaN`) as 0 rather than letting NaN propagate into the mix.
#[inline]
fn sanitize_volume(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub struct PlayRequest {
    pub data: TrackData,
    pub format: Format,
    /// App-assigned id echoed back on Started/Finished/Error so the app can
    /// ignore lifecycle events from a track it has already skipped past.
    pub token: u64,
}

pub enum Control {
    Play(PlayRequest),
    TogglePause,
    SetVolume(f32),
    Shutdown,
}

pub enum Event {
    /// Engine loaded and playing; tags + resolved policy for the header pane.
    Started {
        tags: TrackTags,
        #[allow(dead_code)] // not displayed yet; useful for a duration readout later
        policy: Policy,
        token: u64,
    },
    /// Track ran out (natural end or cap+fade) — pick the next one.
    Finished { token: u64 },
    /// Track failed to load; skip it.
    Error { msg: String, token: u64 },
}

pub struct MixerConfig {
    pub sample_rate: u32,
    pub subtune_mode: SubtuneMode,
    pub spc_min_secs: u32,
    pub max_track_secs: u32,
    pub volume: f32,
    pub sid_db: Option<std::sync::Arc<crate::engine::sid::SidDb>>,
    /// Apply the calibrated per-engine gain trims (off with --no-trim).
    pub apply_trim: bool,
}

pub fn run(
    cfg: MixerConfig,
    control: Receiver<Control>,
    events: Sender<Event>,
    mut audio: HeapProd<f32>,
    mut fft_tap: HeapProd<f32>,
    flush_gen: Arc<AtomicU64>,
) {
    let mut engine: Option<Box<dyn engine::Engine>> = None;
    let mut frames: u64 = 0;
    let mut cap_frames: u64 = 0;
    let mut fade_frames: u64 = 0;
    let mut trim = 1.0f32;
    let mut volume = sanitize_volume(cfg.volume);
    let mut paused = false;
    let mut token = 0u64;
    let mut block = vec![0f32; BLOCK_FRAMES * 2];
    let mut mono = vec![0f32; BLOCK_FRAMES];

    loop {
        // Drain all pending control messages first.
        while let Ok(msg) = control.try_recv() {
            match msg {
                Control::Play(req) => {
                    token = req.token;
                    engine = None;
                    frames = 0;
                    // Drop any audio the output already holds from the previous
                    // track so a skip is heard immediately, not after the ring
                    // drains (~340 ms). The cpal callback clears on this bump.
                    flush_gen.fetch_add(1, Ordering::Relaxed);
                    trim = if cfg.apply_trim {
                        engine_trim(req.format)
                    } else {
                        1.0
                    };
                    match load(req, &cfg) {
                        Ok((e, policy)) => {
                            cap_frames = (policy.cap_secs * cfg.sample_rate as f64) as u64;
                            fade_frames = (policy.fade_secs * cfg.sample_rate as f64) as u64;
                            let tags = e.tags().clone();
                            engine = Some(e);
                            let _ = events.send(Event::Started { tags, policy, token });
                        }
                        Err(e) => {
                            let _ = events.send(Event::Error {
                                msg: e.to_string(),
                                token,
                            });
                        }
                    }
                }
                Control::TogglePause => paused = !paused,
                Control::SetVolume(v) => volume = sanitize_volume(v),
                Control::Shutdown => return,
            }
        }

        let free = audio.vacant_len();
        if free < BLOCK_FRAMES * 2 {
            std::thread::sleep(Duration::from_millis(4));
            continue;
        }

        match (&mut engine, paused) {
            (Some(e), false) => {
                let status = e.render(&mut block);
                for (i, frame) in block.chunks_exact_mut(2).enumerate() {
                    let gain =
                        volume * trim * envelope_gain(frames + i as u64, cap_frames, fade_frames);
                    frame[0] = sanitize(frame[0] * gain);
                    frame[1] = sanitize(frame[1] * gain);
                    mono[i] = (frame[0] + frame[1]) * 0.5;
                }
                frames += BLOCK_FRAMES as u64;
                audio.push_slice(&block);
                fft_tap.push_slice(&mono); // dropped samples are fine; UI drains fast

                if status == RenderStatus::Ended || frames >= cap_frames {
                    engine = None;
                    let _ = events.send(Event::Finished { token });
                }
            }
            _ => {
                // Paused or idle: feed silence to BOTH rings so cpal never
                // underruns loudly and the spectrum decays to flat instead of
                // freezing on the last frame.
                block.fill(0.0);
                audio.push_slice(&block);
                mono.fill(0.0);
                fft_tap.push_slice(&mono);
                std::thread::sleep(Duration::from_millis(4));
            }
        }
    }
}

fn load(req: PlayRequest, cfg: &MixerConfig) -> Result<(Box<dyn engine::Engine>, Policy)> {
    let data = req.data.load()?;
    let params = EngineParams {
        sample_rate: cfg.sample_rate,
        subtune_mode: cfg.subtune_mode,
        sid_db: cfg.sid_db.clone(),
    };
    let engine = engine::create(data, req.format, &params)?;
    let policy = duration_policy(
        req.format,
        engine.tags().duration_secs,
        cfg.spc_min_secs,
        cfg.max_track_secs,
    );
    Ok((engine, policy))
}

/// Headless verification mode: render one track straight to a WAV file.
/// Returns (seconds rendered, peak amplitude 0..1).
pub fn render_to_wav(
    data: TrackData,
    format: Format,
    cfg: &MixerConfig,
    seconds_limit: Option<f64>,
    out_path: &std::path::Path,
) -> Result<(f64, f32)> {
    let trim = if cfg.apply_trim { engine_trim(format) } else { 1.0 };
    let (mut engine, policy) = load(PlayRequest { data, format, token: 0 }, cfg)?;
    let cap_secs = seconds_limit.unwrap_or(policy.cap_secs).min(policy.cap_secs);
    let cap_frames = (cap_secs * cfg.sample_rate as f64) as u64;
    let fade_frames = (policy.fade_secs * cfg.sample_rate as f64) as u64;

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: cfg.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(out_path, spec)?;
    let mut block = vec![0f32; BLOCK_FRAMES * 2];
    let mut frames: u64 = 0;
    let mut peak = 0f32;

    loop {
        let status = engine.render(&mut block);
        for (i, frame) in block.chunks_exact(2).enumerate() {
            let gain =
                cfg.volume * trim * envelope_gain(frames + i as u64, cap_frames, fade_frames);
            for &s in frame {
                let v = sanitize(s * gain);
                peak = peak.max(v.abs());
                writer.write_sample((v * 32767.0) as i16)?;
            }
        }
        frames += BLOCK_FRAMES as u64;
        if status == RenderStatus::Ended || frames >= cap_frames {
            break;
        }
    }
    writer.finalize()?;
    Ok((frames as f64 / cfg.sample_rate as f64, peak))
}
