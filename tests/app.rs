//! The whole interaction model, exercised without a terminal.
//!
//! This is why `state` and `keys` do not depend on ratatui: every key the user
//! can press is checkable here, on CI, on a laptop, before the hardware
//! arrives.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use vellum::app::{keys, App, Mode};
use vellum::content::Tab;
use vellum::ddb::Character;
use vellum::derive::derive;
use vellum::session::Session;

fn app() -> App {
    let ch: Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).expect("fixture parses");
    let sheet = derive(&ch);
    // A temp path per test: these must never touch the real session file.
    let path = std::env::temp_dir().join(format!(
        "vellum-test-{}-{:?}.json",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let session = Session::seed(ch.id, ch.removed_hit_points, ch.temporary_hit_points, false);
    App::new(sheet, ch, session, path, None)
}

fn press(a: &mut App, c: KeyCode) {
    keys::handle(a, c, KeyModifiers::NONE);
}

/// Drop to exactly 0 hit points without triggering the massive-damage rule,
/// which kills outright when the remainder reaches your maximum.
fn drop_to_zero(a: &mut App) {
    let exact = a.current_hp().to_string();
    press(a, KeyCode::Char('d'));
    typed(a, &exact);
    press(a, KeyCode::Enter);
}

/// Press the digit that jumps to `tab`. Naming the tab rather than hardcoding
/// its digit means inserting a tab cannot silently retarget a test.
fn go(a: &mut App, tab: Tab) {
    press(a, KeyCode::Char(tab.key()));
}

fn typed(a: &mut App, text: &str) {
    for c in text.chars() {
        press(a, KeyCode::Char(c));
    }
}

#[test]
fn digits_jump_straight_to_a_tab() {
    let mut a = app();
    for tab in Tab::ALL {
        press(&mut a, KeyCode::Char(tab.key()));
        assert_eq!(a.tab, tab, "digit {} did not reach {}", tab.key(), tab.label());
    }
    // One past the last tab must be inert — not a panic, not a wrap-around.
    let last = *Tab::ALL.last().unwrap();
    let past = char::from_digit(Tab::ALL.len() as u32 + 1, 10).unwrap();
    press(&mut a, KeyCode::Char(past));
    assert_eq!(a.tab, last);
    press(&mut a, KeyCode::Char('0'));
    assert_eq!(a.tab, last);
}

#[test]
fn tab_cycles_both_ways_and_wraps() {
    let mut a = app();
    press(&mut a, KeyCode::BackTab);
    assert_eq!(a.tab, Tab::Notes, "shift-tab from the first tab wraps to the last");
    press(&mut a, KeyCode::Tab);
    assert_eq!(a.tab, Tab::Vitals);
}

#[test]
fn selection_is_remembered_per_tab() {
    let mut a = app();
    go(&mut a, Tab::Feats); // a long list
    for _ in 0..5 {
        press(&mut a, KeyCode::Char('j'));
    }
    let feats_at = a.selected();
    assert_eq!(feats_at, 5);

    go(&mut a, Tab::Gear);
    assert_eq!(a.selected(), 0, "a fresh tab starts at the top");

    go(&mut a, Tab::Feats); // back again
    assert_eq!(a.selected(), feats_at, "returning to a tab should not lose your place");
}

#[test]
fn selection_cannot_leave_the_list() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    let n = a.rows().len();
    assert!(n > 0);
    for _ in 0..(n + 50) {
        press(&mut a, KeyCode::Char('j'));
    }
    assert_eq!(a.selected(), n - 1, "ran off the bottom");
    for _ in 0..(n + 50) {
        press(&mut a, KeyCode::Char('k'));
    }
    assert_eq!(a.selected(), 0, "ran off the top");
}

#[test]
fn g_and_shift_g_jump_to_the_ends() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('G'));
    assert_eq!(a.selected(), a.rows().len() - 1);
    press(&mut a, KeyCode::Char('g'));
    assert_eq!(a.selected(), 0);
}

#[test]
fn movement_keys_are_inert_on_fixed_panes() {
    // VITALS and SKILLS are not lists; j/k must not appear to do something.
    let mut a = app();
    for tab in [Tab::Vitals, Tab::Skills] {
        press(&mut a, KeyCode::Char(tab.key()));
        assert!(!a.is_list_tab());
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.selected(), 0);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.mode, Mode::List, "there is nothing to open on {}", tab.label());
        press(&mut a, KeyCode::Char('/'));
        assert_eq!(a.mode, Mode::List, "filtering a fixed pane is meaningless");
    }
}

#[test]
fn filter_narrows_the_list_and_is_case_insensitive() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    let all = a.rows().len();

    press(&mut a, KeyCode::Char('/'));
    assert_eq!(a.mode, Mode::Filter);
    typed(&mut a, "SNEAK");

    let hits = a.rows().len();
    assert!(hits > 0, "expected a Sneak Attack row");
    assert!(hits < all, "filter did not narrow anything");
    assert!(a.rows().iter().all(|r| {
        let hay = format!("{} {} {}", r.name, r.meta, r.snippet).to_lowercase();
        hay.contains("sneak")
    }));
}

#[test]
fn printable_keys_are_text_while_filtering_not_commands() {
    // Typing "javelin" must not jump tabs on the "1"-adjacent keys, and "q"
    // must not quit mid-word.
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "q1g");
    assert_eq!(a.filter, "q1g");
    assert_eq!(a.tab, Tab::Feats, "a digit changed tabs while filtering");
    assert!(!a.quit, "q quit while filtering");
}

