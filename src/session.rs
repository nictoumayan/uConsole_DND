//! Mutable play state, layered on top of the immutable snapshot.
//!
//! This is the decision the whole project was designed around: the snapshot is
//! what D&D Beyond sent and is never written to, and everything that changes
//! during play lives here, in its own file. Re-importing after a level-up
//! replaces the snapshot without touching the HP you are tracking mid-combat.
//!
//! Nothing here is ever pushed back to D&D Beyond.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The fourteen conditions from SRD 5.2.1 (CC-BY-4.0). Exhaustion is tracked
/// separately because it has levels rather than being a simple toggle.
pub const CONDITIONS: [&str; 14] = [
    "Blinded",
    "Charmed",
    "Deafened",
    "Frightened",
    "Grappled",
    "Incapacitated",
    "Invisible",
    "Paralyzed",
    "Petrified",
    "Poisoned",
    "Prone",
    "Restrained",
    "Stunned",
    "Unconscious",
];

pub const MAX_EXHAUSTION: u8 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Bumped only on a breaking change to this file's shape.
    pub version: u32,
    pub character_id: i64,
    /// Hit points removed. Stored as damage rather than as a current total so
    /// that a level-up — which raises max HP in the snapshot — leaves you
    /// wounded by the same amount rather than mysteriously healed.
    pub damage: i32,
    pub temporary_hp: i32,
    pub conditions: Vec<String>,
    pub exhaustion: u8,
    pub death_successes: u8,
    pub death_failures: u8,
    pub inspiration: bool,
    /// Final ability scores the player has corrected by hand, keyed "STR".."CHA".
    ///
    /// This exists because D&D Beyond does not ship everything its own sheet
    /// applies. The 2024 background ability increases are the known case: the
    /// Scribe background grants +1/+1/+1 across Dexterity, Intelligence and
    /// Wisdom, D&D Beyond's website applies them, and the character JSON
    /// contains no trace — `"wisdom-score"` appears zero times in a 200KB
    /// payload. There is nothing to compute from, so the number has to come
    /// from the player.
    ///
    /// Correcting the score rather than the derived value is deliberate: one
    /// entry here fixes armour class, passive perception, every save, every
    /// skill and initiative at once, and stays correct as the character levels.
    #[serde(default)]
    pub ability_overrides: BTreeMap<String, i32>,

    /// Expended limited uses, keyed `"<kind>:<id>"`.
    ///
    /// The recharge type is stored alongside the count rather than looked up
    /// from the snapshot, so a rest works from the session alone and a
    /// re-import cannot orphan the bookkeeping. BTreeMap keeps the JSON
    /// stable, which makes the file diffable.
    #[serde(default)]
    pub uses: BTreeMap<String, UseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseEntry {
    pub used: u32,
    pub max: u32,
    /// "short rest", "long rest", "dawn", "special".
    pub reset: String,
}

impl Session {
    /// A fresh session seeded from whatever the snapshot last recorded, so the
    /// first launch agrees with the website instead of starting at full HP.
    pub fn seed(character_id: i64, removed_hp: i32, temp_hp: i32, inspiration: bool) -> Session {
        Session {
            version: 1,
            character_id,
            damage: removed_hp.max(0),
            temporary_hp: temp_hp.max(0),
            conditions: Vec::new(),
            exhaustion: 0,
            death_successes: 0,
            death_failures: 0,
            inspiration,
            ability_overrides: BTreeMap::new(),
            uses: BTreeMap::new(),
        }
    }

