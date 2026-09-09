//! Narrow deserialisation of the D&D Beyond character payload.
//!
//! The live payload is ~200-550KB across 73 top-level keys. We model only what
//! the sheet needs. `serde` ignores unknown fields by default and every struct
//! is `#[serde(default)]`, so D&D Beyond can add, reorder or restructure
//! anything we don't read without breaking us. We only break if they rename or
//! remove a field named below — which is a far smaller surface than the DOM
//! scraping that keeps breaking Beyond20.

use serde::{Deserialize, Deserializer};

/// `#[serde(default)]` covers a *missing* field. It does not cover a field
/// that is present and explicitly `null`, which D&D Beyond does freely and
/// unpredictably — `alwaysPrepared: null` on a spell is what first caught us.
/// Every scalar below goes through this so a stray null degrades to the
/// default instead of failing the whole import.
fn nullable<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Character {
    #[serde(deserialize_with = "nullable")]
    pub id: i64,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub avatar_url: String,
    #[serde(deserialize_with = "nullable")]
    pub base_hit_points: i32,
    pub bonus_hit_points: Option<i32>,
    pub override_hit_points: Option<i32>,
    #[serde(deserialize_with = "nullable")]
    pub removed_hit_points: i32,
    #[serde(deserialize_with = "nullable")]
    pub temporary_hit_points: i32,
    #[serde(deserialize_with = "nullable")]
    pub inspiration: bool,
    pub stats: Vec<StatEntry>,
    pub bonus_stats: Vec<StatEntry>,
    pub override_stats: Vec<StatEntry>,
    pub race: Race,
    pub classes: Vec<Class>,
    pub inventory: Vec<InventoryItem>,
    pub custom_items: Vec<CustomItem>,
    pub currencies: Currencies,
    pub modifiers: Modifiers,
    pub death_saves: DeathSaves,
    pub actions: Actions,
    pub spells: SpellBuckets,
    pub class_spells: Vec<ClassSpells>,
    pub feats: Vec<Feat>,
    pub background: Background,
    pub notes: Notes,
    pub traits: Traits,
}