#[test]
fn backspace_edits_the_filter() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Backspace);
    assert_eq!(a.filter, "snea");
}

#[test]
fn changing_tab_clears_the_filter() {
    // Carrying "fire" from SPELLS into GEAR would silently hide most of a kit.
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Enter);
    go(&mut a, Tab::Gear);
    assert!(a.filter.is_empty(), "filter leaked across tabs");
}

#[test]
fn filtering_clamps_a_selection_that_falls_off_the_end() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('G')); // last row
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak"); // far fewer rows now
    assert!(a.selected() < a.rows().len(), "selection left the filtered list");
    assert!(a.selected_row().is_some());
}

#[test]
fn escape_backs_out_one_layer_at_a_time() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Enter); // leave filter mode, keep the filter
    assert_eq!(a.mode, Mode::List);
    assert!(!a.filter.is_empty());

    press(&mut a, KeyCode::Enter); // open detail
    assert_eq!(a.mode, Mode::Detail);

    press(&mut a, KeyCode::Esc); // detail -> list
    assert_eq!(a.mode, Mode::List);
    assert!(!a.quit);

    press(&mut a, KeyCode::Esc); // clears the filter
    assert!(a.filter.is_empty());
    assert!(!a.quit, "escape quit while a filter was still active");

    press(&mut a, KeyCode::Esc); // nothing left to back out of
    assert!(a.quit);
}

#[test]
fn detail_opens_the_selected_row_and_scrolls() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('j'));
    let expected = a.selected_row().unwrap().name;

    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::Detail);
    assert_eq!(a.selected_row().unwrap().name, expected);

    press(&mut a, KeyCode::Char('j'));
    assert_eq!(a.detail_scroll, 1);
    press(&mut a, KeyCode::Char('k'));
    assert_eq!(a.detail_scroll, 0);
    press(&mut a, KeyCode::Char('k'));
    assert_eq!(a.detail_scroll, 0, "scrolled above the top");
}

#[test]
fn n_and_p_walk_the_list_without_closing_the_detail() {
    // Reading down a list of spells one at a time is the common case.
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Enter);
    let first = a.selected_row().unwrap().name;

    press(&mut a, KeyCode::Char('j')); // scroll within the entry
    press(&mut a, KeyCode::Char('n')); // next entry
    assert_eq!(a.mode, Mode::Detail, "n closed the overlay");
    assert_ne!(a.selected_row().unwrap().name, first);
    assert_eq!(a.detail_scroll, 0, "scroll should reset on a new entry");

    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.selected_row().unwrap().name, first);
}

#[test]
fn q_quits_from_the_list_and_ctrl_c_from_anywhere() {
    let mut a = app();
    press(&mut a, KeyCode::Char('q'));
    assert!(a.quit);

    let mut b = app();
    go(&mut b, Tab::Feats);
    press(&mut b, KeyCode::Char('/'));
    typed(&mut b, "sneak");
    assert!(!b.quit);
    keys::handle(&mut b, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(b.quit, "ctrl-c must escape filter mode too");
}

#[test]
fn opening_detail_on_an_empty_filtered_list_is_a_no_op() {
    let mut a = app();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "zzzzzzzz");
    assert_eq!(a.rows().len(), 0);
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::List, "opened a detail view with nothing selected");
}

// -- phase 3: play state ---------------------------------------------------

#[test]
fn damage_and_heal_are_typed_as_numbers_then_applied() {
    let mut a = app();
    // Not max_hp: the session seeds from the snapshot's removedHitPoints, so
    // a fresh app is already wounded by whatever the website last recorded.
    let start = a.current_hp();
    press(&mut a, KeyCode::Char('d'));
    assert!(matches!(a.mode, vellum::app::state::Mode::Number(_)));
    typed(&mut a, "12");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.current_hp(), start - 12);
    assert_eq!(a.mode, Mode::List, "should return to the list");

    press(&mut a, KeyCode::Char('h'));
    typed(&mut a, "5");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.current_hp(), start - 7);
}

#[test]
fn play_keys_work_from_every_tab() {
    // Mid-combat you should not have to navigate somewhere before you can
    // take damage.
    let mut a = app();
    let start = a.current_hp();
    for tab in Tab::ALL {
        press(&mut a, KeyCode::Char(tab.key()));
        press(&mut a, KeyCode::Char('d'));
        typed(&mut a, "1");
        press(&mut a, KeyCode::Enter);
    }
    assert_eq!(a.current_hp(), start - Tab::ALL.len() as i32);
}

#[test]
fn escaping_a_number_entry_applies_nothing() {
    let mut a = app();
    let before = a.current_hp();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "99");
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.current_hp(), before);
    assert_eq!(a.mode, Mode::List);
    assert!(a.number_buffer.is_empty(), "buffer should not survive a cancel");
}

#[test]
fn non_digits_are_ignored_while_typing_a_number() {
    let mut a = app();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "1q2");
    assert_eq!(a.number_buffer, "12");
    assert!(!a.quit, "q quit during numeric entry");
}

#[test]
fn backspace_edits_a_number_and_an_empty_entry_is_a_no_op() {
    let mut a = app();
    let before = a.current_hp();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "15");
    press(&mut a, KeyCode::Backspace);
    assert_eq!(a.number_buffer, "1");
    press(&mut a, KeyCode::Backspace);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.current_hp(), before, "an empty entry must not damage you");
}

