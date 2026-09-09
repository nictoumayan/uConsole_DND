//! Phase 1 acceptance tests.
//!
//! Every assertion here is a rule that D&D Beyond applies and does not ship a
//! result for. If one of these breaks, a number on the sheet is wrong and you
//! will find out at the table rather than here — which is the whole point of
//! having them.

use vellum::ddb::Character;
use vellum::derive::{derive, tables::ability_modifier, tables::proficiency_bonus, Sheet};

fn fixture() -> Sheet {
    let raw = include_str!("fixtures/srd_rogue.json");
    let ch: Character = serde_json::from_str(raw).expect("fixture parses");
    derive(&ch)
}

fn skill(s: &Sheet, name: &str) -> i32 {
    s.skills.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no skill {name}")).value
}

fn save(s: &Sheet, name: &str) -> i32 {
    s.saves.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no save {name}")).value
}

#[test]
fn ability_modifier_rounds_down_for_negatives() {
    // (9 - 10) / 2 truncates to 0 in Rust; the correct modifier is -1.
    assert_eq!(ability_modifier(8), -1);
    assert_eq!(ability_modifier(9), -1);
    assert_eq!(ability_modifier(10), 0);
    assert_eq!(ability_modifier(11), 0);
    assert_eq!(ability_modifier(17), 3);
    assert_eq!(ability_modifier(3), -4);
}

#[test]
fn proficiency_bonus_steps_every_four_levels() {
    for (level, expected) in [(1, 2), (4, 2), (5, 3), (8, 3), (9, 4), (13, 5), (17, 6), (20, 6)] {
        assert_eq!(proficiency_bonus(level), expected, "level {level}");
    }
}

#[test]
fn score_modifiers_are_applied_to_base_stats() {
    // The single most important assertion in the suite: `stats` is NOT what
    // D&D Beyond displays. Two +1 feat modifiers take DEX from 15 to 17.
    let s = fixture();
    assert_eq!(s.score(vellum::derive::tables::Ability::Dex), 17);
    assert_eq!(s.score(vellum::derive::tables::Ability::Con), 12);
    assert_eq!(s.score(vellum::derive::tables::Ability::Str), 8);
}

#[test]
fn player_chosen_modifiers_still_count() {
    // Regression guard. Those DEX bonuses, every class skill proficiency and
    // both Expertise entries arrive with `isGranted: false`, because the
    // player chose them from a list. They are active. Filtering on that flag
    // silently drops half the sheet.
    let s = fixture();
    assert_eq!(s.score(vellum::derive::tables::Ability::Dex), 17, "isGranted:false score bonus dropped");
    assert_eq!(skill(&s, "Stealth"), 9, "isGranted:false expertise dropped");
    assert_eq!(skill(&s, "Acrobatics"), 6, "isGranted:false proficiency dropped");
}

#[test]
fn expertise_doubles_proficiency_and_stacks_once() {
    let s = fixture();
    assert_eq!(s.proficiency_bonus, 3);
    assert_eq!(skill(&s, "Stealth"), 9, "DEX +3 + expertise 6");
    assert_eq!(skill(&s, "Insight"), 8, "WIS +2 + expertise 6");
    assert_eq!(skill(&s, "Acrobatics"), 6, "DEX +3 + proficiency 3");
    assert_eq!(skill(&s, "Athletics"), 2, "STR -1 + proficiency 3");
    assert_eq!(skill(&s, "Perception"), 2, "WIS +2, not proficient");
}

#[test]
fn set_base_senses_take_the_maximum_never_the_sum() {
    // Two darkvision entries, 60 and 120. She sees 120ft, not 180ft.
    let s = fixture();
    let dv = s.senses.iter().find(|(n, _)| n == "darkvision").expect("darkvision");
    assert_eq!(dv.1, 120);
}

#[test]
fn light_armour_takes_full_dex_and_unequipped_shields_are_ignored() {
    let s = fixture();
    assert_eq!(s.armor_class.value, 14, "Leather 11 + DEX +3; shield is not equipped");
    assert!(s.armor_class.formula.contains("Leather"));
    assert!(!s.armor_class.formula.contains("Shield"));
}

#[test]
fn hit_points_add_con_per_level() {
    let s = fixture();
    assert_eq!(s.hp.max.value, 51, "base 43 + CON +1 x 8");
    assert_eq!(s.hp.current, 45, "51 - 6 removed");
    assert_eq!(s.hp.temporary, 0);
}

#[test]
fn saving_throws_use_class_proficiencies() {
    let s = fixture();
    assert_eq!(save(&s, "DEX"), 6, "+3 DEX, proficient");
    assert_eq!(save(&s, "INT"), 5, "+2 INT, proficient");
    assert_eq!(save(&s, "CHA"), 2, "+2 CHA, not proficient");
    assert_eq!(save(&s, "STR"), -1);
}

#[test]
fn conditional_modifiers_are_surfaced_not_folded_in() {
    // The 40ft fly speed only applies in dim light. It must never silently
    // become a number on the sheet.
    let s = fixture();
    assert_eq!(s.conditionals.len(), 1);
    assert!(s.conditionals[0].condition.contains("dim light"));
    assert!(s.conditionals[0].subject.contains("innate speed flying"));
}