/// `stats` is a fixed 6-entry list ordered STR, DEX, CON, INT, WIS, CHA.
/// `value` is null throughout `overrideStats` when nothing is overridden.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default)]
pub struct StatEntry {
    #[serde(deserialize_with = "nullable")]
    pub id: i32,
    pub value: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Race {
    pub weight_speeds: Option<WeightSpeeds>,
    #[serde(deserialize_with = "nullable")]
    pub full_name: String,
    #[serde(deserialize_with = "nullable")]
    pub base_race_name: String,
    pub racial_traits: Vec<RacialTrait>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WeightSpeeds {
    pub normal: Option<Speeds>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Speeds {
    #[serde(deserialize_with = "nullable")]
    pub walk: i32,
    #[serde(deserialize_with = "nullable")]
    pub fly: i32,
    #[serde(deserialize_with = "nullable")]
    pub swim: i32,
    #[serde(deserialize_with = "nullable")]
    pub climb: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RacialTrait {
    pub definition: TraitDefinition,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TraitDefinition {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub snippet: String,
    /// D&D Beyond hides some traits from the rendered sheet (ability score
    /// increases and the like). Respect that or the tab fills with noise.
    #[serde(deserialize_with = "nullable")]
    pub hide_in_sheet: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Class {
    #[serde(deserialize_with = "nullable")]
    pub level: i32,
    /// What D&D Beyond last recorded. Seeds a fresh session only.
    #[serde(deserialize_with = "nullable")]
    pub hit_dice_used: i32,
    pub definition: NamedDefinition,
    pub subclass_definition: Option<NamedDefinition>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NamedDefinition {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    /// The die size: 8 means d8. Zero on definitions that are not classes.
    #[serde(deserialize_with = "nullable")]
    pub hit_dice: i32,
    /// 1=STR .. 6=CHA. Absent on classes that do not cast.
    pub spell_casting_ability_id: Option<i32>,
    /// Present on a class definition: every feature the class ever gets, at
    /// all 20 levels. Must be filtered by `required_level`.
    pub class_features: Vec<ClassFeature>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClassFeature {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub required_level: i32,
    #[serde(deserialize_with = "nullable")]
    pub display_order: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Actions {
    pub race: Vec<Action>,
    pub class: Vec<Action>,
    pub feat: Vec<Action>,
}

/// How often a limited use recharges.
///
/// D&D Beyond is not consistent here: actions and spells carry a numeric
/// `resetType`, while inventory items carry a *string* ("Dawn"). Both shapes
/// have to deserialise or the whole import fails on one wand.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ResetType {
    Code(i32),
    Name(String),
}

impl ResetType {
    /// Codes 1 and 2 are corroborated by this character's own data — the feat
    /// that says "Once per Short/Long Rest" carries 1, and the lineage spells
    /// that recharge overnight carry 2. Anything else is rendered as
    /// "special" rather than guessed at; a wrong recharge label is worse than
    /// no label.
    pub fn label(&self) -> String {
        match self {
            ResetType::Code(1) => "short rest".into(),
            ResetType::Code(2) => "long rest".into(),
            ResetType::Name(n) if !n.trim().is_empty() => n.to_lowercase(),
            _ => "special".into(),
        }
    }

    /// Whether a short rest restores it. Everything a short rest restores, a
    /// long rest restores too.
    pub fn on_short_rest(&self) -> bool {
        matches!(self, ResetType::Code(1))
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LimitedUse {
    #[serde(deserialize_with = "nullable")]
    pub max_uses: i32,
    /// What D&D Beyond last recorded. Used only to seed a fresh session.
    #[serde(deserialize_with = "nullable")]
    pub number_used: i32,
    pub reset_type: Option<ResetType>,
}

impl LimitedUse {
    /// Some entries carry only `maxNumberConsumed` and no cap — those are not
    /// a limited resource, they are a spell that can be upcast.
    pub fn is_real(&self) -> bool {
        self.max_uses > 0
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiceValue {
    #[serde(deserialize_with = "nullable")]
    pub dice_count: i32,
    #[serde(deserialize_with = "nullable")]
    pub dice_value: i32,
    #[serde(deserialize_with = "nullable")]
    pub fixed_value: i32,
}

impl DiceValue {
    pub fn is_real(&self) -> bool {
        self.dice_count > 0 && self.dice_value > 1
    }

    pub fn notation(&self) -> String {
        let mut s = format!("{}d{}", self.dice_count, self.dice_value);
        if self.fixed_value != 0 {
            s.push_str(&format!("{:+}", self.fixed_value));
        }
        s
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WeaponProperty {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    #[serde(deserialize_with = "nullable")]
    pub id: i64,
    pub limited_use: Option<LimitedUse>,
    /// Already scaled to the character's level — the `{{scalevalue}}` in the
    /// description text is resolved here, so no prose has to be parsed.
    pub dice: Option<DiceValue>,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub snippet: String,
    pub activation: Activation,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Activation {
    pub activation_type: Option<i32>,
}

impl Activation {
    /// D&D Beyond's activation codes. Unknown values render blank rather than
    /// guessing — a wrong action economy label is worse than none.
    pub fn label(&self) -> &'static str {
        match self.activation_type {
            Some(1) => "Action",
            Some(2) => "No Action",
            Some(3) => "Bonus Action",
            Some(4) => "Reaction",
            Some(6) => "1 Minute",
            Some(7) => "1 Hour",
            Some(8) => "Special",
            _ => "",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpellBuckets {
    pub race: Vec<SpellEntry>,
    pub class: Vec<SpellEntry>,
    pub item: Vec<SpellEntry>,
    pub feat: Vec<SpellEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ClassSpells {
    pub spells: Vec<SpellEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpellEntry {
    #[serde(deserialize_with = "nullable")]
    pub id: i64,
    pub limited_use: Option<LimitedUse>,
    pub definition: SpellDefinition,
    #[serde(deserialize_with = "nullable")]
    pub prepared: bool,
    #[serde(deserialize_with = "nullable")]
    pub always_prepared: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpellDefinition {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub level: i32,
    #[serde(deserialize_with = "nullable")]
    pub school: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub snippet: String,
    #[serde(deserialize_with = "nullable")]
    pub concentration: bool,
    #[serde(deserialize_with = "nullable")]
    pub ritual: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Feat {
    pub definition: FeatDefinition,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeatDefinition {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub snippet: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Background {
    pub definition: BackgroundDefinition,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackgroundDefinition {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub short_description: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Notes {
    pub allies: Option<String>,
    pub backstory: Option<String>,
    pub enemies: Option<String>,
    pub organizations: Option<String>,
    pub other_holdings: Option<String>,
    pub other_notes: Option<String>,
    pub personal_possessions: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Traits {
    pub personality_traits: Option<String>,
    pub ideals: Option<String>,
    pub bonds: Option<String>,
    pub flaws: Option<String>,
    pub appearance: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Currencies {
    #[serde(deserialize_with = "nullable")]
    pub cp: i32,
    #[serde(deserialize_with = "nullable")]
    pub sp: i32,
    #[serde(deserialize_with = "nullable")]
    pub gp: i32,
    #[serde(deserialize_with = "nullable")]
    pub ep: i32,
    #[serde(deserialize_with = "nullable")]
    pub pp: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomItem {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    pub description: Option<String>,
    pub quantity: Option<i32>,
    pub notes: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InventoryItem {
    #[serde(deserialize_with = "nullable")]
    pub id: i64,
    pub limited_use: Option<LimitedUse>,
    #[serde(deserialize_with = "nullable")]
    pub quantity: i32,
    #[serde(deserialize_with = "nullable")]
    pub equipped: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_attuned: bool,
    pub definition: ItemDefinition,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemDefinition {
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    /// Present on both armour ("Leather" -> 11) and shields ("Shield" -> 2),
    /// so it is not on its own enough to tell them apart. `type` is.
    pub armor_class: Option<i32>,
    /// "Light Armor" | "Medium Armor" | "Heavy Armor" | "Shield" | ...
    #[serde(rename = "type", deserialize_with = "nullable")]
    pub kind: String,
    #[serde(deserialize_with = "nullable")]
    pub filter_type: String,
    pub damage: Option<DiceValue>,
    #[serde(deserialize_with = "nullable")]
    pub damage_type: String,
    /// 1 = melee, 2 = ranged.
    pub attack_type: Option<i32>,
    #[serde(deserialize_with = "nullable")]
    pub range: i32,
    #[serde(deserialize_with = "nullable")]
    pub long_range: i32,
    /// 1 = Simple, 2 = Martial.
    #[serde(deserialize_with = "nullable")]
    pub category_id: i32,
    pub properties: Vec<WeaponProperty>,
    /// A magic weapon's bonuses arrive here rather than in the top-level
    /// modifier list.
    pub granted_modifiers: Vec<Modifier>,
    #[serde(deserialize_with = "nullable")]
    pub rarity: String,
    #[serde(deserialize_with = "nullable")]
    pub description: String,
    #[serde(deserialize_with = "nullable")]
    pub can_attune: bool,
}

/// The six modifier buckets. Order of application does not matter for the
/// arithmetic we do (all additive), but the bucket is worth keeping for
/// display — "where did this +1 come from?" is a question you ask at the table.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Modifiers {
    pub race: Vec<Modifier>,
    pub class: Vec<Modifier>,
    pub background: Vec<Modifier>,
    pub item: Vec<Modifier>,
    pub feat: Vec<Modifier>,
    pub condition: Vec<Modifier>,
}

impl Modifiers {
    pub fn all(&self) -> impl Iterator<Item = &Modifier> {
        self.race
            .iter()
            .chain(&self.class)
            .chain(&self.background)
            .chain(&self.item)
            .chain(&self.feat)
            .chain(&self.condition)
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Modifier {
    /// "bonus" | "set" | "set-base" | "proficiency" | "expertise"
    /// | "advantage" | "immunity" | "resistance" | "language" | ...
    #[serde(rename = "type", deserialize_with = "nullable")]
    pub kind: String,
    /// "dexterity-score", "stealth", "darkvision", "saving-throws", ...
    #[serde(deserialize_with = "nullable")]
    pub sub_type: String,
    pub value: Option<i32>,
    /// Non-empty means the modifier is conditional. We display these but never
    /// fold them into a derived number — e.g. a 40ft fly speed that only
    /// applies "in an area of dim light or darkness".
    pub restriction: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub requires_attunement: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_granted: bool,
}

impl Modifier {
    /// A modifier we may fold into a derived number.
    ///
    /// NOTE: this deliberately does **not** consult `is_granted`. That flag
    /// does not mean "active" — it appears to mean "granted automatically"
    /// as opposed to "chosen by the player from a list". Rihanne's Expertise
    /// in Stealth and four of her class skill proficiencies all arrive with
    /// `isGranted: false` and are unquestionably active. Filtering on it drops
    /// half the skill bonuses on any character who ever made a choice, which
    /// is every character.
    ///
    /// The only thing that genuinely gates a number is `restriction`.
    pub fn is_unconditional(&self) -> bool {
        self.restriction
            .as_deref()
            .map(|r| r.trim().is_empty())
            .unwrap_or(true)
    }

    /// Conditional modifiers are shown to the player rather than folded in —
    /// they are the ones you have to make a ruling about at the table.
    pub fn condition(&self) -> Option<&str> {
        self.restriction
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeathSaves {
    pub fail_count: Option<i32>,
    pub success_count: Option<i32>,
    #[serde(deserialize_with = "nullable")]
    pub is_stabilized: bool,
}