#[test]
fn conditions_overlay_toggles_and_wraps() {
    let mut a = app();
    press(&mut a, KeyCode::Char('c'));
    assert_eq!(a.mode, Mode::Conditions);
    press(&mut a, KeyCode::Char(' '));
    assert!(a.session.has_condition("Blinded"));
    press(&mut a, KeyCode::Char(' '));
    assert!(!a.session.has_condition("Blinded"));

    // Up from the first row wraps to the exhaustion counter at the bottom.
    press(&mut a, KeyCode::Char('k'));
    assert!(a.on_exhaustion_row());
    press(&mut a, KeyCode::Char('+'));
    assert_eq!(a.session.exhaustion, 1);
    press(&mut a, KeyCode::Char('-'));
    assert_eq!(a.session.exhaustion, 0);

    press(&mut a, KeyCode::Esc);
    assert_eq!(a.mode, Mode::List);
}

#[test]
fn death_save_keys_are_bound_only_while_dying() {
    let mut a = app();
    // Healthy: `s` and `f` must do nothing at all.
    press(&mut a, KeyCode::Char('s'));
    press(&mut a, KeyCode::Char('f'));
    assert_eq!(a.session.death_successes, 0);
    assert_eq!(a.session.death_failures, 0);

    drop_to_zero(&mut a);
    assert!(a.is_dying());

    press(&mut a, KeyCode::Char('s'));
    assert_eq!(a.session.death_successes, 1);
    press(&mut a, KeyCode::Char('f'));
    assert_eq!(a.session.death_failures, 1);
}

#[test]
fn a_long_rest_from_the_rest_menu_restores_everything() {
    let mut a = app();
    let max = a.max_hp();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "20");
    press(&mut a, KeyCode::Enter);

    press(&mut a, KeyCode::Char('r'));
    assert_eq!(a.mode, Mode::Rest);
    press(&mut a, KeyCode::Char('l'));
    assert_eq!(a.current_hp(), max);
    assert_eq!(a.mode, Mode::List);
}

#[test]
fn the_rest_menu_can_be_dismissed_without_resting() {
    let mut a = app();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "20");
    press(&mut a, KeyCode::Enter);
    let hurt = a.current_hp();

    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.current_hp(), hurt, "escaping the menu healed you");
    assert_eq!(a.mode, Mode::List);
}

#[test]
fn inspiration_toggles() {
    let mut a = app();
    let before = a.session.inspiration;
    press(&mut a, KeyCode::Char('i'));
    assert_ne!(a.session.inspiration, before);
}

#[test]
fn play_keys_are_text_while_filtering() {
    // "hard leather" contains d, h, r, c, i, t — none may fire as commands.
    let mut a = app();
    let start = a.current_hp();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "hard leather");
    assert_eq!(a.filter, "hard leather");
    assert_eq!(a.current_hp(), start, "a filter keystroke damaged the character");
    assert_eq!(a.mode, Mode::Filter);
}

// -- phase 4: rolling ------------------------------------------------------

fn seeded() -> App {
    // Deterministic dice so these assert on outcomes, not on luck.
    let mut a = app();
    a = a.with_seed(0xC0FFEE);
    a
}

#[test]
fn the_roll_tab_lists_everything_you_get_asked_to_roll() {
    let mut a = seeded();
    go(&mut a, Tab::Roll);
    let names: Vec<String> = a.rows().iter().map(|r| r.name.clone()).collect();

    assert!(names.contains(&"Initiative".to_string()));
    assert!(names.contains(&"DEX save".to_string()));
    assert!(names.contains(&"Stealth".to_string()));
    assert!(names.contains(&"Death save".to_string()));
    // 1 initiative + 6 saves + 18 skills + 1 death save
    assert_eq!(a.rows().len(), 26);
    assert!(a.rows().iter().all(|r| r.roll.is_some()), "a row with nothing to roll");
}

#[test]
fn enter_rolls_on_the_roll_tab_and_opens_detail_everywhere_else() {
    let mut a = seeded();
    go(&mut a, Tab::Roll);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::List, "rolling should not open an overlay");
    assert_eq!(a.rolls.len(), 1);

    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::Detail);
    assert_eq!(a.rolls.len(), 1, "opening a detail view rolled dice");
}

#[test]
fn a_skill_roll_uses_the_derived_modifier() {
    let mut a = seeded();
    go(&mut a, Tab::Roll);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "stealth");
    press(&mut a, KeyCode::Enter); // leave filter
    press(&mut a, KeyCode::Enter); // roll

    let r = a.last_roll().expect("a roll");
    assert_eq!(r.label, "Stealth");
    // Stealth is +9 on the fixture; the modifier must match the sheet.
    assert_eq!(r.expr.modifier, 9);
    assert_eq!(r.total, r.kept as i32 + 9);
}

#[test]
fn advantage_and_disadvantage_roll_two_dice() {
    let mut a = seeded();
    go(&mut a, Tab::Roll);
    press(&mut a, KeyCode::Char('a'));
    let adv = a.last_roll().unwrap().clone();
    assert_eq!(adv.dice.len(), 2);
    assert_eq!(adv.kept, *adv.dice.iter().max().unwrap());

    press(&mut a, KeyCode::Char('z'));
    let dis = a.last_roll().unwrap();
    assert_eq!(dis.kept, *dis.dice.iter().min().unwrap());
}

#[test]
fn rolling_does_nothing_on_tabs_that_are_not_rollable() {
    let mut a = seeded();
    for tab in [Tab::Vitals, Tab::Skills, Tab::Gear, Tab::Notes] {
        go(&mut a, tab);
        press(&mut a, KeyCode::Char('a'));
        press(&mut a, KeyCode::Char('z'));
    }
    assert!(a.rolls.is_empty(), "rolled from a tab with nothing to roll");
}

