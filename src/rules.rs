//! The 2024 rules that turn a character sheet into rolls.
//!
//! Everything here is game mechanics from SRD 5.2.1 (CC-BY-4.0, Wizards of the
//! Coast), verified against the official PDF rather than a secondary source.
//! It is deliberately separate from `derive`: `derive` answers "what are this
//! character's numbers", this answers "what happens when they roll".
//!
//! The point of putting it in one place is that the sheet and the dice can
//! never disagree. If you are Poisoned, the roller knows without being told.

use crate::derive::tables::Ability;
use crate::dice::Advantage;

// ---------------------------------------------------------------------------
// Exhaustion
// ---------------------------------------------------------------------------

/// 2024 exhaustion is one rule rather than a table: every level is -2 on every
/// d20 test and -5 feet of Speed, and level 6 is death.
pub const EXHAUSTION_D20_PENALTY_PER_LEVEL: i32 = 2;
pub const EXHAUSTION_SPEED_PENALTY_PER_LEVEL: i32 = 5;

pub fn exhaustion_penalty(level: u8) -> i32 {
    -(level as i32) * EXHAUSTION_D20_PENALTY_PER_LEVEL
}

pub fn exhaustion_speed_penalty(level: u8) -> i32 {
    (level as i32) * EXHAUSTION_SPEED_PENALTY_PER_LEVEL
}

// ---------------------------------------------------------------------------
// Conditions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Neutral,
    Advantage,
    Disadvantage,
}

/// The mechanical effects of a condition, restricted to what a character sheet
/// can actually act on.
///
/// Several effects are deliberately *not* modelled as numbers because they
/// depend on the fiction — whether you can see the source of your fear, whether
/// the attacker is within five feet, whether a check relies on sight. Those
/// surface as `note` so the player makes the ruling instead of the app
/// pretending to.
#[derive(Debug, Clone, Copy)]
pub struct Effects {
    pub attack: Disposition,
    pub ability_check: Disposition,
    /// Some conditions single out the Initiative roll specifically, separately
    /// from ability checks in general — Incapacitated and Invisible both do,
    /// under a heading the SRD calls "Surprise". Missed entirely when working
    /// from condition summaries rather than the source.
    pub initiative: Disposition,
    /// Saves that take Disadvantage.
    pub save_disadvantage: &'static [Ability],
    /// Saves that fail automatically.
    pub save_auto_fail: &'static [Ability],
    pub speed_zero: bool,
    pub incapacitated: bool,
    /// What the app cannot decide for you.
    pub note: &'static str,
}

const NONE: Effects = Effects {
    attack: Disposition::Neutral,
    ability_check: Disposition::Neutral,
    initiative: Disposition::Neutral,
    save_disadvantage: &[],
    save_auto_fail: &[],
    speed_zero: false,
    incapacitated: false,
    note: "",
};

const STR_DEX: &[Ability] = &[Ability::Str, Ability::Dex];

