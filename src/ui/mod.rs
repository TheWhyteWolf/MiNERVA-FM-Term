pub mod nowplaying;
pub mod schemes;
pub mod spectrum;
pub mod ticker;

use nowplaying::HeaderState;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use schemes::{color, ColorMode, SCHEMES};
use spectrum::{bar_x, COLS};

pub const HEADER_ROWS: u16 = 12;
pub const MIN_W: u16 = 40;
pub const MIN_H: u16 = 15;

pub struct View<'a> {
    pub header: HeaderState<'a>,
    pub bars: &'a [f32; COLS],
    pub scheme_idx: usize,
    pub spectrum_char: char,
    pub ticker_line: String,
    pub ticker_colour: schemes::Rgb,
    pub mode: ColorMode,
}

pub fn draw(f: &mut Frame, v: &View) {
    let area = f.area();
    if area.width < MIN_W || area.height < MIN_H {
        let msg = Paragraph::new(format!(
            "MiNERVA-FM needs at least {MIN_W}x{MIN_H} — resize me"
        ))
        .style(Style::default().fg(color(schemes::ansi::YELLOW, v.mode)));
        f.render_widget(msg, area);
        return;
    }

    let [header_area, vis_area, ticker_area] = Layout::vertical([
        Constraint::Length(HEADER_ROWS),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let header_lines = nowplaying::lines(&v.header, header_area.width as usize, v.mode);
    f.render_widget(Paragraph::new(header_lines), header_area);

    draw_spectrum(f, vis_area, v);

    let ticker = Paragraph::new(Span::styled(
        v.ticker_line.clone(),
        Style::default().fg(color(v.ticker_colour, v.mode)),
    ));
    f.render_widget(ticker, ticker_area);
}

/// Vertical stacks of the track's character, palette-coloured bottom→top,
/// 56 bars spread across the pane width — the vis pane of the shell radio.
fn draw_spectrum(f: &mut Frame, area: Rect, v: &View) {
    if area.height == 0 {
        return;
    }
    let pal = SCHEMES[v.scheme_idx % SCHEMES.len()].colors;
    let rows = area.height as usize;
    let width = area.width as usize;
    let buf = f.buffer_mut();

    for c in 0..COLS {
        let x = area.x + bar_x(c, width) as u16;
        if x >= area.right() {
            continue;
        }
        let h = ((v.bars[c] * rows as f32).round() as usize).min(rows);
        for r in 0..h {
            let frac = r as f32 / (rows.saturating_sub(1).max(1)) as f32;
            let idx = ((frac * pal.len() as f32) as usize).min(pal.len() - 1);
            let y = area.bottom() - 1 - r as u16;
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(v.spectrum_char);
                cell.set_style(Style::default().fg(color(pal[idx], v.mode)));
            }
        }
    }
}