#[test]
fn a_death_save_applies_itself() {
    // Rolling one and then recording it by hand is double-entry that gets
    // skipped mid-fight.
    let mut a = seeded();
    drop_to_zero(&mut a);
    assert!(a.is_dying());

    go(&mut a, Tab::Roll);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "death");
    press(&mut a, KeyCode::Enter);

    let before = (a.session.death_successes, a.session.death_failures);
    press(&mut a, KeyCode::Enter);
    let after = (a.session.death_successes, a.session.death_failures);
    assert_ne!(before, after, "a death save roll changed nothing");

    let r = a.last_roll().unwrap();
    if r.kept >= 10 {
        assert!(after.0 > before.0 || r.is_nat20());
    } else {
        assert!(after.1 > before.1);
    }
}

#[test]
fn free_form_dice_parse_and_roll() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('x'));
    assert_eq!(a.mode, Mode::Dice);
    typed(&mut a, "2d6+3");
    press(&mut a, KeyCode::Enter);

    let r = a.last_roll().expect("a roll");
    assert_eq!(r.expr.count, 2);
    assert_eq!(r.expr.sides, 6);
    assert!((5..=15).contains(&r.total), "2d6+3 out of range: {}", r.total);
    assert_eq!(a.mode, Mode::List);
}

#[test]
fn a_bad_dice_expression_keeps_the_prompt_open_so_you_can_fix_it() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('x'));
    typed(&mut a, "2dd6");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::Dice, "prompt closed on a typo");
    assert!(a.dice_error.is_some());
    assert!(a.rolls.is_empty());

    // Fixable in place rather than retyped: "2dd6" -> "2dd" -> "2d" -> "2d6".
    press(&mut a, KeyCode::Backspace);
    press(&mut a, KeyCode::Backspace);
    assert_eq!(a.dice_buffer, "2d");
    typed(&mut a, "6");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::List);
    assert_eq!(a.rolls.len(), 1);
}

#[test]
fn dice_entry_swallows_command_keys() {
    // "1d20" contains a digit that would jump tabs and a 'd' that would open
    // the damage prompt.
    let mut a = seeded();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('x'));
    typed(&mut a, "1d20");
    assert_eq!(a.dice_buffer, "1d20");
    assert_eq!(a.tab, Tab::Feats);
    assert!(matches!(a.mode, Mode::Dice));
}

#[test]
fn the_roll_log_keeps_newest_first_and_is_capped() {
    let mut a = seeded();
    go(&mut a, Tab::Roll);
    for _ in 0..(vellum::app::state::ROLL_LOG_CAP + 15) {
        press(&mut a, KeyCode::Enter);
    }
    assert_eq!(a.rolls.len(), vellum::app::state::ROLL_LOG_CAP, "log grew without bound");

    press(&mut a, KeyCode::Char('x'));
    typed(&mut a, "1d4");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.last_roll().unwrap().expr.sides, 4, "newest roll is not first");
}

#[test]
fn the_roll_log_opens_and_closes() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('l'));
    assert_eq!(a.mode, Mode::RollLog);
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.mode, Mode::List);
}

#[test]
fn dice_keys_are_text_while_filtering() {
    let mut a = seeded();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "axe lantern");
    assert_eq!(a.filter, "axe lantern");
    assert!(a.rolls.is_empty(), "a filter keystroke rolled dice");
    assert_eq!(a.mode, Mode::Filter);
}

// -- limited uses ----------------------------------------------------------

#[test]
fn u_spends_a_use_and_shift_u_hands_it_back() {
    let mut a = seeded();
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "uncanny");
    press(&mut a, KeyCode::Enter);

    let row = a.selected_row().unwrap();
    assert_eq!(a.remaining_uses(&row), Some((2, 2)));

    press(&mut a, KeyCode::Char('u'));
    assert_eq!(a.remaining_uses(&a.selected_row().unwrap()), Some((1, 2)));
    press(&mut a, KeyCode::Char('u'));
    assert_eq!(a.remaining_uses(&a.selected_row().unwrap()), Some((0, 2)));

    // Spending past empty is a no-op, not an underflow.
    press(&mut a, KeyCode::Char('u'));
    assert_eq!(a.remaining_uses(&a.selected_row().unwrap()), Some((0, 2)));

    press(&mut a, KeyCode::Char('U'));
    assert_eq!(a.remaining_uses(&a.selected_row().unwrap()), Some((1, 2)));
}

#[test]
fn spending_a_use_on_a_row_that_has_none_does_nothing() {
    let mut a = seeded();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('u'));
    press(&mut a, KeyCode::Char('U'));
    assert!(a.session.uses.is_empty());
}

#[test]
fn a_short_rest_restores_short_rest_uses_only() {
    let mut a = seeded();
    // Uncanny Dodge: short rest. Faerie Fire: long rest.
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "uncanny");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('u'));

    go(&mut a, Tab::Spells);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "faerie");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('u'));

    assert_eq!(a.session.expended_count(), 2);

    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('s')); // short rest
    assert_eq!(a.mode, Mode::ShortRest, "a short rest opens the hit dice screen");
    assert_eq!(a.session.uses_of("action:501"), 0, "short-rest use not restored");
    assert_eq!(a.session.uses_of("spell:601"), 1, "long-rest use wrongly restored");
    press(&mut a, KeyCode::Esc); // leave the rest screen

    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('l')); // long rest
    assert_eq!(a.session.expended_count(), 0);
}

