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
    /// Base walking speed before exhaustion or conditions. `rules` applies
    /// those, because they change during play and this does not.
    pub walking_speed: i32,
    /// Present only for a class that casts.
    pub spellcasting: Option<Spellcasting>,
    pub carrying_capacity: i32,
    /// Languages, tools, armour and weapons. All present in the payload as
    /// modifiers and, until now, displayed nowhere.
    pub proficiencies: Proficiencies,
    /// One pool per die size, so a multiclass character keeps its 5d8 and its
    /// 3d10 apart rather than averaging them into nonsense.
    pub hit_dice: Vec<HitDicePool>,
    /// Weapon attacks, plus class features that carry their own dice.
    pub attacks: Vec<Attack>,
}

#[derive(Debug, Clone)]
pub struct Attack {
    pub name: String,
    /// None for a feature like Sneak Attack that adds damage to someone
    /// else's attack roll rather than making its own.
    pub to_hit: Option<i32>,
    /// Dice notation as rolled, e.g. "1d4+4".
    pub damage: String,
    pub damage_dice: (u32, u32),
    pub damage_modifier: i32,
    pub damage_type: String,
    pub range: String,
    pub proficient: bool,
    pub ability: Option<Ability>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitDicePool {
    /// 8 means d8.
    pub die: i32,
    pub total: i32,
}

impl HitDicePool {
    pub fn label(&self) -> String {
        format!("d{}", self.die)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Proficiencies {
    pub languages: Vec<String>,
    pub tools: Vec<String>,
    pub armor: Vec<String>,
    pub weapons: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Spellcasting {
    pub ability: Ability,
    pub save_dc: i32,
    pub attack_bonus: i32,
    /// Maxima by slot level, index 0 being level 1. The payload ships these as
    /// zero, exactly like armour class and proficiency bonus, so they come
    /// from the class tables.
    pub slots: [u8; 9],
    /// Warlock only: (slots, the level they are cast at). Its own pool, and it
    /// comes back on a short rest.
    pub pact: Option<(u8, u8)>,
    pub caster_level: i32,
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
    // Attacks depend on the finished sheet (ability modifiers, proficiency
    // bonus, weapon proficiencies), so they are a second pass over it.
    let mut sheet = derive_base(ch, overrides);
    sheet.attacks = derive_attacks(ch, &sheet);
    sheet
}

fn derive_base(ch: &Character, overrides: &AbilityOverrides) -> Sheet {
    let total_level: i32 = ch.classes.iter().map(|c| c.level).sum();
    let pb = proficiency_bonus(total_level);
    let scores = derive_scores(ch, overrides);
    let mod_of = |a: Ability| scores[a.index()].modifier;

    let hp = derive_hp(ch, total_level, mod_of(Ability::Con));
    let armor_class = derive_ac(
        ch,
        mod_of(Ability::Dex),
        mod_of(Ability::Con),
        mod_of(Ability::Wis),
    );

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
        walking_speed: ch
            .race
            .weight_speeds
            .as_ref()
            .and_then(|w| w.normal.as_ref())
            .map(|n| n.walk)
            .filter(|w| *w > 0)
            .unwrap_or(30),
        spellcasting: derive_spellcasting(ch, pb, &scores),
        carrying_capacity: crate::rules::carrying_capacity(scores[Ability::Str.index()].score),
        proficiencies: derive_proficiencies(ch),
        hit_dice: derive_hit_dice(ch),
        attacks: Vec::new(), // filled below, once the sheet exists
    }
}

/// Weapon attacks come from equipped weapons and from features whose dice the
/// payload has already scaled. Everything needed is structured — the damage
/// dice, the damage type, the Finesse property — so nothing is parsed out of
/// description prose.
pub fn derive_attacks(ch: &Character, sheet: &Sheet) -> Vec<Attack> {
    let mut out = Vec::new();
    let str_mod = sheet.modifier(Ability::Str);
    let dex_mod = sheet.modifier(Ability::Dex);
    let pb = sheet.proficiency_bonus;
    let profs = &sheet.proficiencies.weapons;

    for item in ch.inventory.iter().filter(|i| i.equipped) {
        let d = &item.definition;
        let Some(dmg) = d.damage.as_ref().filter(|x| x.is_real()) else {
            continue;
        };

        let is_ranged = d.attack_type == Some(2);
        let finesse = d.properties.iter().any(|p| p.name.eq_ignore_ascii_case("Finesse"));
        let ability = crate::rules::attack_ability(is_ranged, finesse, str_mod, dex_mod);
        let ability_mod = sheet.modifier(ability);
        let proficient = crate::rules::weapon_proficient(d.category_id, &d.kind, profs);

        // A magic weapon's plus arrives on the item, not in the character's
        // modifier list.
        let magic: i32 = d
            .granted_modifiers
            .iter()
            .filter(|m| m.kind == "bonus" && m.sub_type == "magic")
            .filter_map(|m| m.value)
            .sum();

        let to_hit = ability_mod + if proficient { pb } else { 0 } + magic;
        let dmg_mod = ability_mod + magic + dmg.fixed_value;

        let range = if is_ranged && d.range > 0 {
            format!("{}/{} ft", d.range, d.long_range)
        } else if d.range > 5 {
            format!("{} ft", d.range)
        } else {
            "melee".to_string()
        };

        let mut notes: Vec<String> = d.properties.iter().map(|p| p.name.clone()).collect();
        if !proficient {
            notes.insert(0, "not proficient".into());
        }

        out.push(Attack {
            name: d.name.clone(),
            to_hit: Some(to_hit),
            damage: format!(
                "{}d{}{}",
                dmg.dice_count,
                dmg.dice_value,
                if dmg_mod == 0 { String::new() } else { format!("{dmg_mod:+}") }
            ),
            damage_dice: (dmg.dice_count.max(0) as u32, dmg.dice_value.max(2) as u32),
            damage_modifier: dmg_mod,
            damage_type: d.damage_type.clone(),
            range,
            proficient,
            ability: Some(ability),
            notes: notes.join(", "),
        });
    }

    // Features with their own dice — Sneak Attack and its kin. These have no
    // attack roll of their own; they ride on one.
    for list in [&ch.actions.class, &ch.actions.race, &ch.actions.feat] {
        for a in list {
            let Some(dice) = a.dice.as_ref().filter(|d| d.is_real()) else {
                continue;
            };
            out.push(Attack {
                name: a.name.clone(),
                to_hit: None,
                damage: dice.notation(),
                damage_dice: (dice.dice_count.max(0) as u32, dice.dice_value.max(2) as u32),
                damage_modifier: dice.fixed_value,
                damage_type: String::new(),
                range: String::new(),
                proficient: true,
                ability: None,
                notes: "extra damage, no attack roll of its own".into(),
            });
        }
    }
    out
}

/// A character has one Hit Point Die per class level, of that class's size.
fn derive_hit_dice(ch: &Character) -> Vec<HitDicePool> {
    let mut pools: Vec<HitDicePool> = Vec::new();
    for c in &ch.classes {
        let die = c.definition.hit_dice;
        if die <= 0 || c.level <= 0 {
            continue;
        }
        match pools.iter_mut().find(|p| p.die == die) {
            Some(p) => p.total += c.level,
            None => pools.push(HitDicePool { die, total: c.level }),
        }
    }
    pools.sort_by_key(|p| p.die);
    pools
}

fn derive_proficiencies(ch: &Character) -> Proficiencies {
    let mut p = Proficiencies::default();
    let skills: Vec<&str> = SKILLS.iter().map(|(slug, _, _)| *slug).collect();

    for m in ch.modifiers.all().filter(|m| m.is_unconditional()) {
        let sub = m.sub_type.as_str();
        match m.kind.as_str() {
            "language" => p.languages.push(title_case(sub)),
            "proficiency" => {
                // Skills and saves have their own rows on the sheet already.
                if skills.contains(&sub) || sub.ends_with("saving-throws") {
                    continue;
                }
                if sub.contains("armor") || sub == "shields" {
                    p.armor.push(title_case(sub));
                } else if sub.contains("weapon")
                    || WEAPON_SLUGS.contains(&sub)
                {
                    p.weapons.push(title_case(sub));
                } else {
                    p.tools.push(title_case(sub));
                }
            }
            _ => {}
        }
    }
    for list in [&mut p.languages, &mut p.tools, &mut p.armor, &mut p.weapons] {
        list.sort();
        list.dedup();
    }
    p
}

/// Specific weapons D&D Beyond names individually rather than by category.
const WEAPON_SLUGS: [&str; 12] = [
    "rapier", "scimitar", "shortsword", "whip", "crossbow-hand", "longsword",
    "shortbow", "longbow", "dagger", "dart", "sling", "quarterstaff",
];

fn title_case(slug: &str) -> String {
    slug.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn derive_spellcasting(ch: &Character, pb: i32, scores: &[Score; 6]) -> Option<Spellcasting> {
    use crate::rules::{caster_kind, CasterKind};

    let kinds: Vec<(CasterKind, i32)> = ch
        .classes
        .iter()
        .map(|c| {
            let sub = c.subclass_definition.as_ref().map(|s| s.name.as_str()).unwrap_or("");
            (caster_kind(&c.definition.name, sub), c.level)
        })
        .collect();

    let slots = crate::rules::spell_slots(&kinds);
    let pact = kinds
        .iter()
        .find(|(k, _)| *k == CasterKind::Pact)
        .map(|(_, level)| crate::rules::pact_slots(*level))
        .filter(|(n, _)| *n > 0);

    // Nothing to report for a character with no magic at all.
    if slots.iter().all(|n| *n == 0) && pact.is_none() {
        return None;
    }

    // 1=STR .. 6=CHA, the same ordering as `stats`. On a multiclass caster the
    // payload names one per class; the first is used for the headline DC, and
    // an individual spell carries its own casting ability if it differs.
    let id = ch
        .classes
        .iter()
        .find_map(|c| c.definition.spell_casting_ability_id)
        .unwrap_or(6);
    let ability = *Ability::ALL.get((id - 1).clamp(0, 5) as usize)?;
    let m = scores[ability.index()].modifier;

    Some(Spellcasting {
        ability,
        save_dc: crate::rules::spell_save_dc(pb, m),
        attack_bonus: crate::rules::spell_attack_bonus(pb, m),
        slots,
        pact,
        caster_level: crate::rules::caster_level(&kinds),
    })
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

fn derive_ac(ch: &Character, dex_mod: i32, con_mod: i32, wis_mod: i32) -> Derived {
    let equipped: Vec<_> = ch.inventory.iter().filter(|i| i.equipped).collect();

    // `armorClass` is present on shields too, so the item's `type` is what
    // distinguishes body armour from a shield.
    let body = equipped
        .iter()
        .find(|i| i.definition.kind.ends_with("Armor") && i.definition.armor_class.is_some());

    // Unarmored Defense replaces the base entirely, and only applies when you
    // are wearing no armour.
    if body.is_none() {
        for class in &ch.classes {
            if let Some((ac, formula)) =
                crate::rules::unarmored_defense(&class.definition.name, dex_mod, con_mod, wis_mod)
            {
                let shield = equipped
                    .iter()
                    .find(|i| i.definition.kind.eq_ignore_ascii_case("Shield"));
                // A Barbarian may add a shield; a Monk may not.
                let allow_shield = class.definition.name.eq_ignore_ascii_case("barbarian");
                return match (shield, allow_shield) {
                    (Some(s), true) => {
                        let b = s.definition.armor_class.unwrap_or(2);
                        Derived::new(ac + b, format!("{formula} + {} {b:+}", s.definition.name))
                    }
                    _ => Derived::new(ac, formula),
                };
            }
        }
    }

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
            // `set-base` takes the HIGHEST value, never the sum. A character
            // with darkvision entries of 60 and 120 sees 120ft, not 180ft.
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
