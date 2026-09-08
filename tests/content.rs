//! Tab content and the HTML-to-text pass.

use vellum::content::{html_to_text, Tab};
use vellum::derive::derive;

#[test]
fn tabs_have_stable_digit_keys() {
    // Direct access beats cycling when you are three seconds into your turn,
    // so the digit must always match the position in the strip.
    for (i, t) in Tab::ALL.iter().enumerate() {
        assert_eq!(t.index(), i);
        assert_eq!(t.key(), char::from_digit(i as u32 + 1, 10).unwrap());
        assert_eq!(Tab::from_name(&(i + 1).to_string()), Some(*t));
        assert_eq!(Tab::from_name(t.label()), Some(*t));
    }
    assert_eq!(Tab::from_name("VITALS"), Tab::from_name("vitals"));
    assert!(Tab::from_name("nope").is_none());
    assert!(Tab::from_name("0").is_none());
    // One past the last tab, derived rather than hardcoded.
    let past = (Tab::ALL.len() + 1).to_string();
    assert!(Tab::from_name(&past).is_none());
}

#[test]
fn strips_the_tags_dndbeyond_actually_uses() {
    let html = "<p>Your quick thinking and <em>agility</em> allow you to \
                move and act quickly.</p>\n<p><strong>Bonus Action.</strong></p>";
    let text = html_to_text(html);
    assert!(!text.contains('<'), "tags survived: {text}");
    assert!(text.contains("Your quick thinking and agility"));
    assert!(text.contains("Bonus Action."));
}

#[test]
fn decodes_the_entities_that_show_up_in_rules_text() {
    assert_eq!(html_to_text("a &amp; b"), "a & b");
    assert_eq!(html_to_text("&lt;tag&gt;"), "<tag>");
    assert_eq!(html_to_text("foe&rsquo;s"), "foe's");
    assert_eq!(html_to_text("5&nbsp;feet"), "5 feet");
    assert_eq!(html_to_text("a&mdash;b"), "a-b");
    // An unknown entity drops rather than emitting a mangled literal.
    assert_eq!(html_to_text("x&zzzz;y"), "xy");
}

#[test]
fn list_items_become_bullets() {
    let text = html_to_text("<ul><li>Dash</li><li>Disengage</li></ul>");
    assert!(text.contains("- Dash"), "got: {text}");
    assert!(text.contains("- Disengage"));
}

#[test]
fn paragraph_breaks_survive_but_blank_runs_collapse() {
    // Every paragraph boundary yields exactly one blank line, however much
    // whitespace the markup had around it. That blank line is what makes the
    // detail view readable.
    assert_eq!(html_to_text("<p>one</p><p>two</p>"), "one\n\ntwo");
    assert_eq!(html_to_text("<p>one</p>\n\n\n<p>two</p>"), "one\n\ntwo");
    // No leading or trailing blank lines, whatever the markup did.
    let t = html_to_text("\n\n<p>only</p>\n\n");
    assert_eq!(t, "only", "got: {t:?}");
}

#[test]
fn empty_and_plain_input_are_handled() {
    assert_eq!(html_to_text(""), "");
    assert_eq!(html_to_text("no markup here"), "no markup here");
}

#[test]
fn class_features_are_filtered_to_the_character_s_level() {
    // The payload lists all twenty levels. Showing a level-8 rogue their
    // level-20 capstone is noise at best and misleading at worst.
    let ch: vellum::ddb::Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let rows = vellum::content::rows_for(Tab::Feats, &ch, &derive(&ch));
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    assert!(names.contains(&"Sneak Attack"), "level 1 feature missing");
    assert!(names.contains(&"Uncanny Dodge"), "level 5 feature missing");
    assert!(
        !names.contains(&"Stroke of Luck"),
        "level 20 capstone shown to a level 8 character: {names:?}"
    );
}

#[test]
fn racial_traits_marked_hide_in_sheet_are_omitted() {
    let ch: vellum::ddb::Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let rows = vellum::content::rows_for(Tab::Feats, &ch, &derive(&ch));
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    assert!(names.contains(&"Fey Ancestry"));
    assert!(
        !names.contains(&"Ability Score Increase"),
        "hideInSheet trait leaked into the list: {names:?}"
    );
}

#[test]
fn a_spell_from_two_sources_appears_once() {
    // Faerie Fire arrives from both the lineage and an item. It is still one
    // spell you can cast.
    let ch: vellum::ddb::Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let rows = vellum::content::rows_for(Tab::Spells, &ch, &derive(&ch));
    let ff = rows.iter().filter(|r| r.name == "Faerie Fire").count();
    assert_eq!(ff, 1, "duplicate spell rows: {:?}", rows.iter().map(|r| &r.name).collect::<Vec<_>>());
    // Cantrips sort before levelled spells.
    assert_eq!(rows.first().map(|r| r.name.as_str()), Some("Dancing Lights"));
}

#[test]
fn an_empty_snippet_falls_back_to_the_description() {
    let ch: vellum::ddb::Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let rows = vellum::content::rows_for(Tab::Feats, &ch, &derive(&ch));
    let skulker = rows.iter().find(|r| r.name == "Skulker").expect("Skulker feat");
    assert!(!skulker.snippet.trim().is_empty(), "blank row in the list");
    assert!(skulker.snippet.contains("adept at slipping"));
}