#[test]
fn uses_survive_switching_tabs_and_filtering() {
    let mut a = seeded();
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "uncanny");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('u'));

    go(&mut a, Tab::Notes);
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "uncanny");
    press(&mut a, KeyCode::Enter);
    assert_eq!(
        a.remaining_uses(&a.selected_row().unwrap()),
        Some((1, 2)),
        "the spend did not stick"
    );
}

// -- ability corrections ---------------------------------------------------

#[test]
fn adjusting_a_score_recomputes_the_whole_sheet_live() {
    let mut a = seeded();
    let ac_before = a.sheet.armor_class.value;

    press(&mut a, KeyCode::Char('o'));
    assert_eq!(a.mode, Mode::Abilities);
    press(&mut a, KeyCode::Char('j')); // STR -> DEX
    press(&mut a, KeyCode::Char('+'));

    assert_eq!(a.sheet.score(vellum::derive::tables::Ability::Dex), 18);
    assert_eq!(a.sheet.armor_class.value, ac_before + 1, "AC did not follow");
    assert!(a.ability_is_overridden(vellum::derive::tables::Ability::Dex));
}

#[test]
fn the_first_nudge_starts_from_the_computed_score_not_from_zero() {
    let mut a = seeded();
    let dex = a.sheet.score(vellum::derive::tables::Ability::Dex);
    press(&mut a, KeyCode::Char('o'));
    press(&mut a, KeyCode::Char('j'));
    press(&mut a, KeyCode::Char('-'));
    assert_eq!(a.sheet.score(vellum::derive::tables::Ability::Dex), dex - 1);
}

#[test]
fn clearing_an_override_returns_to_the_computed_score() {
    let mut a = seeded();
    let dex = a.sheet.score(vellum::derive::tables::Ability::Dex);
    press(&mut a, KeyCode::Char('o'));
    press(&mut a, KeyCode::Char('j'));
    for _ in 0..3 {
        press(&mut a, KeyCode::Char('+'));
    }
    assert_eq!(a.sheet.score(vellum::derive::tables::Ability::Dex), dex + 3);

    press(&mut a, KeyCode::Char('0'));
    assert_eq!(a.sheet.score(vellum::derive::tables::Ability::Dex), dex);
    assert!(!a.ability_is_overridden(vellum::derive::tables::Ability::Dex));
}

#[test]
fn the_ability_cursor_wraps() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('o'));
    press(&mut a, KeyCode::Char('k'));
    assert_eq!(a.ability_cursor, 5, "up from the first should wrap to the last");
    press(&mut a, KeyCode::Char('j'));
    assert_eq!(a.ability_cursor, 0);
}

// -- hit dice and rests ----------------------------------------------------

#[test]
fn a_short_rest_opens_a_screen_for_spending_dice_one_at_a_time() {
    // "You can decide to spend an additional Hit Point Die after each roll",
    // so this is a screen you sit in, not a single keystroke.
    let mut a = seeded();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "30");
    press(&mut a, KeyCode::Enter);
    let hurt = a.current_hp();

    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('s'));
    assert_eq!(a.mode, Mode::ShortRest);
    assert_eq!(a.hit_dice(), vec![("d8".to_string(), 8, 8)]);

    press(&mut a, KeyCode::Char(' '));
    assert_eq!(a.hit_dice()[0].1, 7, "a die should have been spent");
    assert!(a.current_hp() > hurt, "spending a die should heal you");
    assert!(a.rest_healed > 0);

    press(&mut a, KeyCode::Esc);
    assert_eq!(a.mode, Mode::List);
    assert_eq!(a.rest_healed, 0, "the running total resets when the rest ends");
}

#[test]
fn a_hit_die_roll_lands_in_the_roll_log() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "30");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('s'));
    press(&mut a, KeyCode::Char(' '));

    let r = a.last_roll().expect("a roll");
    assert_eq!(r.label, "Hit die");
    assert_eq!(r.expr.sides, 8);
    assert_eq!(r.expr.modifier, a.sheet.modifier(vellum::derive::tables::Ability::Con));
}

#[test]
fn an_empty_hit_dice_pool_refuses_rather_than_healing_for_free() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "40");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('s'));
    for _ in 0..8 {
        press(&mut a, KeyCode::Char(' '));
    }
    assert_eq!(a.hit_dice()[0].1, 0);
    let hp = a.current_hp();
    let rolls = a.rolls.len();
    press(&mut a, KeyCode::Char(' '));
    assert_eq!(a.current_hp(), hp, "healed with no dice left");
    assert_eq!(a.rolls.len(), rolls, "rolled a die that was not there");
}

#[test]
fn a_long_rest_gives_every_hit_die_back() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "30");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('s'));
    press(&mut a, KeyCode::Char(' '));
    press(&mut a, KeyCode::Char(' '));
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.hit_dice()[0].1, 6);

    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('l'));
    assert_eq!(a.hit_dice()[0].1, 8, "2024 long rests restore the whole pool");
    assert_eq!(a.current_hp(), a.max_hp());
}

#[test]
fn you_cannot_rest_at_zero_hit_points() {
    // "To start a Short Rest, you must have at least 1 Hit Point."
    let mut a = seeded();
    drop_to_zero(&mut a);
    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('l'));
    assert_eq!(a.current_hp(), 0, "a long rest at 0 hp should be refused");
    assert!(a.notice.is_some(), "the refusal should be explained");
    assert_eq!(a.mode, Mode::List);
}

// -- attacks ---------------------------------------------------------------

