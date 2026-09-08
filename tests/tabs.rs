//! Layout invariants. The whole point of the tabbed design is that it fits the
//! panel; a test that does not check the geometry is not testing the design.

use vellum::content::Tab;
use vellum::ddb::Character;
use vellum::derive::derive;
use vellum::tabs::{self, COLS, ROWS};

fn character() -> Character {
    serde_json::from_str(include_str!("fixtures/srd_rogue.json")).expect("fixture parses")
}

#[test]
fn every_tab_renders_exactly_one_panel() {
    let ch = character();
    let sheet = derive(&ch);
    for tab in Tab::ALL {
        let out = tabs::render(&tabs::View {
            sheet: &sheet,
            character: &ch,
            tab,
            scroll: 0,
            portrait: None,
            ascii_portrait: true,
        });
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), ROWS, "{} produced {} rows", tab.label(), lines.len());
        for (i, l) in lines.iter().enumerate() {
            assert!(
                l.chars().count() <= COLS,
                "{} row {i} is {} cols: {l:?}",
                tab.label(),
                l.chars().count()
            );
        }
    }
}

#[test]
fn the_status_bar_is_on_every_tab() {
    // HP and AC are what you glance at mid-combat; they must never be a tab
    // away.
    let ch = character();
    let sheet = derive(&ch);
    for tab in Tab::ALL {
        let out = tabs::render(&tabs::View {
            sheet: &sheet,
            character: &ch,
            tab,
            scroll: 0,
            portrait: None,
            ascii_portrait: true,
        });
        let head = out.lines().take(2).collect::<Vec<_>>().join(" ");
        assert!(head.contains("HP 45/51"), "{}: {head}", tab.label());
        assert!(head.contains("AC 14"), "{}: {head}", tab.label());
        assert!(head.contains("TEST ROGUE"), "{}", tab.label());
    }
}

#[test]
fn the_selected_tab_is_marked_in_the_strip() {
    let ch = character();
    let sheet = derive(&ch);
    for tab in Tab::ALL {
        let out = tabs::render(&tabs::View {
            sheet: &sheet,
            character: &ch,
            tab,
            scroll: 0,
            portrait: None,
            ascii_portrait: true,
        });
        let strip = out.lines().nth(3).unwrap();
        assert!(
            strip.contains(&format!("[{}·{}]", tab.key(), tab.label())),
            "{} not marked in: {strip}",
            tab.label()
        );
    }
}

#[test]
fn scrolling_past_the_end_clamps_instead_of_blanking() {
    let ch = character();
    let sheet = derive(&ch);
    let at = |scroll| {
        tabs::render(&tabs::View {
            sheet: &sheet,
            character: &ch,
            tab: Tab::Feats,
            scroll,
            portrait: None,
            ascii_portrait: true,
        })
    };
    // A blank content pane is indistinguishable from a broken tab.
    let far = at(9999);
    let content: String = far.lines().skip(5).take(15).collect();
    assert!(!content.trim().is_empty(), "scrolled past the end into a blank pane");
}

#[test]
fn detail_view_also_fits_the_panel() {
    let ch = character();
    let rows = vellum::content::rows_for(Tab::Feats, &ch, 8);
    if let Some(row) = rows.first() {
        let out = tabs::detail(row, 0);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() <= ROWS, "detail is {} rows", lines.len());
        for l in &lines {
            assert!(l.chars().count() <= COLS, "detail row too wide: {l:?}");
        }
    }
}
