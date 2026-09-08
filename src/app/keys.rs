//! Key dispatch. Separated from the event loop so the whole keymap can be
//! driven from tests without a terminal attached.

use super::state::{App, Mode};
use crate::content::Tab;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

pub fn handle(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // Ctrl-C always quits, from any mode, including mid-filter.
    if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
        app.quit = true;
        return;
    }

    match app.mode {
        // In filter mode every printable key is text, not a command —
        // otherwise you could not search for "javelin" without jumping tabs.
        Mode::Filter => match code {
            KeyCode::Esc => app.escape(),
            KeyCode::Enter => app.mode = Mode::List,
            KeyCode::Backspace => app.pop_filter(),
            KeyCode::Down => app.move_selection(1),
            KeyCode::Up => app.move_selection(-1),
            KeyCode::Char(c) => app.push_filter(c),
            _ => {}
        },

        Mode::Detail => match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => app.escape(),
            KeyCode::Char('j') | KeyCode::Down => app.scroll_detail(1),
            KeyCode::Char('k') | KeyCode::Up => app.scroll_detail(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => app.scroll_detail(app.page_rows as isize),
            KeyCode::PageUp => app.scroll_detail(-(app.page_rows as isize)),
            KeyCode::Char('g') | KeyCode::Home => app.detail_scroll = 0,
            // Move through the list without closing the overlay — reading down
            // a list of spells one at a time is the common case.
            KeyCode::Char('n') | KeyCode::Right => {
                app.move_selection(1);
                app.detail_scroll = 0;
            }
            KeyCode::Char('p') | KeyCode::Left => {
                app.move_selection(-1);
                app.detail_scroll = 0;
            }
            _ => {}
        },

        Mode::List => match code {
            KeyCode::Char('q') | KeyCode::Esc => app.escape(),
            KeyCode::Char('/') => app.start_filter(),
            KeyCode::Enter | KeyCode::Right => app.open_detail(),
            KeyCode::Char('j') | KeyCode::Down => app.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_selection(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => app.move_selection(app.page_rows as isize),
            KeyCode::PageUp => app.move_selection(-(app.page_rows as isize)),
            KeyCode::Char('g') | KeyCode::Home => app.select_first(),
            KeyCode::Char('G') | KeyCode::End => app.select_last(),
            KeyCode::Tab => app.cycle_tab(true),
            KeyCode::BackTab => app.cycle_tab(false),
            // Digits jump straight to a tab. Cycling to the pane you want is
            // fine at a desk and wrong three seconds into your turn.
            KeyCode::Char(c @ '1'..='7') => {
                let i = c.to_digit(10).unwrap() as usize - 1;
                app.go_to_tab(Tab::ALL[i]);
            }
            _ => {}
        },
    }
}
