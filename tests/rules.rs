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
    // Rihanne's class grants advantage on Initiative; rolling it should use it.
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
    for pinned in ["Grappled", "Restrained", "Paralyzed", "Stunned", "Unconscious", "Petrified"] {
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