#[test]
fn equipped_weapons_become_rollable_attacks_on_the_actions_tab() {
    let mut a = seeded();
    go(&mut a, Tab::Actions);
    let rows = a.rows();
    let leather_is_not_an_attack = !rows.iter().any(|r| r.name == "Leather");
    assert!(leather_is_not_an_attack, "armour is not a weapon");

    let dagger = rows.iter().find(|r| r.name == "Dagger").expect("an equipped dagger");
    assert!(dagger.roll.is_some(), "an attack should roll");
    assert!(dagger.damage.is_some(), "an attack should have damage");
    assert!(dagger.meta.contains("to hit"));
}

#[test]
fn enter_rolls_an_attack_and_shift_d_rolls_its_damage() {
    let mut a = seeded();
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "dagger");
    press(&mut a, KeyCode::Enter);

    press(&mut a, KeyCode::Enter); // attack
    let hit = a.last_roll().expect("an attack roll").clone();
    assert_eq!(hit.expr.sides, 20, "an attack roll is a d20");
    assert_eq!(hit.label, "Dagger");
    assert_eq!(a.mode, Mode::List, "rolling should not open the detail view");

    press(&mut a, KeyCode::Char('D')); // damage
    let dmg = a.last_roll().expect("a damage roll");
    assert_eq!(dmg.expr.sides, 4, "a dagger deals d4");
    assert!(dmg.label.to_lowercase().contains("piercing"));
}

#[test]
fn a_row_with_no_attack_roll_opens_its_detail_instead() {
    // Sneak Attack adds damage to someone else's roll; it has none of its own.
    let mut a = seeded();
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak attack");
    press(&mut a, KeyCode::Enter);
    let row = a.selected_row().unwrap();
    assert!(row.roll.is_none(), "Sneak Attack should have no to-hit");
    assert!(row.damage.is_some(), "but it should still roll damage");

    press(&mut a, KeyCode::Enter);
    assert_eq!(a.mode, Mode::Detail, "enter should open, not roll");
}

#[test]
fn an_attack_roll_goes_through_the_rules_engine() {
    // Poisoned gives Disadvantage on attack rolls.
    let mut a = seeded();
    a.session.add_condition("Poisoned");
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "dagger");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Enter);

    let r = a.last_roll().unwrap();
    assert_eq!(r.dice.len(), 2, "disadvantage rolls two dice");
    assert_eq!(r.kept, *r.dice.iter().min().unwrap());
}

#[test]
fn damage_is_not_touched_by_conditions_or_exhaustion() {
    // Those modify d20 tests. A damage roll is not one.
    let mut a = seeded();
    a.session.add_condition("Poisoned");
    a.session.adjust_exhaustion(3);
    go(&mut a, Tab::Actions);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "dagger");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('D'));

    let r = a.last_roll().unwrap();
    assert_eq!(r.dice.len(), 1, "damage should be a single roll");
    assert_eq!(r.advantage, vellum::dice::Advantage::Normal);
}

// -- loading a character ---------------------------------------------------

#[test]
fn the_load_screen_accepts_every_url_shape_and_rejects_junk() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('L'));
    assert_eq!(a.mode, Mode::Load);

    typed(&mut a, "https://www.dndbeyond.com/characters/123456789/AbCdEf");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.pending_load, Some(123456789), "should hand the id to the loop");
    a.pending_load = None;

    press(&mut a, KeyCode::Esc);
    press(&mut a, KeyCode::Char('L'));
    typed(&mut a, "not a url");
    press(&mut a, KeyCode::Enter);
    assert!(a.pending_load.is_none(), "junk must not reach the network");
    assert!(matches!(a.load_status, vellum::app::state::LoadStatus::Failed(_)));
}

#[test]
fn typing_a_url_does_not_fire_commands() {
    // A D&D Beyond URL contains d, h, c, r, s, digits — all of them bound.
    let mut a = seeded();
    let hp = a.current_hp();
    press(&mut a, KeyCode::Char('L'));
    typed(&mut a, "https://www.dndbeyond.com/characters/123456789");
    assert_eq!(a.load_buffer, "https://www.dndbeyond.com/characters/123456789");
    assert_eq!(a.current_hp(), hp, "a keystroke damaged the character");
    assert_eq!(a.tab, Tab::Vitals, "a digit changed tabs");
    assert!(!a.quit);
}

#[test]
fn adopting_a_character_replaces_everything() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "10");
    press(&mut a, KeyCode::Enter);
    go(&mut a, Tab::Gear);
    assert!(!a.rolls.is_empty() || a.session.damage > 0);

    let ch: Character =
        serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let fresh_session = Session::seed(ch.id, 0, 0, false);
    a.adopt(ch, fresh_session, std::env::temp_dir().join("adopted.json"), None);

    assert_eq!(a.tab, Tab::Vitals, "should land on vitals");
    assert_eq!(a.session.damage, 0, "the new character's own state");
    assert!(a.rolls.is_empty(), "the previous character's rolls should not carry over");
    assert_eq!(a.mode, Mode::List);
}

#[test]
fn escape_will_not_strand_you_on_an_empty_first_run() {
    let ch = Character::default();
    let sheet = vellum::derive::derive(&ch);
    let mut a = App::new(
        sheet,
        ch,
        Session::seed(0, 0, 0, false),
        std::env::temp_dir().join("empty.json"),
        None,
    );
    a.open_load();
    assert!(a.is_empty());
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.mode, Mode::Load, "there is nothing to escape to yet");
    assert!(!a.quit);
}

// -- undo ------------------------------------------------------------------

