//! The interactive TUI.
//!
//! `state` and `keys` know nothing about ratatui or about a terminal, so the
//! entire interaction model is exercised headlessly in tests/app.rs. This
//! module is only the loop: set the terminal up, draw, read a key, repeat.

pub mod keys;
pub mod state;
pub mod theme;
pub mod ui;

pub use state::{App, Mode};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::time::Duration;

use crate::portrait::Portrait;

pub fn run(mut app: App, portrait: Option<Portrait>) -> Result<()> {
    // A panic while the terminal is in raw mode leaves the user's shell
    // wrecked — no echo, no line editing. Restore first, then panic.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        previous(info);
    }));

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app, portrait.as_ref());
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    portrait: Option<&Portrait>,
) -> Result<()> {
    loop {
        terminal
            .draw(|f| ui::draw(f, app, portrait))
            .context("drawing frame")?;

        // A blocking read would be simplest, but polling lets the loop wake to
        // redraw on a resize without waiting for a keypress.
        if !event::poll(Duration::from_millis(250)).context("polling for input")? {
            continue;
        }
        match event::read().context("reading input")? {
            // Windows terminals emit both press and release; only act on press
            // or every key fires twice.
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                keys::handle(app, k.code, k.modifiers);
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
        if app.quit {
            return Ok(());
        }
    }
}
