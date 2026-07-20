pub mod mixer;
pub mod output;

use crate::engine::Format;

/// Engine-agnostic playback bounds for one track, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Policy {
    pub cap_secs: f64,
    pub fade_secs: f64,
}

/// Mirror of the shell radio's duration rules: SPC tags get a minimum floor
/// (many rips carry tiny ID666 lengths and the emulated audio loops anyway),
/// VGM defaults to GME's 2.5-minute fallback, and everything is capped so an
/// infinite loop flag can never wedge the stream.
pub fn duration_policy(
    format: Format,
    engine_secs: Option<f64>,
    spc_min_secs: u32,
    max_track_secs: u32,
) -> Policy {
    let max = max_track_secs.max(1) as f64;
    let natural = match format {
        Format::Spc => engine_secs.unwrap_or(120.0).max(spc_min_secs as f64),
        Format::Vgm => engine_secs.unwrap_or(150.0),
        Format::GmeOther | Format::Sid => engine_secs.unwrap_or(120.0),
        Format::Tracker => engine_secs.unwrap_or(max),
        Format::Stream => max, // decoders end naturally; cap is a backstop
    };
    let cap_secs = natural.min(max);
    // Same rule as the shell: if there is no room for the fade, skip it.
    let fade_secs = if cap_secs - 5.0 >= 1.0 { 5.0 } else { 0.0 };
    Policy { cap_secs, fade_secs }
}

/// Per-frame gain from the fade-out envelope. `1.0` before the fade window,
/// linear to `0.0` at the cap, `0.0` after.
pub fn envelope_gain(frame: u64, cap_frames: u64, fade_frames: u64) -> f32 {
    let fade_start = cap_frames.saturating_sub(fade_frames);
    if frame >= cap_frames {
        0.0
    } else if frame >= fade_start && fade_frames > 0 {
        (cap_frames - frame) as f32 / fade_frames as f32
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spc_floor_applies() {
        // Confronting_Ganon.spc carries a 4s tag; the floor lifts it to 60s.
        let p = duration_policy(Format::Spc, Some(4.0), 60, 900);
        assert_eq!(p.cap_secs, 60.0);
        assert_eq!(p.fade_secs, 5.0);
    }

    #[test]
    fn spc_long_tag_survives() {
        let p = duration_policy(Format::Spc, Some(376.0), 60, 900);
        assert_eq!(p.cap_secs, 376.0);
    }

    #[test]
    fn max_track_caps_everything() {
        let p = duration_policy(Format::Vgm, Some(4000.0), 60, 900);
        assert_eq!(p.cap_secs, 900.0);
    }

    #[test]
    fn tiny_cap_drops_fade() {
        let p = duration_policy(Format::Spc, Some(2.0), 3, 900);
        assert_eq!(p.cap_secs, 3.0);
        assert_eq!(p.fade_secs, 0.0);
    }

    #[test]
    fn envelope_shape() {
        // 10-frame track, 4-frame fade: full gain until frame 6.
        assert_eq!(envelope_gain(0, 10, 4), 1.0);
        assert_eq!(envelope_gain(5, 10, 4), 1.0);
        assert_eq!(envelope_gain(6, 10, 4), 1.0);
        assert_eq!(envelope_gain(8, 10, 4), 0.5);
        assert_eq!(envelope_gain(10, 10, 4), 0.0);
        assert_eq!(envelope_gain(11, 10, 4), 0.0);
        // No fade window: hard stop at cap.
        assert_eq!(envelope_gain(9, 10, 0), 1.0);
        assert_eq!(envelope_gain(10, 10, 0), 0.0);
    }
}
