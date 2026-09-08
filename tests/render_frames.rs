//! Draws real frames through ratatui's TestBackend, at the uConsole's exact
//! geometry. Catches panics and layout regressions without a terminal.

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use vellum::app::{keys, ui, App};
use vellum::content::Tab;
use vellum::ddb::Character;
use vellum::derive::derive;

const COLS: u16 = 80;
const ROWS: u16 = 22;

fn app() -> App {
    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let sheet = derive(&ch);
    App::new(sheet, ch)
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
    keys::handle(&mut a, KeyCode::Char('6'), KeyModifiers::NONE);
    keys::handle(&mut a, KeyCode::Enter, KeyModifiers::NONE);
    let s = screen(&mut a);
    assert!(s.to_uppercase().contains("ALERT"), "detail title missing: {s}");
    assert!(s.contains("Initiative"), "detail body missing: {s}");
}

#[test]
fn the_filter_prompt_appears_in_the_footer_while_typing() {
    let mut a = app();
    keys::handle(&mut a, KeyCode::Char('6'), KeyModifiers::NONE);
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
    keys::handle(&mut a, KeyCode::Char('6'), KeyModifiers::NONE);
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
