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
    /// `flush_to` is the running total of samples the mixer had written when
    /// it last skipped; the callback discards up to exactly that watermark,
    /// so stale audio goes and the new track's audio stays.
    pub fn new(cons: HeapCons<f32>, flush_to: Arc<AtomicU64>) -> Result<AudioOutput> {
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
                build::<f32>(&device, &stream_config, channels, cons, flush_to)?
            }
            cpal::SampleFormat::I16 => {
                build::<i16>(&device, &stream_config, channels, cons, flush_to)?
            }
            cpal::SampleFormat::U16 => {
                build::<u16>(&device, &stream_config, channels, cons, flush_to)?
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

/// Discard the audio the mixer queued before its last skip, and nothing more.
///
/// `read_total` is what this consumer has taken so far; `target` is what the
/// mixer had written at the moment it skipped. Everything below `target` is
/// the outgoing track's. Returns the updated `read_total`.
#[inline]
fn drop_stale(cons: &mut HeapCons<f32>, read_total: u64, target: u64) -> u64 {
    if target > read_total {
        read_total + cons.skip((target - read_total) as usize) as u64
    } else {
        read_total
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut cons: HeapCons<f32>,
    flush_to: Arc<AtomicU64>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    // Samples actually popped since the stream opened, measured against the
    // mixer's write watermark. Both counters start at zero.
    let mut read_total: u64 = 0;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            // A skip publishes the write count from *before* the new track, so
            // everything below it is the previous track's and only that much is
            // dropped. Clearing the whole ring instead would also swallow the
            // new track's audio, which the mixer has usually queued by now.
            read_total = drop_stale(&mut cons, read_total, flush_to.load(Ordering::Acquire));
            for frame in data.chunks_mut(channels) {
                let (l, r) = (cons.try_pop(), cons.try_pop());
                // An underrun zero-fills but consumes nothing, so only real
                // pops advance the counter.
                read_total += l.is_some() as u64 + r.is_some() as u64;
                let (l, r) = (l.unwrap_or(0.0), r.unwrap_or(0.0));
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

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Observer, Producer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn flush_drops_only_the_outgoing_track() {
        let (mut prod, mut cons) = HeapRb::<f32>::new(64).split();
        // Old track: 10 samples queued, 4 of them already played out.
        prod.push_slice(&[1.0; 10]);
        let mut read_total = cons.skip(4) as u64;

        // The mixer skips here, having written 10 samples in total, and then
        // immediately queues the new track — which is the race the old
        // generation counter lost.
        let target = 10u64;
        prod.push_slice(&[2.0; 8]);

        read_total = drop_stale(&mut cons, read_total, target);
        assert_eq!(read_total, 10, "should have consumed exactly the old track");
        assert_eq!(cons.occupied_len(), 8, "the new track must survive intact");
        assert_eq!(cons.try_pop(), Some(2.0));
    }

    #[test]
    fn flush_is_idempotent_and_never_rewinds() {
        let (mut prod, mut cons) = HeapRb::<f32>::new(64).split();
        prod.push_slice(&[1.0; 4]);
        let read_total = drop_stale(&mut cons, 0, 4);
        assert_eq!(read_total, 4);
        // Same watermark seen again on the next callback: nothing more to drop.
        prod.push_slice(&[2.0; 3]);
        assert_eq!(drop_stale(&mut cons, read_total, 4), 4);
        assert_eq!(cons.occupied_len(), 3);
        // A stale target below read_total is ignored rather than underflowing.
        assert_eq!(drop_stale(&mut cons, read_total, 1), 4);
        assert_eq!(cons.occupied_len(), 3);
    }

    #[test]
    fn flush_tolerates_a_target_the_ring_cannot_reach_yet() {
        let (mut prod, mut cons) = HeapRb::<f32>::new(64).split();
        prod.push_slice(&[1.0; 2]);
        // Ask to drop past what is queued: skip takes what it can and the
        // remainder is dropped on a later callback, never double-counted.
        let read_total = drop_stale(&mut cons, 0, 5);
        assert_eq!(read_total, 2);
        assert_eq!(cons.occupied_len(), 0);
        prod.push_slice(&[1.0; 4]);
        assert_eq!(drop_stale(&mut cons, read_total, 5), 5);
        assert_eq!(cons.occupied_len(), 1);
    }
}
