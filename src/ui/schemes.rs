//! The radio's colour schemes and spectrum characters, RGB values as in the
//! web edition (index.html VIS_SCHEMES / VIS_CHARS), which in turn mirror the
//! vis config the shell radio writes.

use ratatui::style::Color;

pub type Rgb = (u8, u8, u8);

pub struct Scheme {
    pub name: &'static str,
    /// Low-amplitude colour first → high-amplitude last (bar gradient).
    pub colors: &'static [Rgb],
}

pub const SCHEMES: &[Scheme] = &[
    Scheme { name: "gameboy", colors: &[(0x0f, 0x38, 0x0f), (0x30, 0x62, 0x30), (0x8b, 0xac, 0x0f), (0x9b, 0xbc, 0x0f)] },
    Scheme { name: "gameboy_pocket", colors: &[(0x2b, 0x2b, 0x22), (0x4d, 0x53, 0x3c), (0x8b, 0x95, 0x6d), (0xc4, 0xcf, 0xa1)] },
    Scheme { name: "bbc_micro", colors: &[(0x00, 0x00, 0xff), (0x00, 0xff, 0x00), (0xff, 0xff, 0x00), (0xff, 0x00, 0x00), (0xff, 0xff, 0xff)] },
    Scheme { name: "pico8", colors: &[(0x1d, 0x2b, 0x53), (0x7e, 0x25, 0x53), (0xff, 0x00, 0x4d), (0xff, 0xa3, 0x00), (0xff, 0xf1, 0xe8)] },
    Scheme { name: "cga", colors: &[(0x00, 0x00, 0x00), (0x55, 0xff, 0xff), (0xff, 0x55, 0xff), (0xff, 0xff, 0xff)] },
    Scheme { name: "nes", colors: &[(0x00, 0x00, 0xfc), (0x00, 0x78, 0xf8), (0x00, 0xb8, 0x00), (0xf8, 0xb8, 0x00), (0xf8, 0x38, 0x00), (0xfc, 0xfc, 0xfc)] },
    Scheme { name: "minerva", colors: &[(0x2a, 0x0a, 0x00), (0x7a, 0x2a, 0x00), (0xff, 0x7a, 0x00), (0xff, 0xc2, 0x4d), (0xff, 0xf3, 0xc0)] },
    Scheme { name: "c64", colors: &[(0x35, 0x28, 0x79), (0x6c, 0x5e, 0xb5), (0x6a, 0xbf, 0xc6), (0xb8, 0xc7, 0x6f), (0xff, 0xff, 0xff)] },
    Scheme { name: "zx_spectrum", colors: &[(0x00, 0x00, 0xd7), (0xd7, 0x00, 0x00), (0xd7, 0x00, 0xd7), (0x00, 0xd7, 0x00), (0xd7, 0xd7, 0x00), (0xff, 0xff, 0xff)] },
];

pub const CHARS: &[char] = &[
    '#', '|', ':', '+', '*', '=', '!', '>', '%', '@', '^', '~', '-', 'o', 'x',
];

/// The terminal accent palette (web edition's ANSI map).
pub mod ansi {
    use super::Rgb;
    pub const BLUE: Rgb = (0x5b, 0x9b, 0xff);
    pub const YELLOW: Rgb = (0xff, 0xd6, 0x33);
    pub const GREEN: Rgb = (0x54, 0xff, 0x7a);
    pub const MAGENTA: Rgb = (0xff, 0x7b, 0xd6);
    pub const RED: Rgb = (0xff, 0x5a, 0x5a);
    pub const CYAN: Rgb = (0x5c, 0xf7, 0xff);
    pub const FG: Rgb = (0xcf, 0xe8, 0xcf);
    pub const DIM: Rgb = (0x5f, 0x7a, 0x52);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    True,
    C256,
}

impl ColorMode {
    pub fn detect() -> ColorMode {
        match std::env::var("COLORTERM") {
            Ok(v) if v.contains("truecolor") || v.contains("24bit") => ColorMode::True,
            _ => ColorMode::C256,
        }
    }
}

/// RGB → ratatui colour honouring the depth mode (6×6×6 cube quantisation
/// for 256-colour terminals).
pub fn color(rgb: Rgb, mode: ColorMode) -> Color {
    match mode {
        ColorMode::True => Color::Rgb(rgb.0, rgb.1, rgb.2),
        ColorMode::C256 => {
            let q = |c: u8| ((c as u16 * 5 + 127) / 255) as u8;
            Color::Indexed(16 + 36 * q(rgb.0) + 6 * q(rgb.1) + q(rgb.2))
        }
    }
}

pub fn scheme_index(name: &str) -> Option<usize> {
    SCHEMES.iter().position(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantises_into_the_colour_cube() {
        match color((255, 255, 255), ColorMode::C256) {
            Color::Indexed(i) => assert_eq!(i, 231), // white corner of the cube
            other => panic!("expected indexed colour, got {other:?}"),
        }
        match color((0, 0, 0), ColorMode::C256) {
            Color::Indexed(i) => assert_eq!(i, 16),
            other => panic!("expected indexed colour, got {other:?}"),
        }
    }
}
