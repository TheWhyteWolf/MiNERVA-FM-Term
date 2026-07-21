//! The mixer thread: owns the active engine, applies volume × fade envelope
//! and duration caps, feeds the audio ring and the FFT tap ring, and reports
//! track lifecycle events back to the app.

use super::{duration_policy, envelope_gain, Policy};
use crate::engine::{self, EngineParams, Format, RenderStatus, SubtuneMode, TrackTags};
use crate::library::TrackData;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use ringbuf::traits::{Observer, Producer};
use ringbuf::HeapProd;
use std::time::Duration;

const BLOCK_FRAMES: usize = 1024;

pub struct PlayRequest {
    pub data: TrackData,
    pub format: Format,
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
    },
    /// Track ran out (natural end or cap+fade) — pick the next one.
    Finished,
    /// Track failed to load; skip it.
    Error(String),
}

pub struct MixerConfig {
    pub sample_rate: u32,
    pub subtune_mode: SubtuneMode,
    pub spc_min_secs: u32,
    pub max_track_secs: u32,
    pub volume: f32,
    pub sid_db: Option<std::sync::Arc<crate::engine::sid::SidDb>>,
}

pub fn run(
    cfg: MixerConfig,
    control: Receiver<Control>,
    events: Sender<Event>,
    mut audio: HeapProd<f32>,
    mut fft_tap: HeapProd<f32>,
) {
    let mut engine: Option<Box<dyn engine::Engine>> = None;
    let mut frames: u64 = 0;
    let mut cap_frames: u64 = 0;
    let mut fade_frames: u64 = 0;
    let mut volume = cfg.volume.clamp(0.0, 1.0);
    let mut paused = false;
    let mut block = vec![0f32; BLOCK_FRAMES * 2];
    let mut mono = vec![0f32; BLOCK_FRAMES];

    loop {
        // Drain all pending control messages first.
        while let Ok(msg) = control.try_recv() {
            match msg {
                Control::Play(req) => {
                    engine = None;
                    frames = 0;
                    match load(req, &cfg) {
                        Ok((e, policy)) => {
                            cap_frames = (policy.cap_secs * cfg.sample_rate as f64) as u64;
                            fade_frames = (policy.fade_secs * cfg.sample_rate as f64) as u64;
                            let tags = e.tags().clone();
                            engine = Some(e);
                            let _ = events.send(Event::Started { tags, policy });
                        }
                        Err(e) => {
                            let _ = events.send(Event::Error(e.to_string()));
                        }
                    }
                }
                Control::TogglePause => paused = !paused,
                Control::SetVolume(v) => volume = v.clamp(0.0, 1.0),
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
                    let gain = volume * envelope_gain(frames + i as u64, cap_frames, fade_frames);
                    frame[0] *= gain;
                    frame[1] *= gain;
                    mono[i] = (frame[0] + frame[1]) * 0.5;
                }
                frames += BLOCK_FRAMES as u64;
                audio.push_slice(&block);
                fft_tap.push_slice(&mono); // dropped samples are fine; UI drains fast

                if status == RenderStatus::Ended || frames >= cap_frames {
                    engine = None;
                    let _ = events.send(Event::Finished);
                }
            }
            _ => {
                // Paused or idle: keep the stream fed with silence so the
                // spectrum decays naturally and cpal never underruns loudly.
                block.fill(0.0);
                audio.push_slice(&block);
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
    let (mut engine, policy) = load(PlayRequest { data, format }, cfg)?;
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
            let gain = cfg.volume * envelope_gain(frames + i as u64, cap_frames, fade_frames);
            for &s in frame {
                let v = (s * gain).clamp(-1.0, 1.0);
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