/// Effects by condition name, matching `session::CONDITIONS`.
pub fn effects(condition: &str) -> Effects {
    match condition {
        "Blinded" => Effects {
            attack: Disposition::Disadvantage,
            note: "auto-fails checks that need sight; attacks against you have advantage",
            ..NONE
        },
        "Charmed" => Effects {
            note: "cannot attack or harm the charmer; they have advantage on social checks with you",
            ..NONE
        },
        "Deafened" => Effects {
            note: "auto-fails checks that need hearing",
            ..NONE
        },
        "Frightened" => Effects {
            // Only while the source is visible — the app cannot know that, so
            // it applies the common case and says so.
            attack: Disposition::Disadvantage,
            ability_check: Disposition::Disadvantage,
            note: "only while you can see the source; you cannot willingly move closer",
            ..NONE
        },
        "Grappled" => Effects {
            attack: Disposition::Disadvantage,
            speed_zero: true,
            note: "disadvantage only against targets other than the grappler",
            ..NONE
        },
        "Incapacitated" => Effects {
            // "Surprised. If you're Incapacitated when you roll Initiative,
            // you have Disadvantage on the roll."
            initiative: Disposition::Disadvantage,
            incapacitated: true,
            note: "no actions, bonus actions or reactions; concentration breaks; you can't speak",
            ..NONE
        },
        "Invisible" => Effects {
            attack: Disposition::Advantage,
            // "Surprise. If you're Invisible when you roll Initiative, you
            // have Advantage on the roll."
            initiative: Disposition::Advantage,
            note: "attacks against you have disadvantage; you are concealed",
            ..NONE
        },
        "Paralyzed" => Effects {
            save_auto_fail: STR_DEX,
            speed_zero: true,
            incapacitated: true,
            note: "melee hits from within 5 feet are critical",
            ..NONE
        },
        "Petrified" => Effects {
            save_auto_fail: STR_DEX,
            speed_zero: true,
            incapacitated: true,
            note: "resistant to all damage; immune to poison and disease",
            ..NONE
        },
        "Poisoned" => Effects {
            attack: Disposition::Disadvantage,
            ability_check: Disposition::Disadvantage,
            ..NONE
        },
        "Prone" => Effects {
            attack: Disposition::Disadvantage,
            note: "melee attacks against you have advantage, ranged have disadvantage",
            ..NONE
        },
        "Restrained" => Effects {
            attack: Disposition::Disadvantage,
            save_disadvantage: &[Ability::Dex],
            speed_zero: true,
            note: "attacks against you have advantage",
            ..NONE
        },
        "Stunned" => Effects {
            save_auto_fail: STR_DEX,
            // No Speed 0 clause: unlike Grappled, Restrained, Paralyzed,
            // Petrified and Unconscious, the SRD entry for Stunned does not
            // zero your Speed. It only makes you Incapacitated.
            incapacitated: true,
            note: "attacks against you have advantage",
            ..NONE
        },
        "Unconscious" => Effects {
            save_auto_fail: STR_DEX,
            speed_zero: true,
            incapacitated: true,
            note: "you drop what you hold and fall prone; melee hits from within 5 feet are critical",
            ..NONE
        },
        _ => NONE,
    }
}

// ---------------------------------------------------------------------------
// Resolving a d20 test
// ---------------------------------------------------------------------------

/// What is being rolled. The distinction matters: in the 2024 rules Initiative
/// is a **Dexterity check**, so anything that hampers ability checks — being
/// Poisoned, being Frightened — hampers Initiative too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    Attack,
    Check { ability: Ability, skill: Option<&'static str> },
    Save { ability: Ability },
    Initiative,
    DeathSave,
}

impl TestKind {
    fn is_ability_check(self) -> bool {
        matches!(self, TestKind::Check { .. } | TestKind::Initiative)
    }
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub advantage: Advantage,
    /// Flat modifier from exhaustion. Negative or zero.
    pub penalty: i32,
    pub auto_fail: bool,
    /// Why, in the order it was decided. Shown to the player — a roll that
    /// silently changes itself is worse than one you have to think about.
    pub sources: Vec<String>,
}