    pub fn load_or_seed(
        path: &Path,
        character_id: i64,
        removed_hp: i32,
        temp_hp: i32,
        inspiration: bool,
    ) -> Session {
        // A corrupt or stale session must never block the sheet — the sheet is
        // the thing you need at the table. Fall back to a seeded one.
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Session>(&raw).ok())
            .filter(|s| s.character_id == character_id && s.version == 1)
            .unwrap_or_else(|| Session::seed(character_id, removed_hp, temp_hp, inspiration))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let json = serde_json::to_string_pretty(self).context("serialising session")?;
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
    }

    pub fn current_hp(&self, max_hp: i32) -> i32 {
        (max_hp - self.damage).max(0)
    }

    pub fn is_dying(&self, max_hp: i32) -> bool {
        self.current_hp(max_hp) == 0 && !self.is_dead()
    }

    pub fn is_dead(&self) -> bool {
        self.death_failures >= 3
    }

    pub fn is_stable(&self) -> bool {
        self.death_successes >= 3
    }

    /// Damage hits temporary hit points first, then real ones, and never
    /// drives HP below zero. Damage taken while already at zero is a failed
    /// death save, which is the rule people forget mid-fight.
    pub fn take_damage(&mut self, amount: i32, max_hp: i32) {
        let amount = amount.max(0);
        if amount == 0 {
            return;
        }
        if self.current_hp(max_hp) == 0 {
            self.fail_death_save();
            return;
        }
        let absorbed = amount.min(self.temporary_hp);
        self.temporary_hp -= absorbed;
        let rest = amount - absorbed;
        self.damage = (self.damage + rest).min(max_hp);

        if self.current_hp(max_hp) == 0 {
            // Dropping to zero ends any prior death-save progress and knocks
            // you out.
            self.death_successes = 0;
            self.death_failures = 0;
            self.add_condition("Unconscious");
        }
    }

    /// Any healing above zero brings you back and wipes death-save progress.
    pub fn heal(&mut self, amount: i32, max_hp: i32) {
        let amount = amount.max(0);
        if amount == 0 {
            return;
        }
        let was_down = self.current_hp(max_hp) == 0;
        self.damage = (self.damage - amount).max(0);
        if was_down && self.current_hp(max_hp) > 0 {
            self.death_successes = 0;
            self.death_failures = 0;
            self.remove_condition("Unconscious");
        }
    }

    /// Temporary hit points do not stack — you take the better pool.
    pub fn set_temp_hp(&mut self, amount: i32) {
        self.temporary_hp = self.temporary_hp.max(amount.max(0));
    }

    pub fn has_condition(&self, name: &str) -> bool {
        self.conditions.iter().any(|c| c == name)
    }

    pub fn add_condition(&mut self, name: &str) {
        if !self.has_condition(name) {
            self.conditions.push(name.to_string());
        }
    }

    pub fn remove_condition(&mut self, name: &str) {
        self.conditions.retain(|c| c != name);
    }

    pub fn toggle_condition(&mut self, name: &str) {
        if self.has_condition(name) {
            self.remove_condition(name);
        } else {
            self.add_condition(name);
        }
    }

    pub fn adjust_exhaustion(&mut self, delta: i8) {
        let next = self.exhaustion as i16 + delta as i16;
        self.exhaustion = next.clamp(0, MAX_EXHAUSTION as i16) as u8;
    }

    pub fn succeed_death_save(&mut self) {
        self.death_successes = (self.death_successes + 1).min(3);
    }

    pub fn fail_death_save(&mut self) {
        self.death_failures = (self.death_failures + 1).min(3);
    }

    pub fn clear_death_saves(&mut self) {
        self.death_successes = 0;
        self.death_failures = 0;
    }

    // -- ability overrides -------------------------------------------------

    pub fn ability_override(&self, abbrev: &str) -> Option<i32> {
        self.ability_overrides.get(abbrev).copied()
    }

    /// Scores are clamped to 1..=30 — the legal range. A typo that sets a
    /// score to 300 should not silently produce a +145 modifier.
    pub fn set_ability_override(&mut self, abbrev: &str, score: i32) {
        self.ability_overrides.insert(abbrev.to_string(), score.clamp(1, 30));
    }

    pub fn clear_ability_override(&mut self, abbrev: &str) {
        self.ability_overrides.remove(abbrev);
    }

    // -- limited uses ------------------------------------------------------

    pub fn uses_of(&self, key: &str) -> u32 {
        self.uses.get(key).map(|e| e.used).unwrap_or(0)
    }

    pub fn remaining(&self, key: &str, max: u32) -> u32 {
        max.saturating_sub(self.uses_of(key))
    }

    /// Spend one. Saturates at the maximum rather than going negative on the
    /// remaining count.
    pub fn spend_use(&mut self, key: &str, max: u32, reset: &str) {
        let e = self.uses.entry(key.to_string()).or_insert(UseEntry {
            used: 0,
            max,
            reset: reset.to_string(),
        });
        e.max = max;
        e.reset = reset.to_string();
        e.used = (e.used + 1).min(max);
    }

    /// Hand one back — for the misclick, and for a DM who rules that one did
    /// not count.
    pub fn restore_use(&mut self, key: &str) {
        if let Some(e) = self.uses.get_mut(key) {
            e.used = e.used.saturating_sub(1);
            if e.used == 0 {
                self.uses.remove(key);
            }
        }
    }

    /// A short rest restores only what recharges on one. Hit dice are spent
    /// deliberately and are not tracked yet, so this is the whole of it.
    pub fn short_rest(&mut self) {
        self.uses.retain(|_, e| e.reset != "short rest");
    }

    /// Full hit points, no temporary pool, death saves cleared, one level of
    /// exhaustion removed, and the conditions a night's sleep ends.
    pub fn long_rest(&mut self) {
        self.damage = 0;
        self.temporary_hp = 0;
        self.clear_death_saves();
        self.adjust_exhaustion(-1);
        self.remove_condition("Unconscious");
        // Everything recharges. A long rest spans dawn in all but contrived
        // cases, and anything it should not have restored can be re-spent.
        self.uses.clear();
    }

    /// How many limited-use things are currently expended — for a header chip
    /// that answers "have I used anything I have forgotten about?".
    pub fn expended_count(&self) -> usize {
        self.uses.values().filter(|e| e.used > 0).count()
    }

    /// A short status chip for the header: "Poisoned, Prone, Exhaustion 2".
    pub fn condition_summary(&self) -> String {
        let mut parts: Vec<String> = self.conditions.clone();
        if self.exhaustion > 0 {
            parts.push(format!("Exhaustion {}", self.exhaustion));
        }
        parts.join(", ")
    }
}
