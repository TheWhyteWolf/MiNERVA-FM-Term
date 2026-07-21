//! C64 SID playback via the system libsidplayfp (reSIDfp), driven through
//! the C shim in vendor/shim/sidshim.cpp. Durations come from the HVSC
//! Songlengths database when one is available.

use super::{Engine, RenderStatus, SubtuneMode, TrackTags};
use anyhow::{anyhow, Result};
use rand::Rng;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::path::Path;
use std::sync::Arc;

extern "C" {
    fn sidshim_create(
        data: *const u8,
        len: usize,
        song: u32,
        sample_rate: u32,
        err_out: *mut *const c_char,
    ) -> *mut c_void;
    fn sidshim_render(h: *mut c_void, out: *mut i16, frames: i32) -> i32;
    fn sidshim_error(h: *mut c_void) -> *const c_char;
    fn sidshim_songs(h: *mut c_void) -> u32;
    fn sidshim_selected_song(h: *mut c_void) -> u32;
    fn sidshim_info(h: *mut c_void, idx: u32) -> *const c_char;
    fn sidshim_destroy(h: *mut c_void);
    fn sidshim_db_open(path: *const c_char) -> *mut c_void;
    fn sidshim_db_length_ms(db: *mut c_void, h: *mut c_void) -> i32;
    fn sidshim_db_close(db: *mut c_void);
}

fn info_str(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // HVSC metadata is latin-1; lossy conversion is fine for display.
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().trim().to_string();
    (!s.is_empty() && s != "<?>").then_some(s)
}

/// Open handle to a Songlengths.md5 database, shared across track loads.
pub struct SidDb(*mut c_void);
unsafe impl Send for SidDb {}
unsafe impl Sync for SidDb {} // read-only lookups after open

impl SidDb {
    pub fn open(path: &Path) -> Option<Arc<SidDb>> {
        let cpath = std::ffi::CString::new(path.to_str()?).ok()?;
        let db = unsafe { sidshim_db_open(cpath.as_ptr()) };
        (!db.is_null()).then(|| Arc::new(SidDb(db)))
    }
}

impl Drop for SidDb {
    fn drop(&mut self) {
        unsafe { sidshim_db_close(self.0) };
    }
}

pub struct SidEngine {
    handle: *mut c_void,
    buf: Vec<i16>,
    tags: TrackTags,
    ended: bool,
}

// Only ever driven from the owning (mixer) thread.
unsafe impl Send for SidEngine {}

impl SidEngine {
    pub fn new(
        data: Vec<u8>,
        sample_rate: u32,
        subtune_mode: SubtuneMode,
        db: Option<&Arc<SidDb>>,
    ) -> Result<SidEngine> {
        // Probe with the default song first to learn the subtune count.
        let mut err: *const c_char = std::ptr::null();
        let mut handle =
            unsafe { sidshim_create(data.as_ptr(), data.len(), 0, sample_rate, &mut err) };
        if handle.is_null() {
            let msg = info_str(err).unwrap_or_else(|| "unknown load failure".into());
            return Err(anyhow!("sid: {msg}"));
        }
        let songs = unsafe { sidshim_songs(handle) }.max(1);

        if songs > 1 && subtune_mode == SubtuneMode::Random {
            let pick = rand::rng().random_range(1..=songs);
            if pick != unsafe { sidshim_selected_song(handle) } {
                unsafe { sidshim_destroy(handle) };
                handle = unsafe {
                    sidshim_create(data.as_ptr(), data.len(), pick, sample_rate, &mut err)
                };
                if handle.is_null() {
                    let msg = info_str(err).unwrap_or_else(|| "unknown load failure".into());
                    return Err(anyhow!("sid: subtune {pick}: {msg}"));
                }
            }
        }
        let selected = unsafe { sidshim_selected_song(handle) }.max(1);

        let duration_secs = db.and_then(|db| {
            let ms = unsafe { sidshim_db_length_ms(db.0, handle) };
            (ms > 0).then(|| ms as f64 / 1000.0)
        });

        let tags = TrackTags {
            title: info_str(unsafe { sidshim_info(handle, 0) }),
            author: info_str(unsafe { sidshim_info(handle, 1) }),
            system: Some("Commodore 64".into()),
            duration_secs,
            subtune: (songs > 1).then_some((selected, songs)),
            ..TrackTags::default()
        };

        Ok(SidEngine {
            handle,
            buf: Vec::new(),
            tags,
            ended: false,
        })
    }
}

impl Engine for SidEngine {
    fn render(&mut self, out: &mut [f32]) -> RenderStatus {
        if self.ended {
            out.fill(0.0);
            return RenderStatus::Ended;
        }
        let frames = out.len() / 2;
        self.buf.resize(out.len(), 0);
        let got = unsafe { sidshim_render(self.handle, self.buf.as_mut_ptr(), frames as i32) };
        if got < 0 {
            let msg = info_str(unsafe { sidshim_error(self.handle) });
            eprintln!("sid render error: {}", msg.unwrap_or_default());
            out.fill(0.0);
            self.ended = true;
            return RenderStatus::Ended;
        }
        let samples = got as usize * 2;
        for (dst, src) in out[..samples].iter_mut().zip(&self.buf[..samples]) {
            *dst = *src as f32 / 32768.0;
        }
        out[samples..].fill(0.0);
        if (got as usize) < frames {
            self.ended = true;
            return RenderStatus::Ended;
        }
        RenderStatus::Continue
    }

    fn tags(&self) -> &TrackTags {
        &self.tags
    }
}

impl Drop for SidEngine {
    fn drop(&mut self) {
        unsafe { sidshim_destroy(self.handle) };
    }
}
