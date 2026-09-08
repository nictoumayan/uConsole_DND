//! Amber phosphor, as ratatui styles.
//!
//! Chosen over the classic green because it is materially easier on the eyes
//! across a four-hour session, and it matches the uConsole's own colourway.

use ratatui::style::{Color, Modifier, Style};

pub const AMBER: Color = Color::Rgb(255, 176, 0);
pub const AMBER_DIM: Color = Color::Rgb(150, 100, 10);
pub const AMBER_BRIGHT: Color = Color::Rgb(255, 220, 140);
pub const BG: Color = Color::Rgb(20, 14, 6);

pub fn base() -> Style {
    Style::default().fg(AMBER).bg(BG)
}

pub fn dim() -> Style {
    Style::default().fg(AMBER_DIM).bg(BG)
}

pub fn bright() -> Style {
    Style::default().fg(AMBER_BRIGHT).bg(BG).add_modifier(Modifier::BOLD)
}

/// Selection is an inverted bar rather than a coloured one — on a 5" panel in
/// a dim room, a filled block is legible at a glance and a hue shift is not.
/// Red rather than amber. Conditions, zero hit points and a failed save are
/// the only things allowed to break the phosphor palette — that is exactly why
/// they read instantly.
pub fn danger() -> Style {
    Style::default().fg(Color::Rgb(255, 90, 60)).bg(BG).add_modifier(Modifier::BOLD)
}

pub fn selection() -> Style {
    Style::default().fg(BG).bg(AMBER).add_modifier(Modifier::BOLD)
}

pub fn tab_active() -> Style {
    Style::default().fg(BG).bg(AMBER).add_modifier(Modifier::BOLD)
}
