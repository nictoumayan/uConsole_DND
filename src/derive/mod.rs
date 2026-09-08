//! Phase 1: raw payload -> computed sheet.
//!
//! This module is the product. D&D Beyond ships no derived values at all —
//! no AC, no proficiency bonus, no skill modifiers, not even spell slot
//! maxima. Everything a player reads off a character sheet is computed here by
//! applying the modifier list to the raw scores.
//!
//! Every derived number carries the formula that produced it, so a mismatch
//! against the live sheet tells you *which rule* is wrong rather than just
//! that something is.

pub mod tables;

use crate::ddb::schema::{Character, Modifier};
use tables::{ability_modifier, proficiency_bonus, Ability, SENSE_SUBTYPES, SKILLS};

/// A computed value plus its provenance. `formula` is what you show the player
/// when they ask "where is my +9 coming from?" at the table.
#[derive(Debug, Clone)]
pub struct Derived {
    pub value: i32,
    pub formula: String,
}

impl Derived {
    fn new(value: i32, formula: impl Into<String>) -> Self {
        Self { value, formula: formula.into() }
    }
}

#[derive(Debug, Clone)]
pub struct Score {
    pub ability: Ability,
    pub score: i32,
    pub modifier: i32,
}

#[derive(Debug, Clone)]
pub struct Hp {
    pub max: Derived,
    pub current: i32,
    pub temporary: i32,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub value: i32,
    pub proficient: bool,
    pub expertise: bool,
}

#[derive(Debug, Clone)]
pub struct Conditional {
    pub subject: String,
    pub condition: String,
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub race: String,
    pub classes: String,
    pub total_level: i32,
    pub proficiency_bonus: i32,
    pub scores: Vec<Score>,
    pub hp: Hp,
    pub armor_class: Derived,
    pub initiative: Derived,
    pub passive_perception: i32,
    pub skills: Vec<Entry>,
    pub saves: Vec<Entry>,
    pub senses: Vec<(String, i32)>,
    pub advantages: Vec<String>,
    pub immunities: Vec<String>,
    pub resistances: Vec<String>,
    pub conditionals: Vec<Conditional>,
    pub class_notes: Vec<String>,
    pub inspiration: bool,
}

impl Sheet {
    pub fn score(&self, ability: Ability) -> i32 {
        self.scores
            .iter()
            .find(|s| s.ability == ability)
            .map(|s| s.score)
            .unwrap_or(10)
    }

    pub fn modifier(&self, ability: Ability) -> i32 {
        ability_modifier(self.score(ability))
    }
}

/// Modifiers of a given kind+subType that we are allowed to fold into a number.
fn active<'a>(
    ch: &'a Character,
    kind: &'a str,
    sub_type: &'a str,
) -> impl Iterator<Item = &'a Modifier> {
    ch.modifiers
        .all()
        .filter(move |m| m.kind == kind && m.sub_type == sub_type && m.is_unconditional())
}

fn has(ch: &Character, kind: &str, sub_type: &str) -> bool {
    active(ch, kind, sub_type).next().is_some()
}

fn sum(ch: &Character, kind: &str, sub_type: &str) -> i32 {
    active(ch, kind, sub_type).filter_map(|m| m.value).sum()
}

/// Final ability scores the player has corrected by hand, keyed "STR".."CHA".
pub type AbilityOverrides = std::collections::BTreeMap<String, i32>;

pub fn derive(ch: &Character) -> Sheet {
    derive_with(ch, &AbilityOverrides::new())
}

