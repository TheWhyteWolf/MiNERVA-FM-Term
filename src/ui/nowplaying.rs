//! The 12-row now-playing header pane, matching the shell/web layout:
//! coloured labels, dim separator on the last row.

use super::schemes::{ansi, color, ColorMode};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Clone, Default)]
pub struct NowPlaying {
    pub id: String,
    pub source: String, // RANDOM / LOCAL
    pub system: Option<String>,
    pub game: String,
    pub track: String,
    pub author: Option<String>,
    pub scheme_name: String,
    pub subtune: Option<(u32, u32)>,
}

pub struct HeaderState<'a> {
    pub now: Option<&'a NowPlaying>,
    pub paused: bool,
    pub volume: f32,
    pub sid_count: usize,
    pub status: Option<&'a str>, // last skip/error, if any
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

pub fn lines(state: &HeaderState, width: usize, mode: ColorMode) -> Vec<Line<'static>> {
    let fg = |rgb| Style::default().fg(color(rgb, mode));
    let mut out = Vec::with_capacity(12);

    let mut head = vec![
        Span::styled("[ NOW PLAYING ]", fg(ansi::BLUE)),
        Span::raw(" "),
    ];
    if let Some(np) = state.now {
        head.push(Span::styled(format!("[{}]", np.source), fg(ansi::YELLOW)));
    }
    if state.paused {
        head.push(Span::styled(" [PAUSED]", fg(ansi::RED)));
    }
    head.push(Span::styled(
        format!("  Vol {:.0}%", state.volume * 100.0),
        fg(ansi::DIM),
    ));
    out.push(Line::from(head));
    out.push(Line::default());

    let field = |label: &str, value: String, vc| {
        Line::from(vec![
            Span::styled(format!("{label} "), fg(ansi::GREEN)),
            Span::styled(value, fg(vc)),
        ])
    };

    match state.now {
        Some(np) => {
            out.push(field("ID:     ", np.id.clone(), ansi::FG));
            if let Some(sys) = &np.system {
                out.push(field("System: ", clip(sys, width.saturating_sub(9)), ansi::FG));
            }
            out.push(field("Game:   ", clip(&np.game, width.saturating_sub(9)), ansi::FG));
            let mut track = np.track.clone();
            if let Some((n, total)) = np.subtune {
                track.push_str(&format!("  (subtune {n}/{total})"));
            }
            out.push(field("Track:  ", clip(&track, width.saturating_sub(9)), ansi::FG));
            if let Some(a) = &np.author {
                out.push(field("Artist: ", clip(a, width.saturating_sub(9)), ansi::FG));
            }
            out.push(field("Colour: ", np.scheme_name.clone(), ansi::FG));
        }
        None => out.push(Line::from(Span::styled("tuning…", fg(ansi::DIM)))),
    }

    if state.sid_count > 0 {
        out.push(Line::from(Span::styled(
            format!("SID: {} file(s) listed — playback lands in v0.2", state.sid_count),
            fg(ansi::DIM),
        )));
    }
    if let Some(msg) = state.status {
        out.push(Line::from(Span::styled(
            clip(msg, width.saturating_sub(1)),
            fg(ansi::RED),
        )));
    }

    // Separator pinned to the pane's last row.
    while out.len() < 11 {
        out.push(Line::default());
    }
    out.truncate(11);
    out.push(Line::from(Span::styled("-".repeat(width), fg(ansi::DIM))));
    out
}
