//! Colors, styles, logo and small rendering helpers shared by all tabs.

use crate::util::{self, Risk};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding};

pub const ACCENT: Color = Color::Cyan;
pub const ACCENT2: Color = Color::Yellow;
pub const DIM: Color = Color::DarkGray;
pub const OK: Color = Color::Green;
pub const WARN: Color = Color::Yellow;
pub const BAD: Color = Color::Red;
pub const TEXT: Color = Color::Gray;

pub const LOGO: [&str; 5] = [
    r"    ____                            ",
    r"   / __ )_________  ____  ____ ___  ",
    r"  / __  / ___/ __ \/ __ \/ __ `__ \ ",
    r" / /_/ / /  / /_/ / /_/ / / / / / / ",
    r"/_____/_/   \____/\____/_/ /_/ /_/  ",
];
pub const BROOM: [&str; 5] =
    [r"        ||        ", r"        ||        ", r"       /||\       ", r"      //||\\   .  ", r"     ///||\\\ .:: "];

pub fn logo_lines() -> Vec<Line<'static>> {
    (0..5)
        .map(|i| {
            Line::from(vec![
                Span::styled(LOGO[i], Style::new().fg(if i < 3 { Color::Cyan } else { Color::Blue }).add_modifier(Modifier::BOLD)),
                Span::styled(BROOM[i], Style::new().fg(if i < 2 { Color::DarkGray } else { Color::Yellow })),
            ])
        })
        .collect()
}

pub fn block(title: &str) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(DIM))
        .title(Line::from(format!(" {title} ")).fg(ACCENT).bold())
        .padding(Padding::horizontal(1))
}

pub fn focused_block(title: &str) -> Block<'static> {
    block(title).border_style(Style::new().fg(ACCENT))
}

pub fn risk_span(r: Risk) -> Span<'static> {
    let c = match r {
        Risk::Safe => OK,
        Risk::Moderate => WARN,
        Risk::Aggressive => BAD,
    };
    Span::styled(format!("{:<10}", r.label()), Style::new().fg(c))
}

pub fn size_span(bytes: u64, measured: bool) -> Span<'static> {
    let c = if bytes >= 1 << 30 {
        BAD
    } else if bytes >= 100 << 20 {
        WARN
    } else if bytes > 0 {
        TEXT
    } else {
        DIM
    };
    let txt = if bytes == 0 {
        "-".to_string()
    } else if measured {
        util::fmt_size(bytes)
    } else {
        format!("~{}", util::fmt_size(bytes))
    };
    Span::styled(format!("{txt:>10}"), Style::new().fg(c))
}

pub fn checkbox(on: bool) -> Span<'static> {
    if on {
        Span::styled("[x] ", Style::new().fg(OK).bold())
    } else {
        Span::styled("[ ] ", Style::new().fg(DIM))
    }
}

pub fn key(k: &str) -> Span<'static> {
    Span::styled(format!(" {k} "), Style::new().fg(Color::Black).bg(DIM))
}

/// "␣ toggle  a all ..." footer from (key, label) pairs
pub fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (k, l) in pairs {
        spans.push(key(k));
        spans.push(Span::styled(format!(" {l}  "), Style::new().fg(TEXT)));
    }
    Line::from(spans)
}

pub fn highlight() -> Style {
    Style::new().bg(Color::Rgb(30, 60, 80)).add_modifier(Modifier::BOLD)
}

/// Centered rectangle of the given size (clamped to `area`).
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [r] = Layout::horizontal([Constraint::Length(w.min(area.width))]).flex(Flex::Center).areas(area);
    let [r] = Layout::vertical([Constraint::Length(h.min(area.height))]).flex(Flex::Center).areas(r);
    r
}

/// A horizontal bar like "██████░░░░" for `ratio` (0..1).
pub fn bar(ratio: f64, width: usize) -> String {
    let filled = ((ratio.clamp(0.0, 1.0)) * width as f64).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

pub fn bar_color(ratio_used: f64) -> Color {
    if ratio_used > 0.9 {
        BAD
    } else if ratio_used > 0.75 {
        WARN
    } else {
        OK
    }
}

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner(tick: usize) -> &'static str {
    SPINNER[tick % SPINNER.len()]
}
