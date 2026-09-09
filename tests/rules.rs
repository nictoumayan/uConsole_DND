//! The 2024 rules engine. Sources: SRD 5.2.1 (CC-BY-4.0).

use vellum::derive::tables::Ability;
use vellum::dice::Advantage;
use vellum::rules::*;

fn c(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

const CHECK: TestKind = TestKind::Check { ability: Ability::Dex, skill: Some("stealth") };

#[test]
fn exhaustion_is_two_per_level_on_every_d20_test() {
    for (level, penalty, speed) in [(0u8, 0, 0), (1, -2, 5), (3, -6, 15), (5, -10, 25), (6, -12, 30)] {
        assert_eq!(exhaustion_penalty(level), penalty, "level {level}");
        assert_eq!(exhaustion_speed_penalty(level), speed, "level {level}");
    }
}

#[test]
fn exhaustion_applies_to_every_kind_of_test_including_death_saves() {
    for kind in [
        TestKind::Attack,
        CHECK,
        TestKind::Save { ability: Ability::Wis },
        TestKind::Initiative,
        TestKind::DeathSave,
    ] {
        let r = resolve(kind, &[], 3, &[], Advantage::Normal);
        assert_eq!(r.penalty, -6, "exhaustion missed {kind:?}");
    }
}

#[test]
fn poisoned_hampers_attacks_and_ability_checks_but_not_saves() {
    let poisoned = c(&["Poisoned"]);
    assert_eq!(
        resolve(TestKind::Attack, &poisoned, 0, &[], Advantage::Normal).advantage,
        Advantage::Disadvantage
    );
    assert_eq!(
        resolve(CHECK, &poisoned, 0, &[], Advantage::Normal).advantage,
        Advantage::Disadvantage
    );
    assert_eq!(
        resolve(TestKind::Save { ability: Ability::Con }, &poisoned, 0, &[], Advantage::Normal)
            .advantage,
        Advantage::Normal,
        "Poisoned does not touch saving throws"
    );
}

#[test]
fn initiative_is_an_ability_check_so_conditions_reach_it() {
    // The 2024 rules make Initiative a Dexterity check, which means being
    // Poisoned hampers it. This is easy to miss.
    let r = resolve(TestKind::Initiative, &c(&["Poisoned"]), 0, &[], Advantage::Normal);
    assert_eq!(r.advantage, Advantage::Disadvantage);
}

#[test]
fn restrained_hampers_dexterity_saves_only() {
    let r = c(&["Restrained"]);
    assert_eq!(
        resolve(TestKind::Save { ability: Ability::Dex }, &r, 0, &[], Advantage::Normal).advantage,
        Advantage::Disadvantage
    );
    assert_eq!(
        resolve(TestKind::Save { ability: Ability::Wis }, &r, 0, &[], Advantage::Normal).advantage,
        Advantage::Normal
    );
}

#[test]
fn paralysis_auto_fails_strength_and_dexterity_saves() {
    let p = c(&["Paralyzed"]);
    for a in [Ability::Str, Ability::Dex] {
        let r = resolve(TestKind::Save { ability: a }, &p, 0, &[], Advantage::Normal);
        assert!(r.auto_fail, "{a:?} save should fail automatically");
    }
    let r = resolve(TestKind::Save { ability: Ability::Wis }, &p, 0, &[], Advantage::Normal);
    assert!(!r.auto_fail, "Wisdom saves are not auto-failed");
}

#[test]
fn advantage_and_disadvantage_cancel_to_a_straight_roll() {
    // "If circumstances cause a roll to have both Advantage and Disadvantage,
    // the roll has neither of them, and you roll one d20."
    let r = resolve(CHECK, &c(&["Poisoned"]), 0, &[], Advantage::Advantage);
    assert_eq!(
        r.advantage,
        Advantage::Normal,
        "asking for advantage while Poisoned must produce a straight roll"
    );
    assert!(
        r.sources.iter().any(|s| s.contains("cancelled by") && s.contains("Poisoned")),
        "the player must be told what cancelled it: {:?}",
        r.sources
    );
}

#[test]
fn several_sources_on_the_same_side_still_mean_two_dice() {
    // "If multiple situations affect a roll and they all grant Advantage on
    // it, you still roll only two d20s."
    let r = resolve(TestKind::Attack, &c(&["Poisoned", "Frightened", "Prone"]), 0, &[], Advantage::Normal);
    assert_eq!(r.advantage, Advantage::Disadvantage);
}

#[test]
fn a_standing_advantage_from_the_sheet_is_applied_without_being_asked() {
    // A Rogue's class grants advantage on Initiative; rolling it should use it.
    let granted = c(&["initiative", "stealth", "death saving throws"]);
    assert_eq!(
        resolve(TestKind::Initiative, &[], 0, &granted, Advantage::Normal).advantage,
        Advantage::Advantage
    );
    assert_eq!(
        resolve(CHECK, &[], 0, &granted, Advantage::Normal).advantage,
        Advantage::Advantage,
        "advantage on stealth should apply to a stealth check"
    );
    assert_eq!(
        resolve(TestKind::DeathSave, &[], 0, &granted, Advantage::Normal).advantage,
        Advantage::Advantage
    );
    // ...but not to an unrelated check.
    let other = TestKind::Check { ability: Ability::Str, skill: Some("athletics") };
    assert_eq!(
        resolve(other, &[], 0, &granted, Advantage::Normal).advantage,
        Advantage::Normal
    );
}

#[test]
fn a_standing_advantage_still_cancels_against_a_condition() {
    let granted = c(&["initiative"]);
    let r = resolve(TestKind::Initiative, &c(&["Poisoned"]), 0, &granted, Advantage::Normal);
    assert_eq!(r.advantage, Advantage::Normal);
}

#[test]
fn conditions_do_not_touch_death_saves_but_exhaustion_does() {
    let r = resolve(TestKind::DeathSave, &c(&["Poisoned", "Prone"]), 0, &[], Advantage::Normal);
    assert_eq!(r.advantage, Advantage::Normal, "a death save is not an ability check");
    let e = resolve(TestKind::DeathSave, &[], 2, &[], Advantage::Normal);
    assert_eq!(e.penalty, -4);
}

#[test]
fn speed_falls_with_exhaustion_and_hits_zero_when_pinned() {
    assert_eq!(effective_speed(30, 0, &[]), 30);
    assert_eq!(effective_speed(30, 2, &[]), 20);
    assert_eq!(effective_speed(30, 8, &[]), 0, "speed floors at zero");
    // Stunned is deliberately absent: see stunned_does_not_zero_your_speed.
    for pinned in ["Grappled", "Restrained", "Paralyzed", "Unconscious", "Petrified"] {
        assert_eq!(effective_speed(30, 0, &c(&[pinned])), 0, "{pinned} should stop you");
    }
    assert_eq!(effective_speed(30, 0, &c(&["Poisoned"])), 30);
}

#[test]
fn unarmored_defense_matches_the_class() {
    assert_eq!(unarmored_defense("Barbarian", 3, 4, 1).unwrap().0, 17);
    assert_eq!(unarmored_defense("Monk", 3, 1, 4).unwrap().0, 17);
    assert!(unarmored_defense("Rogue", 3, 1, 4).is_none());
}

#[test]
fn spell_dc_and_attack_use_the_standard_formulas() {
    assert_eq!(spell_save_dc(3, 4), 15); // 8 + PB + mod
    assert_eq!(spell_attack_bonus(3, 4), 7);
}

#[test]
fn massive_damage_needs_the_remainder_to_reach_maximum() {
    assert!(is_massive_damage(120, 55, 55), "55 hp taking 120 leaves 65 over");
    assert!(!is_massive_damage(109, 55, 55), "54 over is one short");
    assert!(is_massive_damage(110, 55, 55));
    assert!(!is_massive_damage(999, 0, 55), "already at zero is a death save, not instant death");
}

#[test]
fn incapacitating_conditions_are_recognised() {
    for name in ["Incapacitated", "Paralyzed", "Petrified", "Stunned", "Unconscious"] {
        assert!(is_incapacitated(&c(&[name])), "{name} should incapacitate");
    }
    assert!(!is_incapacitated(&c(&["Poisoned", "Prone"])));
}

#[test]
fn carrying_capacity_is_strength_times_fifteen() {
    assert_eq!(carrying_capacity(8), 120);
    assert_eq!(carrying_capacity(20), 300);
}

#[test]
fn every_condition_in_the_session_list_has_a_rules_entry() {
    // The two lists must not drift apart.
    for name in vellum::session::CONDITIONS {
        let e = effects(name);
        let modelled = e.attack != Disposition::Neutral
            || e.ability_check != Disposition::Neutral
            || !e.save_auto_fail.is_empty()
            || !e.save_disadvantage.is_empty()
            || e.speed_zero
            || e.incapacitated
            || !e.note.is_empty();
        assert!(modelled, "{name} has no effects at all — is it spelled the same in both lists?");
    }
}

// -- corrections made after reading the official SRD 5.2.1 PDF -------------

#[test]
fn incapacitated_gives_disadvantage_on_initiative_specifically() {
    // "Surprised. If you're Incapacitated when you roll Initiative, you have
    // Disadvantage on the roll." Incapacitated does nothing to ability checks
    // in general, so this is invisible unless you read the entry.
    let inc = c(&["Incapacitated"]);
    assert_eq!(
        resolve(TestKind::Initiative, &inc, 0, &[], Advantage::Normal).advantage,
        Advantage::Disadvantage
    );
    assert_eq!(
        resolve(CHECK, &inc, 0, &[], Advantage::Normal).advantage,
        Advantage::Normal,
        "Incapacitated does not hamper ordinary ability checks"
    );
}

#[test]
fn invisible_gives_advantage_on_initiative_and_on_attacks() {
    // "Surprise. If you're Invisible when you roll Initiative, you have
    // Advantage on the roll."
    let inv = c(&["Invisible"]);
    assert_eq!(
        resolve(TestKind::Initiative, &inv, 0, &[], Advantage::Normal).advantage,
        Advantage::Advantage
    );
    assert_eq!(
        resolve(TestKind::Attack, &inv, 0, &[], Advantage::Normal).advantage,
        Advantage::Advantage
    );
    assert_eq!(
        resolve(CHECK, &inv, 0, &[], Advantage::Normal).advantage,
        Advantage::Normal,
        "Invisible does not help ordinary ability checks"
    );
}

#[test]
fn stunned_does_not_zero_your_speed() {
    // Grappled, Restrained, Paralyzed, Petrified and Unconscious each carry an
    // explicit "Speed 0" clause. Stunned does not — it only incapacitates.
    assert_eq!(effective_speed(30, 0, &c(&["Stunned"])), 30);
    assert!(is_incapacitated(&c(&["Stunned"])));
    for pinned in ["Grappled", "Restrained", "Paralyzed", "Petrified", "Unconscious"] {
        assert_eq!(effective_speed(30, 0, &c(&[pinned])), 0, "{pinned} should say Speed 0");
    }
}

#[test]
fn passive_perception_shifts_by_five_with_advantage_or_disadvantage() {
    // "A creature's Passive Perception equals 10 plus the creature's Wisdom
    // (Perception) check bonus. If the creature has Advantage on such checks,
    // increase the score by 5. If Disadvantage, decrease the score by 5."
    assert_eq!(passive_perception(6, &[], &[]), 16);
    assert_eq!(
        passive_perception(6, &c(&["Poisoned"]), &[]),
        11,
        "disadvantage on ability checks should lower it by 5"
    );
    assert_eq!(
        passive_perception(6, &[], &c(&["perception"])),
        21,
        "a standing advantage on Perception should raise it by 5"
    );
    // Cancelling leaves it unchanged.
    assert_eq!(passive_perception(6, &c(&["Poisoned"]), &c(&["perception"])), 16);
}

#[test]
fn the_srd_example_of_passive_perception_checks_out() {
    // "a level 1 character with a Wisdom of 15 and proficiency in Perception
    // has a Passive Perception of 14 (10 + 2 + 2)"
    assert_eq!(passive_perception(2 + 2, &[], &[]), 14);
}

#[test]
fn several_disadvantages_and_one_advantage_still_cancel() {
    // "This is true even if multiple circumstances impose Disadvantage and
    // only one grants Advantage."
    let r = resolve(
        TestKind::Attack,
        &c(&["Poisoned", "Frightened", "Prone"]),
        0,
        &[],
        Advantage::Advantage,
    );
    assert_eq!(r.advantage, Advantage::Normal);
}

#[test]
fn a_hit_die_always_heals_at_least_one() {
    // "You regain Hit Points equal to the total (minimum of 1 Hit Point)."
    // A 1 on a d8 with a -2 Constitution modifier still heals you.
    assert_eq!(hit_die_healing(1, -2), 1);
    assert_eq!(hit_die_healing(1, -5), 1);
    assert_eq!(hit_die_healing(5, 1), 6);
    assert_eq!(hit_die_healing(8, 3), 11);
}

#[test]
fn resting_requires_at_least_one_hit_point() {
    // "To start a Short Rest, you must have at least 1 Hit Point."
    assert!(!can_rest(0));
    assert!(can_rest(1));
    assert!(can_rest(55));
}

#[test]
fn attack_ability_follows_finesse_and_range() {
    // Finesse takes the better; ranged takes Dexterity; melee takes Strength.
    assert_eq!(attack_ability(false, true, -1, 3), Ability::Dex);
    assert_eq!(attack_ability(false, true, 4, 2), Ability::Str);
    assert_eq!(attack_ability(true, false, 4, 2), Ability::Dex, "ranged is always DEX");
    assert_eq!(attack_ability(false, false, -1, 3), Ability::Str, "plain melee is STR");
}

#[test]
fn weapon_proficiency_comes_from_category_or_name() {
    let simple = vec!["Simple Weapons".to_string()];
    assert!(weapon_proficient(1, "Dagger", &simple), "category 1 is Simple");
    assert!(!weapon_proficient(2, "Greatsword", &simple), "category 2 is Martial");

    let named = vec!["Rapier".to_string()];
    assert!(weapon_proficient(2, "Rapier", &named), "named individually");
    assert!(weapon_proficient(2, "rapier", &named), "and case-insensitively");
    assert!(!weapon_proficient(2, "Greatsword", &named));
}

// -- spellcasting ----------------------------------------------------------

#[test]
fn half_caster_slots_match_the_paladin_table() {
    // The only slot table encoded is the full-caster one. This pins the
    // derivation against the Paladin table's real numbers, extracted from the
    // SRD PDF — if the derivation were wrong, every Paladin would be wrong.
    let paladin: [(i32, [u8; 5]); 20] = [
        (1, [2, 0, 0, 0, 0]),   (2, [2, 0, 0, 0, 0]),
        (3, [3, 0, 0, 0, 0]),   (4, [3, 0, 0, 0, 0]),
        (5, [4, 2, 0, 0, 0]),   (6, [4, 2, 0, 0, 0]),
        (7, [4, 3, 0, 0, 0]),   (8, [4, 3, 0, 0, 0]),
        (9, [4, 3, 2, 0, 0]),   (10, [4, 3, 2, 0, 0]),
        (11, [4, 3, 3, 0, 0]),  (12, [4, 3, 3, 0, 0]),
        (13, [4, 3, 3, 1, 0]),  (14, [4, 3, 3, 1, 0]),
        (15, [4, 3, 3, 2, 0]),  (16, [4, 3, 3, 2, 0]),
        (17, [4, 3, 3, 3, 1]),  (18, [4, 3, 3, 3, 1]),
        (19, [4, 3, 3, 3, 2]),  (20, [4, 3, 3, 3, 2]),
    ];
    for (level, expected) in paladin {
        let got = single_class_slots(CasterKind::Half, level);
        assert_eq!(&got[..5], &expected[..], "Paladin {level}");
        assert_eq!(&got[5..], &[0u8; 4], "a half caster has no slots above 5th");
    }
}

#[test]
fn half_levels_round_up_in_a_multiclass_too() {
    // "Half your levels (round up) in the Paladin and Ranger classes." Round
    // up is the 2024 rule; 2014 rounded down. Because both roundings agree,
    // a level 3 Paladin is caster level 2 whether or not they multiclass.
    assert_eq!(caster_level(&[(CasterKind::Half, 3)]), 2);
    assert_eq!(single_class_slots(CasterKind::Half, 3)[0], 3);
    assert_eq!(spell_slots(&[(CasterKind::Half, 3)]), spell_slots(&[(CasterKind::Full, 2)]));
}

#[test]
fn the_srd_multiclass_example_works_out() {
    // "if you are a level 4 Ranger / level 3 Sorcerer, you count as a level 5
    // character... you have four level 1 spell slots, three level 2 slots"
    let classes = [(CasterKind::Half, 4), (CasterKind::Full, 3)];
    assert_eq!(caster_level(&classes), 5);
    let slots = spell_slots(&classes);
    assert_eq!(slots[0], 4);
    assert_eq!(slots[1], 3);
    assert_eq!(slots[2], 2);
}

#[test]
fn a_third_caster_contributes_a_third_rounded_up() {
    // Eldritch Knight and Arcane Trickster are not in SRD 5.2.1 — it ships one
    // subclass per class and omits both — so this branch follows the same
    // shape as the half-caster rule rather than extracted text.
    assert_eq!(caster_level(&[(CasterKind::Third, 3)]), 1);
    assert_eq!(caster_level(&[(CasterKind::Third, 4)]), 2);
    assert_eq!(caster_level(&[(CasterKind::Third, 7)]), 3);
    assert_eq!(single_class_slots(CasterKind::Third, 3)[0], 2, "two level 1 slots at Fighter 3");
}

#[test]
fn caster_kind_reads_the_subclass_for_third_casters() {
    assert_eq!(caster_kind("Wizard", ""), CasterKind::Full);
    assert_eq!(caster_kind("Paladin", "Oath of Devotion"), CasterKind::Half);
    assert_eq!(caster_kind("Warlock", ""), CasterKind::Pact);
    assert_eq!(caster_kind("Rogue", "Assassin"), CasterKind::None);
    // A third caster is a property of the subclass, not the class.
    assert_eq!(caster_kind("Rogue", "Arcane Trickster"), CasterKind::Third);
    assert_eq!(caster_kind("Fighter", "Eldritch Knight"), CasterKind::Third);
}

#[test]
fn a_non_caster_has_no_slots() {
    assert_eq!(spell_slots(&[(CasterKind::None, 8)]), [0; 9]);
    assert_eq!(spell_slots(&[]), [0; 9]);
}

#[test]
fn pact_magic_is_its_own_small_pool() {
    assert_eq!(pact_slots(1), (1, 1));
    assert_eq!(pact_slots(2), (2, 1));
    assert_eq!(pact_slots(5), (2, 3));
    assert_eq!(pact_slots(11), (3, 5));
    assert_eq!(pact_slots(17), (4, 5));
    assert_eq!(pact_slots(20), (4, 5));
    // And it is not part of the ordinary slot pool.
    assert_eq!(spell_slots(&[(CasterKind::Pact, 5)]), [0; 9]);
}

#[test]
fn slot_levels_never_exceed_the_table() {
    for level in 0..=25 {
        for kind in [CasterKind::Full, CasterKind::Half, CasterKind::Third] {
            let s = single_class_slots(kind, level);
            assert!(s.iter().all(|n| *n <= 4), "level {level} {kind:?} gave {s:?}");
        }
        let _ = pact_slots(level);
    }
}

#[test]
fn concentration_dc_is_ten_or_half_the_damage() {
    // "The DC equals 10 or half the damage taken (round down), whichever
    // number is higher."
    assert_eq!(concentration_dc(0), 10);
    assert_eq!(concentration_dc(19), 10, "half of 19 rounds down to 9");
    assert_eq!(concentration_dc(20), 10);
    assert_eq!(concentration_dc(21), 10, "half of 21 rounds down to 10");
    assert_eq!(concentration_dc(22), 11);
    assert_eq!(concentration_dc(50), 25);
}