#[test]
fn passive_perception_is_ten_plus_perception() {
    assert_eq!(fixture().passive_perception, 12);
}

#[test]
fn immunities_and_class_notes_are_collected() {
    let s = fixture();
    assert!(s.immunities.iter().any(|i| i == "magical sleep"));
    assert!(s.class_notes.iter().any(|n| n == "Sneak Attack 4d6"), "rogue 8 -> 4d6");
    assert_eq!(s.classes, "Rogue 8 (Assassin)");
    assert_eq!(s.total_level, 8);
}

/// Runs only if you have dropped a real snapshot at tests/fixtures/real/.
/// That directory is gitignored — see tests/fixtures/README.md.
#[test]
fn real_character_matches_dndbeyond() {
    let dir = std::path::Path::new("tests/fixtures/real");
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("skipping: no tests/fixtures/real/ — see tests/fixtures/README.md");
        return;
    };
    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).expect("reading snapshot");
        let ch: Character = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", path.display()));
        let s = derive(&ch);

        // Sanity floor: if these fail the payload shape changed underneath us.
        assert!(!s.name.is_empty(), "{}: no name", path.display());
        assert!(s.total_level > 0, "{}: no class levels", path.display());
        assert!(s.hp.max.value > 0, "{}: nonsense HP", path.display());
        assert!(s.armor_class.value >= 10, "{}: nonsense AC", path.display());
        eprintln!("{} -> {} / {} / AC {} / HP {}", path.display(), s.name, s.classes, s.armor_class.value, s.hp.max.value);
        checked += 1;
    }
    eprintln!("checked {checked} real snapshot(s)");
}

// -- hand-entered ability scores -------------------------------------------

#[test]
fn an_ability_override_replaces_the_computed_score() {
    use vellum::derive::{derive_with, AbilityOverrides};
    let raw = include_str!("fixtures/srd_rogue.json");
    let ch: Character = serde_json::from_str(raw).unwrap();

    let mut o = AbilityOverrides::new();
    o.insert("DEX".into(), 18);
    let s = derive_with(&ch, &o);
    assert_eq!(s.score(vellum::derive::tables::Ability::Dex), 18);
    // Untouched abilities still come from the payload.
    assert_eq!(s.score(vellum::derive::tables::Ability::Str), 8);
}

#[test]
fn correcting_a_score_fixes_everything_downstream_at_once() {
    // The whole reason the override lives on the score and not on armour
    // class: one correction has to fix AC, passive perception, saves, skills
    // and initiative together.
    use vellum::derive::{derive_with, AbilityOverrides};
    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();

    let before = derive_with(&ch, &AbilityOverrides::new());
    assert_eq!(before.armor_class.value, 14);
    assert_eq!(before.initiative.value, 3);

    let mut o = AbilityOverrides::new();
    o.insert("DEX".into(), 18);
    let after = derive_with(&ch, &o);

    assert_eq!(after.armor_class.value, 15, "AC did not follow the score");
    assert_eq!(after.initiative.value, 4, "initiative did not follow");
    assert_eq!(save(&after, "DEX"), 7, "save did not follow");
    assert_eq!(skill(&after, "Stealth"), 10, "expertise skill did not follow");
    assert_eq!(after.hp.max.value, before.hp.max.value, "DEX must not touch hit points");
}

#[test]
fn a_wisdom_correction_moves_passive_perception() {
    use vellum::derive::{derive_with, AbilityOverrides};
    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let mut o = AbilityOverrides::new();
    o.insert("WIS".into(), 16);
    let s = derive_with(&ch, &o);
    // WIS 16 -> +3, not proficient in Perception on the fixture -> PP 13.
    assert_eq!(s.modifier(vellum::derive::tables::Ability::Wis), 3);
    assert_eq!(s.passive_perception, 10 + skill(&s, "Perception"));
}

#[test]
fn proficiencies_are_split_by_kind_and_exclude_skills_and_saves() {
    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let p = &derive(&ch).proficiencies;
    // Skills and saving throws have their own rows; they must not leak in here.
    let all: Vec<&String> = p
        .languages
        .iter()
        .chain(&p.tools)
        .chain(&p.armor)
        .chain(&p.weapons)
        .collect();
    for banned in ["Stealth", "Acrobatics", "Dexterity Saving Throws"] {
        assert!(!all.iter().any(|s| s.as_str() == banned), "{banned} leaked into proficiencies");
    }
    assert!(all.iter().all(|s| !s.contains('-')), "slugs should be title-cased: {all:?}");
}

#[test]
fn hit_dice_come_from_class_level_and_die_size() {
    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let s = derive(&ch);
    assert_eq!(s.hit_dice.len(), 1, "one pool for a single-class character");
    assert_eq!(s.hit_dice[0].die, 8, "a Rogue's Hit Point Die is a d8");
    assert_eq!(s.hit_dice[0].total, 8, "one die per class level");
    assert_eq!(s.hit_dice[0].label(), "d8");
}
