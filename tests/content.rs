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

#[test]
fn limited_uses_parse_from_both_shapes_dndbeyond_uses() {
    // Actions and spells carry a numeric resetType; inventory items carry a
    // string. Both have to work or one wand fails the whole import.
    let ch: vellum::ddb::Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let sheet = derive(&ch);

    let actions = vellum::content::rows_for(Tab::Actions, &ch, &sheet);
    let uncanny = actions.iter().find(|r| r.name == "Uncanny Dodge").expect("action");
    let u = uncanny.uses.as_ref().expect("numeric resetType 1 not parsed");
    assert_eq!(u.max, 2);
    assert_eq!(u.reset, "short rest");
    assert_eq!(u.key, "action:501", "key must be stable across re-imports");

    let spells = vellum::content::rows_for(Tab::Spells, &ch, &sheet);
    let ff = spells.iter().find(|r| r.name == "Faerie Fire").expect("spell");
    assert_eq!(ff.uses.as_ref().expect("resetType 2").reset, "long rest");

    let gear = vellum::content::rows_for(Tab::Gear, &ch, &sheet);
    let shield = gear.iter().find(|r| r.name == "Shield").expect("item");
    let s = shield.uses.as_ref().expect("string resetType not parsed");
    assert_eq!(s.max, 3);
    assert_eq!(s.reset, "dawn");
}

#[test]
fn an_entry_without_a_cap_is_not_a_limited_resource() {
    // `maxNumberConsumed` with no `maxUses` is an upcastable spell, not a
    // charge. Treating it as one would show a pip that never depletes.
    let ch: vellum::ddb::Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let sheet = derive(&ch);
    let rows = vellum::content::rows_for(Tab::Spells, &ch, &sheet);
    let dancing = rows.iter().find(|r| r.name == "Dancing Lights").expect("cantrip");
    assert!(dancing.uses.is_none(), "a spell with no cap got a use tracker");
}

#[test]
fn rows_without_limited_uses_carry_none() {
    let ch: vellum::ddb::Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let sheet = derive(&ch);
    let rows = vellum::content::rows_for(Tab::Feats, &ch, &sheet);
    assert!(rows.iter().all(|r| r.uses.is_none()));
}
