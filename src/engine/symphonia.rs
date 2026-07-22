//! Streamed audio formats (.mp3 .flac .wav .ogg) via symphonia, resampled to
//! the pipeline rate with rubato when the file rate differs.

use super::{Engine, RenderStatus, TrackTags};
use anyhow::{anyhow, Result};
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use std::collections::VecDeque;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTagKey};
use symphonia::core::probe::Hint;

const CHUNK: usize = 1024; // resampler input frames per process call

pub struct SymphoniaEngine {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    resampler: Option<FastFixedIn<f32>>,
    /// Deinterleaved stereo accumulation waiting to fill a resampler chunk.
    acc: [Vec<f32>; 2],
    /// Interleaved stereo output at the pipeline rate, ready to hand out.
    pending: VecDeque<f32>,
    eof: bool,
    tags: TrackTags,
}

impl SymphoniaEngine {
    pub fn new(data: Vec<u8>, sample_rate: u32) -> Result<SymphoniaEngine> {
        let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(data)), Default::default());
        let probed = symphonia::default::get_probe().format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        let mut format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow!("no decodable audio track"))?;
        let track_id = track.id;
        let params = track.codec_params.clone();
        let in_rate = params.sample_rate.ok_or_else(|| anyhow!("unknown sample rate"))?;
        let decoder =
            symphonia::default::get_codecs().make(&params, &DecoderOptions::default())?;

        let mut tags = TrackTags {
            duration_secs: params
                .n_frames
                .map(|n| n as f64 / in_rate as f64),
            ..TrackTags::default()
        };
        if let Some(rev) = format.metadata().current() {
            for tag in rev.tags() {
                match tag.std_key {
                    Some(StandardTagKey::TrackTitle) => tags.title = Some(tag.value.to_string()),
                    Some(StandardTagKey::Artist) => tags.author = Some(tag.value.to_string()),
                    Some(StandardTagKey::Album) => tags.game = Some(tag.value.to_string()),
                    _ => {}
                }
            }
        }

        let resampler = if in_rate != sample_rate {
            Some(FastFixedIn::<f32>::new(
                sample_rate as f64 / in_rate as f64,
                1.1,
                PolynomialDegree::Cubic,
                CHUNK,
                2,
            )?)
        } else {
            None
        };

        Ok(SymphoniaEngine {
            format,
            decoder,
            track_id,
            resampler,
            acc: [Vec::new(), Vec::new()],
            pending: VecDeque::new(),
            eof: false,
            tags,
        })
    }

    /// Decode one packet into the deinterleaved stereo accumulator.
    /// Returns false at end of stream.
    fn decode_packet(&mut self) -> bool {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(_) => return false, // EOF or unrecoverable
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let mut buf =
                        SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    buf.copy_interleaved_ref(decoded);
                    // Use the decoded packet's own channel count, not the
                    // container probe's, in case they disagree.
                    let ch = spec.channels.count().max(1);
                    for frame in buf.samples().chunks_exact(ch) {
                        let l = frame[0];
                        let r = if ch > 1 { frame[1] } else { frame[0] };
                        self.acc[0].push(l);
                        self.acc[1].push(r);
                    }
                    return true;
                }
                Err(SymError::DecodeError(_)) => continue, // skip bad packet
                Err(_) => return false,
            }
        }
    }

    /// Move accumulated frames through the resampler (or straight through)
    /// into `pending`. `flush` drains a final partial chunk at EOF.
    fn drain_acc(&mut self, flush: bool) {
        match &mut self.resampler {
            None => {
                for i in 0..self.acc[0].len() {
                    self.pending.push_back(self.acc[0][i]);
                    self.pending.push_back(self.acc[1][i]);
                }
                self.acc[0].clear();
                self.acc[1].clear();
            }
            Some(rs) => {
                while self.acc[0].len() >= CHUNK {
                    let input = [
                        self.acc[0][..CHUNK].to_vec(),
                        self.acc[1][..CHUNK].to_vec(),
                    ];
                    self.acc[0].drain(..CHUNK);
                    self.acc[1].drain(..CHUNK);
                    if let Ok(out) = rs.process(&input, None) {
                        for i in 0..out[0].len() {
                            self.pending.push_back(out[0][i]);
                            self.pending.push_back(out[1][i]);
                        }
                    }
                }
                if flush && !self.acc[0].is_empty() {
                    let input = [std::mem::take(&mut self.acc[0]), std::mem::take(&mut self.acc[1])];
                    if let Ok(out) = rs.process_partial(Some(&input), None) {
                        for i in 0..out[0].len() {
                            self.pending.push_back(out[0][i]);
                            self.pending.push_back(out[1][i]);
                        }
                    }
                }
            }
        }
    }
}

impl Engine for SymphoniaEngine {
    fn render(&mut self, out: &mut [f32]) -> RenderStatus {
        while self.pending.len() < out.len() && !self.eof {
            if self.decode_packet() {
                self.drain_acc(false);
            } else {
                self.eof = true;
                self.drain_acc(true);
            }
        }
        let mut n = 0;
        while n < out.len() {
            match self.pending.pop_front() {
                Some(s) => {
                    out[n] = s;
                    n += 1;
                }
                None => break,
            }
        }
        out[n..].fill(0.0);
        if n < out.len() && self.eof {
            RenderStatus::Ended
        } else {
            RenderStatus::Continue
        }
    }

    fn tags(&self) -> &TrackTags {
        &self.tags
    }
}