pub fn derive_with(ch: &Character, overrides: &AbilityOverrides) -> Sheet {
    let total_level: i32 = ch.classes.iter().map(|c| c.level).sum();
    let pb = proficiency_bonus(total_level);
    let scores = derive_scores(ch, overrides);
    let mod_of = |a: Ability| scores[a.index()].modifier;

    let hp = derive_hp(ch, total_level, mod_of(Ability::Con));
    let armor_class = derive_ac(ch, mod_of(Ability::Dex));

    let init_bonus = sum(ch, "bonus", "initiative");
    let initiative = Derived::new(
        mod_of(Ability::Dex) + init_bonus,
        if init_bonus == 0 {
            format!("DEX {:+}", mod_of(Ability::Dex))
        } else {
            format!("DEX {:+} {:+}", mod_of(Ability::Dex), init_bonus)
        },
    );

    let skills = derive_skills(ch, pb, &scores);
    let saves = derive_saves(ch, pb, &scores);

    let passive_perception = 10 + skills
        .iter()
        .find(|s| s.name == "Perception")
        .map(|s| s.value)
        .unwrap_or(mod_of(Ability::Wis));

    let classes = ch
        .classes
        .iter()
        .map(|c| match &c.subclass_definition {
            Some(sub) if !sub.name.is_empty() => {
                format!("{} {} ({})", c.definition.name, c.level, sub.name)
            }
            _ => format!("{} {}", c.definition.name, c.level),
        })
        .collect::<Vec<_>>()
        .join(" / ");

    let class_notes = ch
        .classes
        .iter()
        .flat_map(|c| tables::class_notes(&c.definition.name, c.level))
        .collect();

    Sheet {
        name: ch.name.clone(),
        race: if ch.race.full_name.is_empty() {
            ch.race.base_race_name.clone()
        } else {
            ch.race.full_name.clone()
        },
        classes,
        total_level,
        proficiency_bonus: pb,
        scores: scores.to_vec(),
        hp,
        armor_class,
        initiative,
        passive_perception,
        skills,
        saves,
        senses: derive_senses(ch),
        advantages: collect_kind(ch, "advantage"),
        immunities: collect_kind(ch, "immunity"),
        resistances: collect_kind(ch, "resistance"),
        conditionals: derive_conditionals(ch),
        class_notes,
        inspiration: ch.inspiration,
    }
}

fn derive_scores(ch: &Character, overrides: &AbilityOverrides) -> [Score; 6] {
    std::array::from_fn(|i| {
        let ability = Ability::ALL[i];

        // A hand-entered score wins over everything. It is the last resort for
        // the things D&D Beyond applies but does not ship — see
        // Session::ability_overrides.
        if let Some(v) = overrides.get(ability.abbrev()) {
            return Score { ability, score: *v, modifier: ability_modifier(*v) };
        }

        // An explicit override on D&D Beyond replaces the whole calculation.
        if let Some(Some(v)) = ch.override_stats.get(i).map(|s| s.value) {
            return Score { ability, score: v, modifier: ability_modifier(v) };
        }

        let base = ch.stats.get(i).and_then(|s| s.value).unwrap_or(10);
        let bonus = ch.bonus_stats.get(i).and_then(|s| s.value).unwrap_or(0);
        let sub = format!("{}-score", ability.slug());

        // The scores in `stats` are NOT what D&D Beyond displays. Feats and
        // racial bonuses arrive as separate modifiers and must be added here —
        // this is the single most load-bearing line in the file.
        let from_modifiers = sum(ch, "bonus", &sub);

        let mut score = base + bonus + from_modifiers;

        // "set" (e.g. a Belt of Giant Strength) floors the score at a value
        // rather than adding to it, and never lowers it.
        for m in active(ch, "set", &sub) {
            if let Some(v) = m.value {
                score = score.max(v);
            }
        }

        Score { ability, score, modifier: ability_modifier(score) }
    })
}

fn derive_hp(ch: &Character, total_level: i32, con_mod: i32) -> Hp {
    let (max, formula) = match ch.override_hit_points {
        Some(v) => (v, "override on D&D Beyond".to_string()),
        None => {
            let bonus = ch.bonus_hit_points.unwrap_or(0);
            let from_con = con_mod * total_level;
            let max = ch.base_hit_points + from_con + bonus;
            let mut f = format!(
                "base {} + CON {:+} x {} levels",
                ch.base_hit_points, con_mod, total_level
            );
            if bonus != 0 {
                f.push_str(&format!(" + bonus {bonus}"));
            }
            (max, f)
        }
    };
    Hp {
        max: Derived::new(max, formula),
        current: max - ch.removed_hit_points,
        temporary: ch.temporary_hit_points,
    }
}