fn undo(a: &mut App) {
    keys::handle(a, KeyCode::Char('z'), KeyModifiers::CONTROL);
}

#[test]
fn undo_reverses_the_last_change_and_says_what_it_was() {
    let mut a = seeded();
    let hp = a.current_hp();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "30");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.next_undo(), Some("hit points"));

    undo(&mut a);
    assert_eq!(a.current_hp(), hp);
    assert_eq!(a.undo_note, Some("hit points"));
    assert!(!a.can_undo());
}

#[test]
fn undo_goes_back_more_than_one_step() {
    let mut a = seeded();
    let hp = a.current_hp();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "10");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('c'));
    press(&mut a, KeyCode::Char(' ')); // Blinded
    press(&mut a, KeyCode::Esc);
    press(&mut a, KeyCode::Char('i')); // inspiration

    assert_eq!(a.next_undo(), Some("inspiration"));
    undo(&mut a);
    assert_eq!(a.next_undo(), Some("conditions"));
    undo(&mut a);
    assert!(a.session.conditions.is_empty());
    undo(&mut a);
    assert_eq!(a.current_hp(), hp);
    assert!(!a.can_undo());
}

#[test]
fn only_changes_are_recorded() {
    // Moving a cursor or switching tabs is not something to undo.
    let mut a = seeded();
    go(&mut a, Tab::Feats);
    for _ in 0..5 {
        press(&mut a, KeyCode::Char('j'));
    }
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Enter);
    go(&mut a, Tab::Gear);
    assert!(!a.can_undo(), "navigation should not fill the undo stack");
}

#[test]
fn undo_does_not_record_itself() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('i'));
    assert!(a.can_undo());
    undo(&mut a);
    assert!(!a.can_undo(), "the undo became something to undo");
}

#[test]
fn undoing_with_nothing_recorded_is_a_no_op() {
    let mut a = seeded();
    let hp = a.current_hp();
    undo(&mut a);
    undo(&mut a);
    assert_eq!(a.current_hp(), hp);
    assert!(a.undo_note.is_none());
    assert!(!a.quit);
}

#[test]
fn undoing_an_ability_correction_rebuilds_the_whole_sheet() {
    // An ability score feeds armour class, saves, skills and attacks, so
    // restoring the session is not enough on its own.
    let mut a = seeded();
    let ac = a.sheet.armor_class.value;
    press(&mut a, KeyCode::Char('o'));
    press(&mut a, KeyCode::Char('j')); // DEX
    press(&mut a, KeyCode::Char('+'));
    assert_eq!(a.sheet.armor_class.value, ac + 1);

    undo(&mut a);
    assert_eq!(a.sheet.armor_class.value, ac, "the sheet did not follow the undo");
    assert_eq!(a.undo_note, Some("an ability score"));
}

#[test]
fn undo_leaves_the_roll_log_alone() {
    // Undoing the heal from a spent hit die must not pretend the die was
    // never rolled. It really came up what it came up.
    let mut a = seeded();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "20");
    press(&mut a, KeyCode::Enter);
    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('s'));
    press(&mut a, KeyCode::Char(' ')); // spend a hit die
    let rolls = a.rolls.len();
    let dice_left = a.hit_dice()[0].1;

    undo(&mut a);
    assert_eq!(a.rolls.len(), rolls, "the roll log should survive an undo");
    assert_eq!(a.hit_dice()[0].1, dice_left + 1, "the die should come back");
}

#[test]
fn the_undo_stack_is_bounded() {
    let mut a = seeded();
    for _ in 0..(vellum::app::state::UNDO_DEPTH + 20) {
        press(&mut a, KeyCode::Char('i')); // toggle inspiration
    }
    // Unwinding everything must terminate and leave the app usable.
    for _ in 0..(vellum::app::state::UNDO_DEPTH + 40) {
        undo(&mut a);
    }
    assert!(!a.can_undo());
    assert!(!a.quit);
}

#[test]
fn undo_does_not_reach_across_characters() {
    // Restoring one character's hit points onto another is worse than not
    // undoing at all.
    let mut a = seeded();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "10");
    press(&mut a, KeyCode::Enter);
    assert!(a.can_undo());

    let ch: Character = serde_json::from_str(include_str!("fixtures/srd_rogue.json")).unwrap();
    let s = Session::seed(ch.id, 0, 0, false);
    a.adopt(ch, s, std::env::temp_dir().join("adopted2.json"), None);
    assert!(!a.can_undo(), "the undo stack survived a character swap");
}

#[test]
fn a_new_keystroke_clears_the_undo_confirmation() {
    let mut a = seeded();
    press(&mut a, KeyCode::Char('i'));
    undo(&mut a);
    assert!(a.undo_note.is_some());
    press(&mut a, KeyCode::Char('j'));
    assert!(a.undo_note.is_none(), "the note should not linger");
}

// -- spellcasting ----------------------------------------------------------

fn caster() -> App {
    let ch: Character =
        serde_json::from_str(include_str!("fixtures/srd_cleric.json")).expect("cleric fixture");
    let sheet = derive(&ch);
    let path = std::env::temp_dir().join(format!("vellum-caster-{}.json", std::process::id()));
    let session = Session::seed(ch.id, 0, 0, false);
    App::new(sheet, ch, session, path, None).with_seed(0xCA57)
}

fn select_spell(a: &mut App, name: &str) {
    go(a, Tab::Spells);
    press(a, KeyCode::Char('/'));
    typed(a, name);
    press(a, KeyCode::Enter);
}

