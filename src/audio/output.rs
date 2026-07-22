//! cpal output stream: the callback only pops frames from the SPSC ring and
//! zero-fills on underrun — real-time safe, no locks, no allocation.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use ringbuf::traits::Consumer;
use ringbuf::HeapCons;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct AudioOutput {
    pub sample_rate: u32,
    // Held so the stream keeps playing; dropped on shutdown.
    _stream: cpal::Stream,
}

impl AudioOutput {
    /// Open the default output device and start pulling stereo f32 frames
    /// from `cons`. The device's own rate/channel count are adapted to.
    /// `flush_gen` is bumped by the mixer on a skip; the callback drains the
    /// ring when it changes so stale audio is dropped immediately.
    pub fn new(cons: HeapCons<f32>, flush_gen: Arc<AtomicU64>) -> Result<AudioOutput> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output device found"))?;
        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let stream_config: cpal::StreamConfig = config.clone().into();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                build::<f32>(&device, &stream_config, channels, cons, flush_gen)?
            }
            cpal::SampleFormat::I16 => {
                build::<i16>(&device, &stream_config, channels, cons, flush_gen)?
            }
            cpal::SampleFormat::U16 => {
                build::<u16>(&device, &stream_config, channels, cons, flush_gen)?
            }
            other => return Err(anyhow!("unsupported sample format {other:?}")),
        };
        stream.play()?;

        Ok(AudioOutput {
            sample_rate,
            _stream: stream,
        })
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut cons: HeapCons<f32>,
    flush_gen: Arc<AtomicU64>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let mut last_gen = flush_gen.load(Ordering::Relaxed);
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            // A skip bumps flush_gen: drop whatever's still queued so the new
            // track starts now instead of after the old audio drains.
            let gen = flush_gen.load(Ordering::Relaxed);
            if gen != last_gen {
                cons.clear();
                last_gen = gen;
            }
            for frame in data.chunks_mut(channels) {
                let l = cons.try_pop().unwrap_or(0.0);
                let r = cons.try_pop().unwrap_or(0.0);
                match channels {
                    1 => frame[0] = T::from_sample((l + r) * 0.5),
                    _ => {
                        frame[0] = T::from_sample(l);
                        frame[1] = T::from_sample(r);
                        for s in frame.iter_mut().skip(2) {
                            *s = T::from_sample(0.0);
                        }
                    }
                }
            }
        },
        |e| eprintln!("audio stream error: {e}"),
        None,
    )?;
    Ok(stream)
}