/// Combine everything that bears on one d20 test.
///
/// `granted` is the character's own advantages from the sheet ("initiative",
/// "stealth", "death saving throws"); `requested` is what the player asked for
/// by pressing a key.
pub fn resolve(
    kind: TestKind,
    conditions: &[String],
    exhaustion: u8,
    granted: &[String],
    requested: Advantage,
) -> Resolution {
    let mut adv: Vec<String> = Vec::new();
    let mut dis: Vec<String> = Vec::new();
    let mut sources: Vec<String> = Vec::new();
    let mut auto_fail = false;

    if requested == Advantage::Advantage {
        adv.push("you".into());
    }
    if requested == Advantage::Disadvantage {
        dis.push("you".into());
    }

    // The character's own standing advantages.
    for g in granted {
        if grant_applies(g, kind) {
            adv.push(g.clone());
        }
    }

    for c in conditions {
        let e = effects(c);
        // Initiative is a Dexterity check, so anything hampering ability
        // checks reaches it — and some conditions single it out on top.
        if kind == TestKind::Initiative {
            match e.initiative {
                Disposition::Advantage => adv.push(c.clone()),
                Disposition::Disadvantage => dis.push(c.clone()),
                Disposition::Neutral => {}
            }
        }

        let disposition = match kind {
            TestKind::Attack => e.attack,
            k if k.is_ability_check() => e.ability_check,
            TestKind::Save { ability } => {
                if e.save_auto_fail.contains(&ability) {
                    auto_fail = true;
                    sources.push(format!("{c}: automatic failure"));
                    Disposition::Neutral
                } else if e.save_disadvantage.contains(&ability) {
                    Disposition::Disadvantage
                } else {
                    Disposition::Neutral
                }
            }
            // A death save is not an ability check and not a save against an
            // ability, so conditions do not touch it. Exhaustion still does.
            TestKind::DeathSave => Disposition::Neutral,
            _ => Disposition::Neutral,
        };
        match disposition {
            Disposition::Advantage => adv.push(c.clone()),
            Disposition::Disadvantage => dis.push(c.clone()),
            Disposition::Neutral => {}
        }
    }

    // "If circumstances cause a roll to have both Advantage and Disadvantage,
    // the roll has neither of them, and you roll one d20." Multiple sources on
    // the same side still mean exactly two dice.
    let advantage = match (adv.is_empty(), dis.is_empty()) {
        (false, true) => Advantage::Advantage,
        (true, false) => Advantage::Disadvantage,
        (false, false) => Advantage::Normal,
        (true, true) => Advantage::Normal,
    };

    // Terse on purpose: this has to share one 80-column row with the roll
    // itself. Name the source that changed the outcome, not every source.
    if !adv.is_empty() && !dis.is_empty() {
        sources.push(format!("cancelled by {}", dis.join(", ")));
    } else if !adv.is_empty() {
        sources.push(format!("adv: {}", adv.join(", ")));
    } else if !dis.is_empty() {
        sources.push(format!("dis: {}", dis.join(", ")));
    }

    let penalty = exhaustion_penalty(exhaustion);
    if penalty != 0 {
        sources.push(format!("exhaustion {penalty}"));
    }

    Resolution { advantage, penalty, auto_fail, sources }
}

/// Does a standing advantage from the character sheet apply to this test?
///
/// The strings come from D&D Beyond modifier subtypes with hyphens turned into
/// spaces — "death saving throws", "initiative", "stealth".
fn grant_applies(grant: &str, kind: TestKind) -> bool {
    let g = grant.to_lowercase();
    match kind {
        TestKind::Initiative => g == "initiative",
        TestKind::DeathSave => g == "death saving throws",
        TestKind::Save { ability } => {
            g == "saving throws" || g == format!("{} saving throws", ability.slug())
        }
        TestKind::Check { ability, skill } => {
            skill.map(|s| g == s.replace('-', " ")).unwrap_or(false)
                || g == format!("{} checks", ability.slug())
        }
        TestKind::Attack => g == "attack rolls",
    }
}

// ---------------------------------------------------------------------------
// Derived values the payload does not carry
// ---------------------------------------------------------------------------

/// Unarmored Defense, for the classes that have it. Mutually exclusive with
/// armour and with each other — the character picks one.
pub fn unarmored_defense(class: &str, dex: i32, con: i32, wis: i32) -> Option<(i32, &'static str)> {
    match class.to_lowercase().as_str() {
        "barbarian" => Some((10 + dex + con, "Unarmored Defense 10 + DEX + CON")),
        "monk" => Some((10 + dex + wis, "Unarmored Defense 10 + DEX + WIS")),
        _ => None,
    }
}

