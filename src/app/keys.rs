//! Key dispatch. Separated from the event loop so the whole keymap can be
//! driven from tests without a terminal attached.

use super::state::{App, Mode, NumberTarget};
use crate::dice::Advantage;
use crate::content::Tab;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

pub fn handle(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // Ctrl-C always quits, from any mode, including mid-filter.
    if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
        app.quit = true;
        return;
    }

    match app.mode {
        // Numeric entry: digits accumulate, enter applies, esc abandons.
        Mode::Number(_) => match code {
            KeyCode::Esc => app.escape(),
            KeyCode::Enter => app.commit_number(),
            KeyCode::Backspace => app.pop_digit(),
            KeyCode::Char(c) => app.push_digit(c),
            _ => {}
        },

        Mode::Conditions => match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('c') => app.escape(),
            KeyCode::Char('j') | KeyCode::Down => app.move_condition_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_condition_cursor(-1),
            KeyCode::Char(' ') | KeyCode::Enter => app.toggle_condition_at_cursor(),
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right => app.adjust_at_cursor(1),
            KeyCode::Char('-') | KeyCode::Left => app.adjust_at_cursor(-1),
            _ => {}
        },

        // A dice expression is text: `d` must not start a damage prompt and
        // `1` must not jump tabs while you are typing "1d20".
        Mode::Dice => match code {
            KeyCode::Esc => app.escape(),
            KeyCode::Enter => app.commit_dice(Advantage::Normal),
            KeyCode::Backspace => app.pop_dice(),
            KeyCode::Char(c) => app.push_dice(c),
            _ => {}
        },

        Mode::Abilities => match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => app.escape(),
            KeyCode::Char('j') | KeyCode::Down => app.move_ability_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_ability_cursor(-1),
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right => app.adjust_ability(1),
            KeyCode::Char('-') | KeyCode::Left => app.adjust_ability(-1),
            KeyCode::Char('0') | KeyCode::Backspace | KeyCode::Delete => {
                app.clear_ability_override()
            }
            _ => {}
        },

        // A URL is text: no digit may jump tabs and no letter may fire a
        // command while it is being pasted or typed.
        Mode::Load => match code {
            KeyCode::Esc => app.escape(),
            KeyCode::Enter => app.submit_load(),
            KeyCode::Backspace => app.pop_load(),
            KeyCode::Char(c) => app.push_load(c),
            _ => {}
        },

        Mode::RollLog => match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('l') => app.escape(),
            _ => {}
        },

        Mode::ShortRest => match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => app.escape(),
            KeyCode::Char('j') | KeyCode::Down => app.move_hit_die_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_hit_die_cursor(-1),
            // One die per press: "you can decide to spend an additional Hit
            // Point Die after each roll".
            KeyCode::Char(' ') | KeyCode::Char('s') => app.spend_hit_die(),
            _ => {}
        },

        Mode::Rest => match code {
            KeyCode::Char('s') => app.rest(false),
            KeyCode::Char('l') => app.rest(true),
            _ => app.escape(),
        },

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

            // Dice. `enter` rolls on the ROLL tab and opens the detail view
            // everywhere else — each tab's enter does the obvious thing for
            // the content it holds.
            KeyCode::Char('a') => app.roll_selected(Advantage::Advantage),
            KeyCode::Char('z') => app.roll_selected(Advantage::Disadvantage),
            KeyCode::Char('x') => app.start_dice(),

            // Limited uses. Lowercase spends, uppercase hands one back — for
            // the misclick, and for a DM who rules that one did not count.
            KeyCode::Char('u') => app.spend_use(),
            KeyCode::Char('U') => app.restore_use(),
            KeyCode::Char('l') => app.open_roll_log(),
            KeyCode::Char('o') => app.open_abilities(),
            KeyCode::Char('L') => app.open_load(),

            // Play state. These work from any tab — mid-combat you should not
            // have to navigate somewhere before you can take damage.
            KeyCode::Char('d') => app.start_number(NumberTarget::Damage),
            KeyCode::Char('h') => app.start_number(NumberTarget::Heal),
            KeyCode::Char('t') => app.start_number(NumberTarget::TempHp),
            KeyCode::Char('c') => app.open_conditions(),
            KeyCode::Char('r') => app.mode = Mode::Rest,
            KeyCode::Char('i') => app.toggle_inspiration(),
            // Death saves are bound only while you are actually dying, so
            // they can use the obvious letters without stealing them the rest
            // of the time. The footer says so when it matters.
            KeyCode::Char('s') if app.is_dying() => app.death_save(true),
            KeyCode::Char('f') if app.is_dying() => app.death_save(false),

            // Enter does the obvious thing for the row, not for the tab: a
            // row that rolls, rolls; anything else opens.
            KeyCode::Enter | KeyCode::Right if app.selected_is_rollable() => {
                app.roll_selected(Advantage::Normal)
            }
            KeyCode::Enter | KeyCode::Right => app.open_detail(),
            KeyCode::Char('D') => app.roll_damage(),
            KeyCode::Char('v') => app.open_detail(),
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
            KeyCode::Char(c @ '1'..='8') => {
                let i = c.to_digit(10).unwrap() as usize - 1;
                app.go_to_tab(Tab::ALL[i]);
            }
            _ => {}
        },
    }
}
