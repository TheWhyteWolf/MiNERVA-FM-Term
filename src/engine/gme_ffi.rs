//! Hand-written bindings for the slice of the game-music-emu C API we drive —
//! the same function set the web edition's wasm build exports.
#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int, c_long};

pub enum MusicEmu {}

/// Error string, or null on success.
pub type gme_err_t = *const c_char;

#[repr(C)]
pub struct gme_info_t {
    /* times in milliseconds; -1 if unknown */
    pub length: c_int,
    pub intro_length: c_int,
    pub loop_length: c_int,
    /// Length if available, else intro+2*loop, else 150000 (2.5 min).
    pub play_length: c_int,
    pub fade_length: c_int,
    pub reserved_i: [c_int; 11],
    pub system: *const c_char,
    pub game: *const c_char,
    pub song: *const c_char,
    pub author: *const c_char,
    pub copyright: *const c_char,
    pub comment: *const c_char,
    pub dumper: *const c_char,
    pub reserved_s: [*const c_char; 9],
}

extern "C" {
    pub fn gme_open_data(
        data: *const u8,
        size: c_long,
        out: *mut *mut MusicEmu,
        sample_rate: c_int,
    ) -> gme_err_t;
    pub fn gme_track_count(emu: *const MusicEmu) -> c_int;
    pub fn gme_start_track(emu: *mut MusicEmu, index: c_int) -> gme_err_t;
    pub fn gme_play(emu: *mut MusicEmu, count: c_int, out: *mut i16) -> gme_err_t;
    pub fn gme_track_ended(emu: *const MusicEmu) -> c_int;
    pub fn gme_track_info(
        emu: *const MusicEmu,
        out: *mut *mut gme_info_t,
        track: c_int,
    ) -> gme_err_t;
    pub fn gme_free_info(info: *mut gme_info_t);
    #[allow(dead_code)] // the mixer's own envelope fades; kept for parity with the wasm export list
    pub fn gme_set_fade(emu: *mut MusicEmu, start_msec: c_int);
    #[allow(dead_code)]
    pub fn gme_ignore_silence(emu: *mut MusicEmu, ignore: c_int);
    pub fn gme_delete(emu: *mut MusicEmu);
}
