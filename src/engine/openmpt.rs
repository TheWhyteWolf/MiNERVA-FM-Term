//! Tracker formats (.mod .xm .s3m .it) via the system libopenmpt C API.
#![allow(non_camel_case_types)]

use super::{Engine, RenderStatus, TrackTags};
use anyhow::{bail, Result};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

enum OpenMptModule {}

extern "C" {
    fn openmpt_module_create_from_memory2(
        filedata: *const c_void,
        filesize: usize,
        logfunc: *const c_void,
        loguser: *mut c_void,
        errfunc: *const c_void,
        erruser: *mut c_void,
        error: *mut c_int,
        error_message: *mut *const c_char,
        ctls: *const c_void,
    ) -> *mut OpenMptModule;
    fn openmpt_module_destroy(module: *mut OpenMptModule);
    fn openmpt_module_read_interleaved_float_stereo(
        module: *mut OpenMptModule,
        samplerate: i32,
        count: usize,
        interleaved_stereo: *mut f32,
    ) -> usize;
    fn openmpt_module_get_duration_seconds(module: *mut OpenMptModule) -> f64;
    fn openmpt_module_set_repeat_count(module: *mut OpenMptModule, repeat_count: i32) -> c_int;
    fn openmpt_module_get_metadata(
        module: *mut OpenMptModule,
        key: *const c_char,
    ) -> *const c_char;
    fn openmpt_free_string(s: *const c_char);
}

pub struct OpenMptEngine {
    module: *mut OpenMptModule,
    sample_rate: i32,
    tags: TrackTags,
    ended: bool,
}

// Only ever driven from the owning (mixer) thread.
unsafe impl Send for OpenMptEngine {}

fn metadata(module: *mut OpenMptModule, key: &str) -> Option<String> {
    let key = CString::new(key).ok()?;
    let ptr = unsafe { openmpt_module_get_metadata(module, key.as_ptr()) };
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().trim().to_string();
    unsafe { openmpt_free_string(ptr) };
    (!s.is_empty()).then_some(s)
}

impl OpenMptEngine {
    pub fn new(data: Vec<u8>, sample_rate: u32) -> Result<OpenMptEngine> {
        let module = unsafe {
            openmpt_module_create_from_memory2(
                data.as_ptr() as *const c_void,
                data.len(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if module.is_null() {
            bail!("libopenmpt could not parse the module");
        }
        unsafe { openmpt_module_set_repeat_count(module, 0) }; // play once

        let duration = unsafe { openmpt_module_get_duration_seconds(module) };
        let tags = TrackTags {
            title: metadata(module, "title"),
            author: metadata(module, "artist"),
            system: metadata(module, "type_long").or_else(|| Some("Tracker".into())),
            duration_secs: (duration.is_finite() && duration > 0.0).then_some(duration),
            ..TrackTags::default()
        };

        Ok(OpenMptEngine {
            module,
            sample_rate: sample_rate as i32,
            tags,
            ended: false,
        })
    }
}

impl Engine for OpenMptEngine {
    fn render(&mut self, out: &mut [f32]) -> RenderStatus {
        if self.ended {
            out.fill(0.0);
            return RenderStatus::Ended;
        }
        let frames = out.len() / 2;
        let got = unsafe {
            openmpt_module_read_interleaved_float_stereo(
                self.module,
                self.sample_rate,
                frames,
                out.as_mut_ptr(),
            )
        };
        if got < frames {
            out[got * 2..].fill(0.0);
            self.ended = true;
            return RenderStatus::Ended;
        }
        RenderStatus::Continue
    }

    fn tags(&self) -> &TrackTags {
        &self.tags
    }
}

impl Drop for OpenMptEngine {
    fn drop(&mut self) {
        unsafe { openmpt_module_destroy(self.module) };
    }
}
