//! The play-state rules. These are the ones that get argued about at a table,
//! so they are the ones worth pinning.

use vellum::session::{Session, CONDITIONS, MAX_EXHAUSTION};

const MAX: i32 = 55;

fn fresh() -> Session {
    Session::seed(1, 0, 0, false)
}

#[test]
fn seeding_agrees_with_the_snapshot() {
    // A first launch should match the website, not start you at full health.
    let s = Session::seed(1, 6, 4, true);
    assert_eq!(s.current_hp(MAX), 49);
    assert_eq!(s.temporary_hp, 4);
    assert!(s.inspiration);
}

#[test]
fn damage_comes_off_temporary_hit_points_first() {
    let mut s = fresh();
    s.set_temp_hp(8);
    s.take_damage(5, MAX);
    assert_eq!(s.temporary_hp, 3, "temp hp should absorb it");
    assert_eq!(s.current_hp(MAX), MAX, "real hp untouched");

    s.take_damage(10, MAX);
    assert_eq!(s.temporary_hp, 0);
    assert_eq!(s.current_hp(MAX), MAX - 7, "overflow carries to real hp");
}

#[test]
fn temporary_hit_points_do_not_stack_they_replace_when_better() {
    let mut s = fresh();
    s.set_temp_hp(5);
    s.set_temp_hp(3);
    assert_eq!(s.temporary_hp, 5, "a worse pool must not replace a better one");
    s.set_temp_hp(9);
    assert_eq!(s.temporary_hp, 9);
}

#[test]
fn hit_points_never_go_below_zero() {
    let mut s = fresh();
    s.take_damage(9999, MAX);
    assert_eq!(s.current_hp(MAX), 0);
    assert_eq!(s.damage, MAX, "damage should cap at max, not run away");
}

#[test]
fn dropping_to_zero_knocks_you_out_and_resets_death_saves() {
    let mut s = fresh();
    s.succeed_death_save();
    s.take_damage(MAX, MAX);
    assert!(s.is_dying(MAX));
    assert_eq!(s.death_successes, 0, "prior progress should not carry over");
    assert!(s.has_condition("Unconscious"));
}

#[test]
fn damage_taken_at_zero_is_a_failed_death_save() {
    // The rule people forget mid-fight.
    let mut s = fresh();
    s.take_damage(MAX, MAX);
    assert_eq!(s.death_failures, 0);
    s.take_damage(3, MAX);
    assert_eq!(s.death_failures, 1);
    assert_eq!(s.current_hp(MAX), 0, "already at zero, stays at zero");
}

#[test]
fn healing_off_zero_clears_death_saves_and_wakes_you() {
    let mut s = fresh();
    s.take_damage(MAX, MAX);
    s.fail_death_save();
    s.fail_death_save();
    s.heal(1, MAX);
    assert_eq!(s.current_hp(MAX), 1);
    assert_eq!(s.death_failures, 0);
    assert!(!s.has_condition("Unconscious"));
    assert!(!s.is_dying(MAX));
}

#[test]
fn healing_cannot_exceed_maximum() {
    let mut s = fresh();
    s.take_damage(10, MAX);
    s.heal(9999, MAX);
    assert_eq!(s.current_hp(MAX), MAX);
    assert_eq!(s.damage, 0);
}

#[test]
fn three_failures_is_dead_three_successes_is_stable() {
    let mut s = fresh();
    s.take_damage(MAX, MAX);
    for _ in 0..5 {
        s.fail_death_save();
    }
    assert_eq!(s.death_failures, 3, "should not count past three");
    assert!(s.is_dead());
    assert!(!s.is_dying(MAX), "dead is not dying");

    let mut t = fresh();
    t.take_damage(MAX, MAX);
    for _ in 0..5 {
        t.succeed_death_save();
    }
    assert_eq!(t.death_successes, 3);
    assert!(t.is_stable());
}

#[test]
fn conditions_toggle_and_never_duplicate() {
    let mut s = fresh();
    s.add_condition("Poisoned");
    s.add_condition("Poisoned");
    assert_eq!(s.conditions.len(), 1);
    s.toggle_condition("Poisoned");
    assert!(!s.has_condition("Poisoned"));
    s.toggle_condition("Poisoned");
    assert!(s.has_condition("Poisoned"));
}

#[test]
fn exhaustion_is_clamped_to_the_legal_range() {
    let mut s = fresh();
    s.adjust_exhaustion(-1);
    assert_eq!(s.exhaustion, 0, "cannot go negative");
    for _ in 0..10 {
        s.adjust_exhaustion(1);
    }
    assert_eq!(s.exhaustion, MAX_EXHAUSTION);
}

