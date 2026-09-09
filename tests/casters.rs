//! The caster and multiclass fixtures. Everything else is tested against a
//! single-class Rogue with no magic, which left whole branches of the derive
//! layer unexercised.

use vellum::ddb::Character;
use vellum::derive::derive;

fn cleric() -> Character {
    serde_json::from_str(include_str!("fixtures/srd_cleric.json")).expect("cleric fixture")
}

fn multiclass() -> Character {
    serde_json::from_str(include_str!("fixtures/srd_multiclass.json")).expect("multiclass fixture")
}

#[test]
fn a_full_caster_gets_slots_from_the_class_table() {
    // The payload ships `available: 0` for every slot, exactly like armour
    // class and proficiency bonus, so these come from the tables.
    let s = derive(&cleric());
    let sc = s.spellcasting.expect("a Cleric casts");
    assert_eq!(sc.caster_level, 5);
    assert_eq!(&sc.slots[..3], &[4, 3, 2], "a level 5 full caster");
    assert_eq!(&sc.slots[3..], &[0; 6], "and nothing above 3rd");
    assert!(sc.pact.is_none());
}

#[test]
fn a_full_casters_dc_and_attack_come_from_its_ability() {
    // Cleric casts on Wisdom: 17 is +3, proficiency +3.
    let s = derive(&cleric());
    let sc = s.spellcasting.unwrap();
    assert_eq!(sc.ability, vellum::derive::tables::Ability::Wis);
    assert_eq!(sc.save_dc, 14, "8 + 3 + 3");
    assert_eq!(sc.attack_bonus, 6, "3 + 3");
}

#[test]
fn a_multiclass_caster_level_adds_the_halves_rounded_up() {
    // Paladin 4 contributes 2, Wizard 3 contributes 3.
    let s = derive(&multiclass());
    let sc = s.spellcasting.expect("both classes cast");
    assert_eq!(sc.caster_level, 5);
    assert_eq!(&sc.slots[..3], &[4, 3, 2]);
}

#[test]
fn multiclass_hit_dice_pools_stay_apart() {
    // A branch no test had ever run: 4d10 and 3d6, not seven of something.
    let s = derive(&multiclass());
    let mut pools: Vec<(i32, i32)> = s.hit_dice.iter().map(|p| (p.die, p.total)).collect();
    pools.sort();
    assert_eq!(pools, vec![(6, 3), (10, 4)]);
}

#[test]
fn medium_armour_caps_the_dexterity_bonus() {
    // Chain Shirt is 13, and the Cleric's DEX 12 is +1 — under the cap, so
    // this checks the medium branch applies at all.
    let s = derive(&cleric());
    assert_eq!(s.armor_class.value, 14);
    assert!(s.armor_class.formula.contains("Chain Shirt"));
}

#[test]
fn a_character_with_no_magic_reports_none() {
    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    assert!(derive(&ch).spellcasting.is_none(), "a Rogue has no slots to show");
}

#[test]
fn spells_carry_their_level_and_concentration_flag() {
    use vellum::content::{rows_for, Tab};
    let ch = cleric();
    let sheet = derive(&ch);
    let rows = rows_for(Tab::Spells, &ch, &sheet);

    let bless = rows.iter().find(|r| r.name == "Bless").expect("Bless");
    let spec = bless.spell.as_ref().expect("a spell spec");
    assert_eq!(spec.level, 1);
    assert!(spec.concentration, "Bless requires Concentration");

    let flame = rows.iter().find(|r| r.name == "Sacred Flame").expect("Sacred Flame");
    assert_eq!(flame.spell.as_ref().unwrap().level, 0, "a cantrip is level 0");

    // Cantrips sort first.
    assert_eq!(rows.first().map(|r| r.name.as_str()), Some("Sacred Flame"));
}
