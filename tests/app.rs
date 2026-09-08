//! The whole interaction model, exercised without a terminal.
//!
//! This is why `state` and `keys` do not depend on ratatui: every key the user
//! can press is checkable here, on CI, on a laptop, before the hardware
//! arrives.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use vellum::app::{keys, App, Mode};
use vellum::content::Tab;
use vellum::ddb::Character;
use vellum::derive::derive;

fn app() -> App {
    let ch: Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).expect("fixture parses");
    let sheet = derive(&ch);
    App::new(sheet, ch)
}

fn press(a: &mut App, c: KeyCode) {
    keys::handle(a, c, KeyModifiers::NONE);
}

fn typed(a: &mut App, text: &str) {
    for c in text.chars() {
        press(a, KeyCode::Char(c));
    }
}

#[test]
fn digits_jump_straight_to_a_tab() {
    let mut a = app();
    press(&mut a, KeyCode::Char('4'));
    assert_eq!(a.tab, Tab::Spells);
    press(&mut a, KeyCode::Char('1'));
    assert_eq!(a.tab, Tab::Vitals);
    press(&mut a, KeyCode::Char('7'));
    assert_eq!(a.tab, Tab::Notes);
    // 8 is not a tab and must be inert, not a panic or a wrap-around.
    press(&mut a, KeyCode::Char('8'));
    assert_eq!(a.tab, Tab::Notes);
}

#[test]
fn tab_cycles_both_ways_and_wraps() {
    let mut a = app();
    press(&mut a, KeyCode::BackTab);
    assert_eq!(a.tab, Tab::Notes, "shift-tab from the first tab wraps to the last");
    press(&mut a, KeyCode::Tab);
    assert_eq!(a.tab, Tab::Vitals);
}

#[test]
fn selection_is_remembered_per_tab() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6')); // FEATS, a long list
    for _ in 0..5 {
        press(&mut a, KeyCode::Char('j'));
    }
    let feats_at = a.selected();
    assert_eq!(feats_at, 5);

    press(&mut a, KeyCode::Char('5')); // GEAR
    assert_eq!(a.selected(), 0, "a fresh tab starts at the top");

    press(&mut a, KeyCode::Char('6')); // back to FEATS
    assert_eq!(a.selected(), feats_at, "returning to a tab should not lose your place");
}

#[test]
fn selection_cannot_leave_the_list() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    let n = a.rows().len();
    assert!(n > 0);
    for _ in 0..(n + 50) {
        press(&mut a, KeyCode::Char('j'));
    }
    assert_eq!(a.selected(), n - 1, "ran off the bottom");
    for _ in 0..(n + 50) {
        press(&mut a, KeyCode::Char('k'));
    }
    assert_eq!(a.selected(), 0, "ran off the top");
}

#[test]
fn g_and_shift_g_jump_to_the_ends() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('G'));
    assert_eq!(a.selected(), a.rows().len() - 1);
    press(&mut a, KeyCode::Char('g'));
    assert_eq!(a.selected(), 0);
}

#[test]
fn movement_keys_are_inert_on_fixed_panes() {
    // VITALS and SKILLS are not lists; j/k must not appear to do something.
    let mut a = app();
    for tab in [Tab::Vitals, Tab::Skills] {
        press(&mut a, KeyCode::Char(tab.key()));
        assert!(!a.is_list_tab());
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.selected(), 0);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.mode, Mode::List, "there is nothing to open on {}", tab.label());
        press(&mut a, KeyCode::Char('/'));
        assert_eq!(a.mode, Mode::List, "filtering a fixed pane is meaningless");
    }
}

#[test]
fn filter_narrows_the_list_and_is_case_insensitive() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    let all = a.rows().len();

    press(&mut a, KeyCode::Char('/'));
    assert_eq!(a.mode, Mode::Filter);
    typed(&mut a, "SNEAK");

    let hits = a.rows().len();
    assert!(hits > 0, "expected a Sneak Attack row");
    assert!(hits < all, "filter did not narrow anything");
    assert!(a.rows().iter().all(|r| {
        let hay = format!("{} {} {}", r.name, r.meta, r.snippet).to_lowercase();
        hay.contains("sneak")
    }));
}

