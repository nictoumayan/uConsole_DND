//! Static 5e reference data. Nothing here comes from D&D Beyond — these are
//! game constants, and the ones that are rules text are SRD 5.2.1 (CC-BY-4.0).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ability {
    Str,
    Dex,
    Con,
    Int,
    Wis,
    Cha,
}

impl Ability {
    /// Index into the fixed 6-entry `stats` / `bonusStats` / `overrideStats`
    /// arrays, which D&D Beyond always orders STR, DEX, CON, INT, WIS, CHA.
    pub const ALL: [Ability; 6] = [
        Ability::Str,
        Ability::Dex,
        Ability::Con,
        Ability::Int,
        Ability::Wis,
        Ability::Cha,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap()
    }

    pub fn abbrev(self) -> &'static str {
        match self {
            Ability::Str => "STR",
            Ability::Dex => "DEX",
            Ability::Con => "CON",
            Ability::Int => "INT",
            Ability::Wis => "WIS",
            Ability::Cha => "CHA",
        }
    }

    /// The slug D&D Beyond uses in modifier subTypes, e.g. "dexterity-score",
    /// "dexterity-saving-throws".
    pub fn slug(self) -> &'static str {
        match self {
            Ability::Str => "strength",
            Ability::Dex => "dexterity",
            Ability::Con => "constitution",
            Ability::Int => "intelligence",
            Ability::Wis => "wisdom",
            Ability::Cha => "charisma",
        }
    }
}

/// (modifier subType, display name, governing ability)
pub const SKILLS: [(&str, &str, Ability); 18] = [
    ("acrobatics", "Acrobatics", Ability::Dex),
    ("animal-handling", "Animal Handling", Ability::Wis),
    ("arcana", "Arcana", Ability::Int),
    ("athletics", "Athletics", Ability::Str),
    ("deception", "Deception", Ability::Cha),
    ("history", "History", Ability::Int),
    ("insight", "Insight", Ability::Wis),
    ("intimidation", "Intimidation", Ability::Cha),
    ("investigation", "Investigation", Ability::Int),
    ("medicine", "Medicine", Ability::Wis),
    ("nature", "Nature", Ability::Int),
    ("perception", "Perception", Ability::Wis),
    ("performance", "Performance", Ability::Cha),
    ("persuasion", "Persuasion", Ability::Cha),
    ("religion", "Religion", Ability::Int),
    ("sleight-of-hand", "Sleight of Hand", Ability::Dex),
    ("stealth", "Stealth", Ability::Dex),
    ("survival", "Survival", Ability::Wis),
];

pub const SENSE_SUBTYPES: [&str; 4] = ["darkvision", "blindsight", "truesight", "tremorsense"];

/// 5e proficiency bonus progression: +2 at 1st, stepping every four levels.
pub fn proficiency_bonus(total_level: i32) -> i32 {
    2 + (total_level.max(1) - 1) / 4
}

pub fn ability_modifier(score: i32) -> i32 {
    // Rust integer division truncates toward zero, which is wrong for odd
    // scores below 10: (9 - 10) / 2 == 0, but the modifier is -1.
    (score - 10).div_euclid(2)
}

/// Class features whose value is a die expression rather than a number, keyed
/// off class + level. Deliberately tiny — add rows as you play other classes.
pub fn class_notes(class_name: &str, level: i32) -> Vec<String> {
    let mut out = Vec::new();
    if class_name.eq_ignore_ascii_case("rogue") {
        out.push(format!("Sneak Attack {}d6", (level + 1) / 2));
    }
    out
}
