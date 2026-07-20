//! SID helpers. Playback itself lands in v0.2 with the jsSID port; the header
//! parser lives here already because the library scanner and the tests use it.

/// Subtune count from a PSID/RSID header: big-endian u16 at offset 0x0E.
/// (The shell radio reads the same field; jsSID only reads the low byte.)
#[allow(dead_code)] // wired up when the SID engine lands in v0.2
pub fn sid_subtune_count(header: &[u8]) -> u32 {
    if header.len() >= 16 && (&header[..4] == b"PSID" || &header[..4] == b"RSID") {
        let n = u16::from_be_bytes([header[14], header[15]]) as u32;
        n.max(1)
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_psid_subtune_count() {
        let mut h = vec![0u8; 16];
        h[..4].copy_from_slice(b"PSID");
        h[14] = 0x00;
        h[15] = 0x0b; // 11 subtunes
        assert_eq!(sid_subtune_count(&h), 11);
    }

    #[test]
    fn rsid_and_bounds() {
        let mut h = vec![0u8; 16];
        h[..4].copy_from_slice(b"RSID");
        h[14] = 0x01;
        h[15] = 0x00; // 256, big-endian
        assert_eq!(sid_subtune_count(&h), 256);
        assert_eq!(sid_subtune_count(b"NOTS"), 1);
        assert_eq!(sid_subtune_count(&h[..10]), 1);
        // zero subtunes clamps to 1
        let mut z = vec![0u8; 16];
        z[..4].copy_from_slice(b"PSID");
        assert_eq!(sid_subtune_count(&z), 1);
    }
}
