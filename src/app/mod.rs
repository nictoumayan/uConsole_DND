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

use crate::portrait::Portrait;
use crate::session::Session;
use crate::{ddb, paths};

/// Fetch a character, cache it, and build everything the app needs from it.
///
/// The portrait is best effort: a missing avatar must never stop a character
/// from loading.
pub fn load_character(id: i64) -> Result<(ddb::Character, Session, std::path::PathBuf, Option<Portrait>)> {
    let raw = ddb::fetch::fetch_raw(id)?;
    let ch: ddb::Character =
        serde_json::from_str(&raw).context("D&D Beyond returned something that is not a character")?;
    if ch.name.is_empty() {
        anyhow::bail!("that payload parsed but has no character name");
    }

    paths::ensure_dir()?;
    std::fs::write(paths::snapshot_path(id)?, raw.as_bytes())?;
    std::fs::write(paths::default_id_path()?, id.to_string())?;

    let portrait = if ch.avatar_url.is_empty() {
        None
    } else {
        match crate::portrait::fetch_avatar(&ch.avatar_url) {
            Ok(bytes) => {
                let _ = std::fs::write(paths::avatar_path(id)?, &bytes);
                Portrait::decode(&bytes, 24, 12).ok()
            }
            Err(_) => None,
        }
    };

    let session_path = paths::session_path(id)?;
    let mut session = Session::load_or_seed(
        &session_path,
        id,
        ch.removed_hit_points,
        ch.temporary_hit_points,
        ch.inspiration,
    );
    if session.hit_dice_used.is_empty() {
        let pools: Vec<(String, u32)> = ch
            .classes
            .iter()
            .filter(|c| c.definition.hit_dice > 0 && c.hit_dice_used > 0)
            .map(|c| (format!("d{}", c.definition.hit_dice), c.hit_dice_used as u32))
            .collect();
        session.seed_hit_dice(&pools);
    }
    Ok((ch, session, session_path, portrait))
}

pub fn run(mut app: App) -> Result<()> {
    // A panic while the terminal is in raw mode leaves the user's shell
    // wrecked — no echo, no line editing. Restore first, then panic.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        previous(info);
    }));

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app)).context("drawing frame")?;

        // The fetch blocks for a second or two. Taking it here — after the
        // frame that says "fetching…" has already been painted — is what stops
        // the UI freezing with no explanation.
        if let Some(id) = app.pending_load.take() {
            match load_character(id) {
                Ok((ch, session, path, portrait)) => app.adopt(ch, session, path, portrait),
                Err(e) => app.load_failed(format!("{e:#}")),
            }
            continue;
        }

        // Block. An earlier version polled every 250ms on the theory that a
        // blocking read would miss resizes; it does not — crossterm delivers
        // Resize through read() like any other event. Four wakeups a second
        // doing nothing is not free on a device running off two 18650s for
        // longer than its battery comfortably lasts.
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
