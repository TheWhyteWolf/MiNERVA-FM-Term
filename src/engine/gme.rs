//! game-music-emu engine: SPC, VGM/VGZ, NSF/NSFE, GBS, AY, KSS, HES, GYM, SAP.

use super::gme_ffi as ffi;
use super::{Engine, EngineParams, RenderStatus, SubtuneMode, TrackTags};
use anyhow::{anyhow, bail, Result};
use flate2::read::GzDecoder;
use rand::Rng;
use std::ffi::CStr;
use std::io::Read;
use std::os::raw::c_int;

pub struct GmeEngine {
    emu: *mut ffi::MusicEmu,
    buf: Vec<i16>,
    tags: TrackTags,
    ended: bool,
}

// The raw emulator pointer is only ever used from the thread that owns the
// engine (the mixer); GME has no thread-local state.
unsafe impl Send for GmeEngine {}

pub const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Gunzip if the buffer carries the gzip magic (.vgz); GME is built without
/// zlib, mirroring the web edition's wasm build.
pub fn maybe_gunzip(data: Vec<u8>) -> Result<Vec<u8>> {
    if data.len() >= 2 && data[..2] == GZIP_MAGIC {
        let mut out = Vec::with_capacity(data.len() * 4);
        GzDecoder::new(&data[..]).read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(data)
    }
}

fn err(res: ffi::gme_err_t) -> Result<()> {
    if res.is_null() {
        Ok(())
    } else {
        let msg = unsafe { CStr::from_ptr(res) }.to_string_lossy().into_owned();
        Err(anyhow!("gme: {msg}"))
    }
}

fn opt_str(p: *const std::os::raw::c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().trim().to_string();
    (!s.is_empty()).then_some(s)
}

impl GmeEngine {
    pub fn new(data: Vec<u8>, params: &EngineParams) -> Result<GmeEngine> {
        let data = maybe_gunzip(data)?;
        let mut emu: *mut ffi::MusicEmu = std::ptr::null_mut();
        err(unsafe {
            ffi::gme_open_data(
                data.as_ptr(),
                data.len() as _,
                &mut emu,
                params.sample_rate as c_int,
            )
        })?;
        if emu.is_null() {
            bail!("gme: open returned no emulator");
        }

        let count = unsafe { ffi::gme_track_count(emu) }.max(1) as u32;
        let track = match params.subtune_mode {
            SubtuneMode::First => 0,
            SubtuneMode::Random if count > 1 => rand::rng().random_range(0..count),
            SubtuneMode::Random => 0,
        };

        let mut tags = TrackTags {
            subtune: (count > 1).then_some((track + 1, count)),
            ..TrackTags::default()
        };
        let mut info: *mut ffi::gme_info_t = std::ptr::null_mut();
        if unsafe { ffi::gme_track_info(emu, &mut info, track as c_int) }.is_null()
            && !info.is_null()
        {
            let i = unsafe { &*info };
            tags.title = opt_str(i.song);
            tags.game = opt_str(i.game);
            tags.system = opt_str(i.system);
            tags.author = opt_str(i.author);
            if i.play_length > 0 {
                tags.duration_secs = Some(i.play_length as f64 / 1000.0);
            }
            unsafe { ffi::gme_free_info(info) };
        }

        if let Err(e) = err(unsafe { ffi::gme_start_track(emu, track as c_int) }) {
            unsafe { ffi::gme_delete(emu) };
            return Err(e);
        }
        // GME auto-fades at the file's tag length, which would defeat the
        // SPC minimum-duration floor (tiny ID666 tags). Push its fade far
        // out; the mixer's envelope owns duration and fading instead.
        // Silence detection stays on, so dead tracks still end early.
        unsafe { ffi::gme_set_fade(emu, i32::MAX / 2) };

        Ok(GmeEngine {
            emu,
            buf: Vec::new(),
            tags,
            ended: false,
        })
    }
}

impl Engine for GmeEngine {
    fn render(&mut self, out: &mut [f32]) -> RenderStatus {
        if self.ended {
            out.fill(0.0);
            return RenderStatus::Ended;
        }
        self.buf.resize(out.len(), 0);
        // count is in 16-bit samples (stereo interleaved), not frames.
        let res = unsafe { ffi::gme_play(self.emu, out.len() as c_int, self.buf.as_mut_ptr()) };
        if !res.is_null() {
            out.fill(0.0);
            self.ended = true;
            return RenderStatus::Ended;
        }
        for (dst, src) in out.iter_mut().zip(&self.buf) {
            *dst = *src as f32 / 32768.0;
        }
        if unsafe { ffi::gme_track_ended(self.emu) } != 0 {
            self.ended = true;
            return RenderStatus::Ended;
        }
        RenderStatus::Continue
    }

    fn tags(&self) -> &TrackTags {
        &self.tags
    }
}

impl Drop for GmeEngine {
    fn drop(&mut self) {
        unsafe { ffi::gme_delete(self.emu) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gunzip_passthrough_leaves_plain_data() {
        let plain = vec![0x56, 0x67, 0x6d, 0x20]; // "Vgm "
        assert_eq!(maybe_gunzip(plain.clone()).unwrap(), plain);
    }

    #[test]
    fn gunzip_detects_magic() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(b"hello vgm").unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(maybe_gunzip(gz).unwrap(), b"hello vgm");
    }
}