#[test]
fn a_long_rest_restores_and_removes_one_exhaustion() {
    let mut s = fresh();
    s.take_damage(30, MAX);
    s.set_temp_hp(5);
    s.adjust_exhaustion(3);
    s.add_condition("Poisoned");
    s.fail_death_save();

    s.long_rest();

    assert_eq!(s.current_hp(MAX), MAX);
    assert_eq!(s.temporary_hp, 0, "a temp pool does not survive the night");
    assert_eq!(s.exhaustion, 2, "one level, not all of them");
    assert_eq!(s.death_failures, 0);
    // Poison is not something a long rest ends on its own.
    assert!(s.has_condition("Poisoned"));
}

#[test]
fn the_condition_summary_reads_as_a_status_chip() {
    let mut s = fresh();
    assert_eq!(s.condition_summary(), "");
    s.add_condition("Poisoned");
    s.add_condition("Prone");
    s.adjust_exhaustion(2);
    assert_eq!(s.condition_summary(), "Poisoned, Prone, Exhaustion 2");
}

#[test]
fn round_trips_through_disk() {
    let dir = std::env::temp_dir().join(format!("vellum-session-{}", std::process::id()));
    let path = dir.join("s.json");
    let mut s = fresh();
    s.take_damage(12, MAX);
    s.add_condition("Frightened");
    s.adjust_exhaustion(1);
    s.save(&path).expect("saves");

    let back = Session::load_or_seed(&path, 1, 0, 0, false);
    assert_eq!(back.damage, 12);
    assert!(back.has_condition("Frightened"));
    assert_eq!(back.exhaustion, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_session_for_a_different_character_is_ignored_not_applied() {
    // Loading character B's hit points onto character A would be worse than
    // starting fresh.
    let dir = std::env::temp_dir().join(format!("vellum-session-x-{}", std::process::id()));
    let path = dir.join("s.json");
    let mut s = Session::seed(999, 0, 0, false);
    s.take_damage(40, MAX);
    s.save(&path).expect("saves");

    let back = Session::load_or_seed(&path, 1, 6, 0, false);
    assert_eq!(back.character_id, 1);
    assert_eq!(back.damage, 6, "should have seeded from the snapshot instead");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_corrupt_session_falls_back_to_seeding_rather_than_failing() {
    // The sheet is the thing you need at the table; a bad file must not block it.
    let dir = std::env::temp_dir().join(format!("vellum-session-c-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.json");
    std::fs::write(&path, "{ this is not json").unwrap();

    let back = Session::load_or_seed(&path, 1, 6, 2, false);
    assert_eq!(back.damage, 6);
    assert_eq!(back.temporary_hp, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_condition_list_is_the_srd_set() {
    assert_eq!(CONDITIONS.len(), 14);
    assert!(CONDITIONS.contains(&"Unconscious"));
    assert!(CONDITIONS.contains(&"Grappled"));
    // Exhaustion has levels, so it is tracked separately rather than as a toggle.
    assert!(!CONDITIONS.contains(&"Exhaustion"));
}

// -- limited uses ----------------------------------------------------------

#[test]
fn spending_a_use_saturates_at_the_maximum() {
    let mut s = fresh();
    for _ in 0..10 {
        s.spend_use("action:1", 2, "short rest");
    }
    assert_eq!(s.uses_of("action:1"), 2);
    assert_eq!(s.remaining("action:1", 2), 0);
}

#[test]
fn restoring_hands_one_back_and_forgets_the_entry_at_zero() {
    // Keeping a zero-use entry around would bloat the file with every ability
    // the character has ever used once.
    let mut s = fresh();
    s.spend_use("action:1", 3, "short rest");
    s.spend_use("action:1", 3, "short rest");
    assert_eq!(s.remaining("action:1", 3), 1);

    s.restore_use("action:1");
    assert_eq!(s.uses_of("action:1"), 1);
    s.restore_use("action:1");
    assert_eq!(s.uses_of("action:1"), 0);
    assert!(!s.uses.contains_key("action:1"), "spent entry not cleaned up");

    // Restoring below zero is a no-op, not an underflow.
    s.restore_use("action:1");
    assert_eq!(s.uses_of("action:1"), 0);
}

#[test]
fn an_untracked_key_reads_as_fully_available() {
    let s = fresh();
    assert_eq!(s.uses_of("item:99"), 0);
    assert_eq!(s.remaining("item:99", 3), 3);
}

#[test]
fn a_short_rest_restores_only_what_recharges_on_one() {
    let mut s = fresh();
    s.spend_use("action:short", 1, "short rest");
    s.spend_use("spell:long", 1, "long rest");
    s.spend_use("item:dawn", 1, "dawn");

    s.short_rest();

    assert_eq!(s.uses_of("action:short"), 0, "short-rest use not restored");
    assert_eq!(s.uses_of("spell:long"), 1, "long-rest use wrongly restored");
    assert_eq!(s.uses_of("item:dawn"), 1, "dawn use wrongly restored");
}

#[test]
fn a_long_rest_restores_everything() {
    let mut s = fresh();
    s.spend_use("action:short", 1, "short rest");
    s.spend_use("spell:long", 1, "long rest");
    s.spend_use("item:dawn", 2, "dawn");
    s.spend_use("action:weird", 1, "special");

    s.long_rest();

    assert!(s.uses.is_empty(), "something survived a long rest: {:?}", s.uses);
    assert_eq!(s.expended_count(), 0);
}

#[test]
fn the_recharge_type_is_stored_with_the_count() {
    // Stored alongside rather than looked up from the snapshot, so a rest
    // works from the session alone and a re-import cannot orphan it.
    let dir = std::env::temp_dir().join(format!("vellum-uses-{}", std::process::id()));
    let path = dir.join("s.json");
    let mut s = fresh();
    s.spend_use("action:1", 2, "short rest");
    s.spend_use("spell:2", 1, "long rest");
    s.save(&path).expect("saves");

    let mut back = Session::load_or_seed(&path, 1, 0, 0, false);
    back.short_rest();
    assert_eq!(back.uses_of("action:1"), 0);
    assert_eq!(back.uses_of("spell:2"), 1, "recharge type did not survive the round trip");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_session_written_before_uses_existed_still_loads() {
    // The field is #[serde(default)], so an older file must not fail to parse.
    let dir = std::env::temp_dir().join(format!("vellum-old-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.json");
    std::fs::write(
        &path,
        r#"{"version":1,"character_id":1,"damage":7,"temporary_hp":0,
            "conditions":[],"exhaustion":0,"death_successes":0,
            "death_failures":0,"inspiration":false}"#,
    )
    .unwrap();

    let s = Session::load_or_seed(&path, 1, 0, 0, false);
    assert_eq!(s.damage, 7, "an older session should still load");
    assert!(s.uses.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ability_overrides_are_clamped_to_the_legal_range() {
    // A typo must not produce a +145 modifier.
    let mut s = fresh();
    s.set_ability_override("DEX", 300);
    assert_eq!(s.ability_override("DEX"), Some(30));
    s.set_ability_override("DEX", -5);
    assert_eq!(s.ability_override("DEX"), Some(1));
    s.clear_ability_override("DEX");
    assert_eq!(s.ability_override("DEX"), None);
}

#[test]
fn ability_overrides_survive_a_round_trip() {
    let dir = std::env::temp_dir().join(format!("vellum-abil-{}", std::process::id()));
    let path = dir.join("s.json");
    let mut s = fresh();
    s.set_ability_override("DEX", 18);
    s.set_ability_override("WIS", 16);
    s.save(&path).expect("saves");

    let back = Session::load_or_seed(&path, 1, 0, 0, false);
    assert_eq!(back.ability_override("DEX"), Some(18));
    assert_eq!(back.ability_override("WIS"), Some(16));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn massive_damage_kills_outright() {
    // "When damage reduces a character to 0 Hit Points and damage remains, the
    // character dies if the remainder equals or exceeds their Hit Point
    // maximum." No saves, no dying — dead.
    let mut s = fresh();
    s.take_damage(MAX * 2, MAX);
    assert!(s.is_dead(), "a hit for double max hp should not leave you dying");
    assert!(!s.is_dying(MAX));

    // Exactly enough to zero you but not enough remainder is survivable.
    let mut t = fresh();
    t.take_damage(MAX, MAX);
    assert!(t.is_dying(MAX), "exactly lethal damage should leave you dying");
    assert!(!t.is_dead());

    // Wounded first, so the remainder is what counts, not the raw number.
    let mut u = fresh();
    u.take_damage(20, MAX);          // at 35 of 55
    u.take_damage(35 + MAX - 1, MAX); // remainder one short of max
    assert!(u.is_dying(MAX), "remainder below max should not kill outright");
}