#[test]
fn casting_spends_a_slot_of_the_spells_level() {
    let mut a = caster();
    assert_eq!(a.slots(), vec![(1, 4, 4), (2, 3, 3), (3, 2, 2)]);

    select_spell(&mut a, "spiritual"); // a level 2 spell
    press(&mut a, KeyCode::Char('C'));
    assert_eq!(
        a.slots(),
        vec![(1, 4, 4), (2, 2, 3), (3, 2, 2)],
        "a level 2 slot should go, and only that"
    );
}

#[test]
fn a_cantrip_costs_nothing() {
    let mut a = caster();
    select_spell(&mut a, "sacred");
    press(&mut a, KeyCode::Char('C'));
    assert_eq!(a.slots(), vec![(1, 4, 4), (2, 3, 3), (3, 2, 2)]);
}

#[test]
fn casting_with_no_slot_left_refuses_and_says_so() {
    let mut a = caster();
    select_spell(&mut a, "spiritual");
    for _ in 0..3 {
        press(&mut a, KeyCode::Char('C'));
    }
    assert_eq!(a.slots()[1], (2, 0, 3), "all three level 2 slots spent");

    press(&mut a, KeyCode::Char('C'));
    assert_eq!(a.slots()[1], (2, 0, 3), "must not go negative");
    assert!(a.notice.is_some(), "the refusal should be explained");
}

#[test]
fn casting_a_concentration_spell_takes_up_concentration() {
    let mut a = caster();
    select_spell(&mut a, "bless");
    press(&mut a, KeyCode::Char('C'));
    assert_eq!(a.session.concentrating_on.as_deref(), Some("Bless"));

    press(&mut a, KeyCode::Char('X'));
    assert!(a.session.concentrating_on.is_none());
}

#[test]
fn a_second_concentration_spell_ends_the_first_and_says_which() {
    // "You lose Concentration on an effect the moment you start casting a
    // spell that requires Concentration."
    let mut a = caster();
    select_spell(&mut a, "bless");
    press(&mut a, KeyCode::Char('C'));

    a.session.concentrate_on("Hold Person");
    assert_eq!(a.session.concentrating_on.as_deref(), Some("Hold Person"));

    select_spell(&mut a, "bless");
    press(&mut a, KeyCode::Char('C'));
    assert_eq!(a.session.concentrating_on.as_deref(), Some("Bless"));
    assert!(
        a.notice.as_deref().is_some_and(|n| n.contains("Hold Person")),
        "the player should be told what was dropped: {:?}",
        a.notice
    );
}

#[test]
fn taking_damage_while_concentrating_works_out_the_save() {
    // The app knows the damage because it just applied it.
    let mut a = caster();
    select_spell(&mut a, "bless");
    press(&mut a, KeyCode::Char('C'));

    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "9");
    press(&mut a, KeyCode::Enter);
    let (spell, dc) = a.pending_concentration().expect("a save should be owed");
    assert_eq!(spell, "Bless");
    assert_eq!(dc, 10, "half of 9 rounds down to 4, so the floor of 10 applies");

    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "30");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.pending_concentration().unwrap().1, 15, "half of 30");
}

#[test]
fn no_save_is_owed_when_not_concentrating() {
    let mut a = caster();
    press(&mut a, KeyCode::Char('d'));
    typed(&mut a, "30");
    press(&mut a, KeyCode::Enter);
    assert!(a.pending_concentration().is_none());
}

#[test]
fn a_long_rest_restores_slots_and_a_short_rest_does_not() {
    let mut a = caster();
    select_spell(&mut a, "spiritual");
    press(&mut a, KeyCode::Char('C'));
    press(&mut a, KeyCode::Char('C'));
    assert_eq!(a.slots()[1], (2, 1, 3));

    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('s')); // short rest
    press(&mut a, KeyCode::Esc);
    assert_eq!(a.slots()[1], (2, 1, 3), "ordinary slots survive a short rest");

    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('l')); // long rest
    assert_eq!(a.slots()[1], (2, 3, 3), "and come back on a long one");
}

#[test]
fn a_long_rest_ends_concentration() {
    // You sleep through it, which means the Unconscious condition.
    let mut a = caster();
    select_spell(&mut a, "bless");
    press(&mut a, KeyCode::Char('C'));
    press(&mut a, KeyCode::Char('r'));
    press(&mut a, KeyCode::Char('l'));
    assert!(a.session.concentrating_on.is_none());
}

#[test]
fn casting_can_be_undone() {
    let mut a = caster();
    select_spell(&mut a, "bless");
    press(&mut a, KeyCode::Char('C'));
    assert_eq!(a.slots()[0], (1, 3, 4));
    assert!(a.session.concentrating_on.is_some());

    undo(&mut a);
    assert_eq!(a.slots()[0], (1, 4, 4), "the slot should come back");
    assert!(a.session.concentrating_on.is_none());
}

#[test]
fn a_character_with_no_slots_shows_none() {
    let a = seeded();
    assert!(a.slots().is_empty());
    assert!(a.pact().is_none());
}

#[test]
fn pressing_slash_again_starts_a_fresh_search() {
    // It used to keep the old text, so typing appended and matched nothing.
    let mut a = seeded();
    go(&mut a, Tab::Feats);
    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "sneak");
    press(&mut a, KeyCode::Enter);
    assert_eq!(a.filter, "sneak");

    press(&mut a, KeyCode::Char('/'));
    typed(&mut a, "alert");
    assert_eq!(a.filter, "alert", "the previous search should not linger");
    assert!(!a.rows().is_empty(), "a doubled filter would match nothing");
}