pub fn spell_save_dc(proficiency_bonus: i32, ability_modifier: i32) -> i32 {
    8 + proficiency_bonus + ability_modifier
}

pub fn spell_attack_bonus(proficiency_bonus: i32, ability_modifier: i32) -> i32 {
    proficiency_bonus + ability_modifier
}

/// Walking Speed after exhaustion and any condition that pins you in place.
pub fn effective_speed(base: i32, exhaustion: u8, conditions: &[String]) -> i32 {
    if conditions.iter().any(|c| effects(c).speed_zero) {
        return 0;
    }
    (base - exhaustion_speed_penalty(exhaustion)).max(0)
}

/// "When damage reduces a character to 0 Hit Points and damage remains, the
/// character dies if the remainder equals or exceeds their Hit Point maximum."
pub fn is_massive_damage(damage: i32, current_hp: i32, max_hp: i32) -> bool {
    let remainder = damage - current_hp;
    current_hp > 0 && remainder >= max_hp
}

/// "For each Hit Point Die you spend in this way, roll the die and add your
/// Constitution modifier to it. You regain Hit Points equal to the total
/// (minimum of 1 Hit Point)."
///
/// The minimum is the part worth encoding: a d8 rolled as a 1 with a -2
/// Constitution modifier still heals you, rather than healing -1.
pub fn hit_die_healing(roll: u32, con_modifier: i32) -> i32 {
    (roll as i32 + con_modifier).max(1)
}

/// "To start a Short Rest, you must have at least 1 Hit Point." The same
/// condition gates a Long Rest.
pub fn can_rest(current_hp: i32) -> bool {
    current_hp >= 1
}

/// Which ability a weapon attack uses.
///
/// "When making an attack with a Finesse weapon, use your choice of your
/// Strength or Dexterity modifier for the attack and damage rolls." Choice
/// means the better one, every time, so that is what this returns.
pub fn attack_ability(is_ranged: bool, finesse: bool, str_mod: i32, dex_mod: i32) -> Ability {
    if finesse {
        if dex_mod >= str_mod { Ability::Dex } else { Ability::Str }
    } else if is_ranged {
        Ability::Dex
    } else {
        Ability::Str
    }
}

/// Whether the character is proficient with a weapon, from category or name.
///
/// `proficiencies` are the title-cased strings the sheet already derives:
/// "Simple Weapons", "Martial Weapons", "Shortsword".
pub fn weapon_proficient(category_id: i32, weapon_type: &str, proficiencies: &[String]) -> bool {
    let category = match category_id {
        1 => "Simple Weapons",
        2 => "Martial Weapons",
        _ => "",
    };
    proficiencies
        .iter()
        .any(|p| p == category || p.eq_ignore_ascii_case(weapon_type))
}

/// Carrying capacity is Strength score times 15.
pub fn carrying_capacity(strength: i32) -> i32 {
    strength * 15
}

pub fn is_incapacitated(conditions: &[String]) -> bool {
    conditions.iter().any(|c| effects(c).incapacitated)
}

/// "A creature's Passive Perception equals 10 plus the creature's Wisdom
/// (Perception) check bonus. If the creature has Advantage on such checks,
/// increase the score by 5. If the creature has Disadvantage on them, decrease
/// the score by 5."
///
/// Exhaustion is deliberately not applied: it reduces a *roll*, and a passive
/// score is not one.
pub fn passive_perception(
    perception_bonus: i32,
    conditions: &[String],
    granted: &[String],
) -> i32 {
    let kind = TestKind::Check { ability: Ability::Wis, skill: Some("perception") };
    let r = resolve(kind, conditions, 0, granted, Advantage::Normal);
    10 + perception_bonus
        + match r.advantage {
            Advantage::Advantage => 5,
            Advantage::Disadvantage => -5,
            Advantage::Normal => 0,
        }
}