fn derive_ac(ch: &Character, dex_mod: i32) -> Derived {
    let equipped: Vec<_> = ch.inventory.iter().filter(|i| i.equipped).collect();

    // `armorClass` is present on shields too, so the item's `type` is what
    // distinguishes body armour from a shield.
    let body = equipped
        .iter()
        .find(|i| i.definition.kind.ends_with("Armor") && i.definition.armor_class.is_some());

    let (mut ac, mut formula) = match body {
        Some(item) => {
            let base = item.definition.armor_class.unwrap_or(10);
            let kind = item.definition.kind.as_str();
            // Light armour takes the full DEX modifier, medium caps it at +2,
            // heavy ignores it entirely.
            let dex_part = if kind.starts_with("Light") {
                dex_mod
            } else if kind.starts_with("Medium") {
                dex_mod.min(2)
            } else {
                0
            };
            let mut f = format!("{} {}", item.definition.name, base);
            if dex_part != 0 || kind.starts_with("Light") || kind.starts_with("Medium") {
                f.push_str(&format!(" + DEX {dex_part:+}"));
            }
            (base + dex_part, f)
        }
        None => (10 + dex_mod, format!("unarmoured 10 + DEX {dex_mod:+}")),
    };

    if let Some(shield) = equipped
        .iter()
        .find(|i| i.definition.kind.eq_ignore_ascii_case("Shield"))
    {
        let bonus = shield.definition.armor_class.unwrap_or(2);
        ac += bonus;
        formula.push_str(&format!(" + {} {bonus:+}", shield.definition.name));
    }

    let misc = sum(ch, "bonus", "armor-class");
    if misc != 0 {
        ac += misc;
        formula.push_str(&format!(" + modifiers {misc:+}"));
    }

    // TODO(phase 3): unarmored-armor-class (Monk, Barbarian) is a different
    // formula entirely and is not modelled. Rihanne wears leather, so this is
    // not on the critical path.
    Derived::new(ac, formula)
}

fn derive_skills(ch: &Character, pb: i32, scores: &[Score; 6]) -> Vec<Entry> {
    SKILLS
        .iter()
        .map(|(slug, name, ability)| {
            let base = scores[ability.index()].modifier;
            let expertise = has(ch, "expertise", slug);
            let proficient = expertise || has(ch, "proficiency", slug);
            let half = has(ch, "half-proficiency", slug);

            let prof_part = if expertise {
                pb * 2
            } else if proficient {
                pb
            } else if half {
                pb / 2
            } else {
                0
            };

            Entry {
                name: name.to_string(),
                value: base + prof_part + sum(ch, "bonus", slug),
                proficient,
                expertise,
            }
        })
        .collect()
}

fn derive_saves(ch: &Character, pb: i32, scores: &[Score; 6]) -> Vec<Entry> {
    Ability::ALL
        .iter()
        .map(|ability| {
            let sub = format!("{}-saving-throws", ability.slug());
            let proficient = has(ch, "proficiency", &sub);
            let base = scores[ability.index()].modifier;
            Entry {
                name: ability.abbrev().to_string(),
                value: base
                    + if proficient { pb } else { 0 }
                    + sum(ch, "bonus", &sub)
                    + sum(ch, "bonus", "saving-throws"),
                proficient,
                expertise: false,
            }
        })
        .collect()
}

fn derive_senses(ch: &Character) -> Vec<(String, i32)> {
    SENSE_SUBTYPES
        .iter()
        .filter_map(|sense| {
            // `set-base` takes the HIGHEST value, never the sum. Rihanne has
            // two darkvision entries, 60 and 120; she sees 120ft, not 180ft.
            active(ch, "set-base", sense)
                .filter_map(|m| m.value)
                .max()
                .map(|v| ((*sense).to_string(), v))
        })
        .collect()
}

fn collect_kind(ch: &Character, kind: &str) -> Vec<String> {
    let mut out: Vec<String> = ch
        .modifiers
        .all()
        .filter(|m| m.kind == kind && m.is_unconditional())
        .map(|m| m.sub_type.replace('-', " "))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn derive_conditionals(ch: &Character) -> Vec<Conditional> {
    ch.modifiers
        .all()
        .filter_map(|m| {
            m.condition().map(|c| Conditional {
                subject: format!("{} {}", m.kind, m.sub_type.replace('-', " ")),
                condition: c.to_string(),
            })
        })
        .collect()
}