#[test]
fn printable_keys_are_text_while_filtering_not_commands() {
    // Typing "javelin" must not jump tabs on the "1"-adjacent keys, and "q"
    // must not quit mid-word.
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "q1g");
    assert_eq!(a.filter, "q1g");
    assert_eq!(a.tab, Tab::Feats, "a digit changed tabs while filtering");
    assert!(!a.quit, "q quit while filtering");
}

#[test]
fn backspace_edits_the_filter() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Backspace);
    assert_eq!(a.filter, "snea");
}

#[test]
fn changing_tab_clears_the_filter() {
    // Carrying "fire" from SPELLS into GEAR would silently hide most of a kit.
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('5'));
    assert!(a.filter.is_empty(), "filter leaked across tabs");
}

#[test]
fn filtering_clamps_a_selection_that_falls_off_the_end() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('G')); // last row
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak"); // far fewer rows now
    assert!(a.selected() < a.rows().len(), "selection left the filtered list");
    assert!(a.selected_row().is_some());
}

#[test]
fn escape_backs_out_one_layer_at_a_time() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Enter); // leave filter mode, keep the filter
    assert_eq!(a.mode, Mode::List);
    assert!(!a.filter.is_empty());

    press(&mut a, KeyCode::Enter); // open detail
    assert_eq!(a.mode, Mode::Detail);

    press(&mut a, KeyCode::Esc); // detail -> list
    assert_eq!(a.mode, Mode::List);
    assert!(!a.quit);

    press(&mut a, KeyCode::Esc); // clears the filter
    assert!(a.filter.is_empty());
    assert!(!a.quit, "escape quit while a filter was still active");

    press(&mut a, KeyCode::Esc); // nothing left to back out of
    assert!(a.quit);
}

#[test]
fn detail_opens_the_selected_row_and_scrolls() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('j'));
    let expected = a.selected_row().unwrap().name;

    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::Detail);
    assert_eq!(a.selected_row().unwrap().name, expected);

    press(&mut a, KeyCode::Char('j'));
    assert_eq!(a.detail_scroll, 1);
    press(&mut a, KeyCode::Char('k'));
    assert_eq!(a.detail_scroll, 0);
    press(&mut a, KeyCode::Char('k'));
    assert_eq!(a.detail_scroll, 0, "scrolled above the top");
}

#[test]
fn n_and_p_walk_the_list_without_closing_the_detail() {
    // Reading down a list of spells one at a time is the common case.
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Enter);
    let first = a.selected_row().unwrap().name;

    press(&mut a, KeyCode::Char('j')); // scroll within the entry
    press(&mut a, KeyCode::Char('n')); // next entry
    assert_eq!(a.mode, Mode::Detail, "n closed the overlay");
    assert_ne!(a.selected_row().unwrap().name, first);
    assert_eq!(a.detail_scroll, 0, "scroll should reset on a new entry");

    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.selected_row().unwrap().name, first);
}

#[test]
fn q_quits_from_the_list_and_ctrl_c_from_anywhere() {
    let mut a = app();
    press(&mut a, KeyCode::Char('q'));
    assert!(a.quit);

    let mut b = app();
    press(&mut b, KeyCode::Char('6'));
    press(&mut b, KeyCode::Char('/'));
    typed(&mut b, "sneak");
    assert!(!b.quit);
    keys::handle(&mut b, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(b.quit, "ctrl-c must escape filter mode too");
}

#[test]
fn opening_detail_on_an_empty_filtered_list_is_a_no_op() {
    let mut a = app();
    press(&mut a, KeyCode::Char('6'));
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "zzzzzzzz");
    assert_eq!(a.rows().len(), 0);
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::List, "opened a detail view with nothing selected");
}
