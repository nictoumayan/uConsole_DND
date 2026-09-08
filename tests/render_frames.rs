//! Draws real frames through ratatui's TestBackend, at the uConsole's exact
//! geometry. Catches panics and layout regressions without a terminal.

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use vellum::app::{keys, ui, App};
use vellum::content::Tab;
use vellum::ddb::Character;
use vellum::derive::derive;
use vellum::session::Session;

const COLS: u16 = 80;
const ROWS: u16 = 22;

fn app() -> App {
    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let sheet = derive(&ch);
    // A temp path per test: these must never touch the real session file.
    let path = std::env::temp_dir().join(format!(
        "vellum-test-{}-{:?}.json",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let session = Session::seed(ch.id, ch.removed_hit_points, ch.temporary_hit_points, false);
    App::new(sheet, ch, session, path)
}

fn screen(app: &mut App) -> String {
    let mut term = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
    term.draw(|f| ui::draw(f, app, None)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..ROWS)
        .map(|y| {
            (0..COLS)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_tab_draws_at_panel_size_without_panicking() {
    let mut a = app();
    for tab in Tab::ALL {
        keys::handle(&mut a, KeyCode::Char(tab.key()), KeyModifiers::NONE);
        let s = screen(&mut a);
        assert!(!s.is_empty(), "{} drew nothing", tab.label());
        assert!(
            s.contains("TEST ROGUE"),
            "{} lost the status bar: {s}",
            tab.label()
        );
    }
}

#[test]
fn hp_and_ac_are_on_screen_on_every_tab() {
    // Mid-combat you glance, you do not navigate.
    let mut a = app();
    for tab in Tab::ALL {
        keys::handle(&mut a, KeyCode::Char(tab.key()), KeyModifiers::NONE);
        let s = screen(&mut a);
        assert!(s.contains("45/51"), "{} lost HP", tab.label());
        assert!(s.contains("AC 14"), "{} lost AC", tab.label());
    }
}

#[test]
fn the_detail_overlay_shows_the_selected_row() {
    let mut a = app();
    keys::handle(&mut a, KeyCode::Char(Tab::Feats.key()), KeyModifiers::NONE);
    keys::handle(&mut a, KeyCode::Enter, KeyModifiers::NONE);
    let s = screen(&mut a);
    assert!(s.to_uppercase().contains("ALERT"), "detail title missing: {s}");
    assert!(s.contains("Initiative"), "detail body missing: {s}");
}

#[test]
fn the_filter_prompt_appears_in_the_footer_while_typing() {
    let mut a = app();
    keys::handle(&mut a, KeyCode::Char(Tab::Feats.key()), KeyModifiers::NONE);
    keys::handle(&mut a, KeyCode::Char('/'), KeyModifiers::NONE);
    for c in "dark".chars() {
        keys::handle(&mut a, KeyCode::Char(c), KeyModifiers::NONE);
    }
    let s = screen(&mut a);
    assert!(s.contains("/dark"), "filter prompt not drawn: {s}");
    assert!(s.contains("Darkvision"), "filtered list not drawn: {s}");
}

#[test]
fn an_empty_filter_result_says_so_rather_than_drawing_a_blank_pane() {
    let mut a = app();
    keys::handle(&mut a, KeyCode::Char(Tab::Feats.key()), KeyModifiers::NONE);
    keys::handle(&mut a, KeyCode::Char('/'), KeyModifiers::NONE);
    for c in "zzzz".chars() {
        keys::handle(&mut a, KeyCode::Char(c), KeyModifiers::NONE);
    }
    let s = screen(&mut a);
    assert!(s.contains("no match"), "silent empty pane: {s}");
}

#[test]
fn it_survives_a_terminal_far_smaller_than_the_panel() {
    // The uConsole is fixed, but a dev laptop window is not.
    let mut a = app();
    for (w, h) in [(20u16, 6u16), (40, 10), (200, 60)] {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| ui::draw(f, &mut a, None)).unwrap();
    }
}

// -- phase 3: play state on screen -----------------------------------------

fn keys(a: &mut App, seq: &[KeyCode]) {
    for k in seq {
        keys::handle(a, *k, KeyModifiers::NONE);
    }
}

#[test]
fn the_status_bar_grows_a_row_only_when_there_is_something_to_say() {
    // Twenty-two rows is too few to reserve space for a usually-blank line.
    let mut a = app();
    let healthy = screen(&mut a);
    assert!(!healthy.contains("DYING"));

    keys(&mut a, &[KeyCode::Char('c'), KeyCode::Char(' ')]); // Blinded on
    keys(&mut a, &[KeyCode::Esc]);
    let with_condition = screen(&mut a);
    assert!(with_condition.contains("Blinded"), "condition not on the status bar");
    assert!(
        with_condition.lines().count() >= healthy.lines().count(),
        "status bar did not grow"
    );
}

#[test]
fn dying_is_impossible_to_miss() {
    let mut a = app();
    keys(&mut a, &[KeyCode::Char('d')]);
    keys(&mut a, &[KeyCode::Char('9'), KeyCode::Char('9')]);
    keys(&mut a, &[KeyCode::Enter]);

    let s = screen(&mut a);
    assert!(s.contains("HP 0/"), "hit points not zeroed: {s}");
    assert!(s.contains("DYING"), "no dying banner: {s}");
    assert!(s.contains("Unconscious"), "unconscious not applied: {s}");
    // The footer should offer the keys you actually need right now.
    assert!(s.contains("s success"), "death save keys not offered: {s}");

    keys(&mut a, &[KeyCode::Char('s'), KeyCode::Char('f')]);
    let s = screen(&mut a);
    assert!(s.contains("●○○"), "death save pips missing: {s}");
}

#[test]
fn the_number_prompt_shows_what_you_are_typing() {
    let mut a = app();
    keys(&mut a, &[KeyCode::Char('d'), KeyCode::Char('1'), KeyCode::Char('2')]);
    let s = screen(&mut a);
    assert!(s.contains("damage: 12"), "prompt not drawn: {s}");
    assert!(s.contains("esc cancel"));
}

#[test]
fn the_conditions_overlay_draws_all_fourteen_plus_exhaustion() {
    let mut a = app();
    keys(&mut a, &[KeyCode::Char('c')]);
    let s = screen(&mut a);
    for name in vellum::session::CONDITIONS {
        assert!(s.contains(name), "{name} missing from the overlay: {s}");
    }
    assert!(s.contains("Exhaustion"), "exhaustion row missing");
}

#[test]
fn the_rest_menu_says_what_each_rest_actually_does() {
    let mut a = app();
    keys(&mut a, &[KeyCode::Char('r')]);
    let s = screen(&mut a);
    assert!(s.contains("short rest") && s.contains("long rest"));
    assert!(s.contains("full hit points"), "long rest effects not spelled out");
}
