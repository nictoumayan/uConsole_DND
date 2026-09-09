//! Interaction state, deliberately free of ratatui and of any terminal.
//!
//! Every keypress is a method on `App`. That means the whole interaction model
//! — tab switching, selection memory, filtering, the detail overlay — is
//! testable headlessly, which matters when the target hardware is in the post.

use crate::content::{rows_for, RollKind, Row, Tab};
use crate::rules;
use crate::dice::{self, Advantage, Expr, Rng, Roll};
use crate::ddb::Character;
use crate::derive::{tables::Ability, Sheet};
use crate::portrait::Portrait;
use crate::session::{Session, CONDITIONS};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Browsing the list on the current tab.
    List,
    /// A row is open full-screen with its full rules text.
    Detail,
    /// Typing into the filter box; keys are text, not commands.
    Filter,
    /// Typing a number — damage, healing, temporary hit points.
    Number(NumberTarget),
    /// The condition toggles.
    Conditions,
    /// Short or long rest.
    Rest,
    /// Typing a dice expression such as `2d6+3`.
    Dice,
    /// The roll log, full-screen.
    RollLog,
    /// Typing a character sheet URL to load.
    Load,
    /// Correcting ability scores by hand.
    Abilities,
    /// Mid short rest, spending Hit Point Dice one at a time.
    ShortRest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberTarget {
    Damage,
    Heal,
    TempHp,
}

impl NumberTarget {
    pub fn prompt(self) -> &'static str {
        match self {
            NumberTarget::Damage => "damage",
            NumberTarget::Heal => "heal",
            NumberTarget::TempHp => "temp hp",
        }
    }
}

pub struct App {
    pub sheet: Sheet,
    pub character: Character,
    pub tab: Tab,
    /// Selection is remembered per tab — coming back to GEAR should not dump
    /// you at the top again.
    selected: [usize; Tab::ALL.len()],
    pub detail_scroll: usize,
    pub mode: Mode,
    pub filter: String,
    pub quit: bool,
    /// Rows visible in the content pane; set by the renderer each frame so
    /// paging keys move by a real screenful.
    pub page_rows: usize,

    // -- play state --------------------------------------------------------
    pub session: Session,
    pub session_path: PathBuf,
    /// Digits typed so far in `Mode::Number`.
    pub number_buffer: String,
    /// Cursor in the conditions overlay. Index `CONDITIONS.len()` is the
    /// exhaustion row, which is a counter rather than a toggle.
    pub condition_cursor: usize,
    /// Surfaced in the footer. A save that fails silently is a session lost.
    pub last_error: Option<String>,

    // -- dice --------------------------------------------------------------
    rng: Rng,
    /// Most recent first. Capped, because this is a session log and not a
    /// campaign history.
    pub rolls: Vec<Roll>,
    pub dice_buffer: String,
    pub dice_error: Option<String>,
    /// Cursor in the ability-override overlay.
    pub ability_cursor: usize,
    /// Which hit dice pool the short-rest screen is spending from.
    pub hit_die_cursor: usize,
    /// Hit points regained so far this short rest.
    pub rest_healed: i32,
    /// Why the last roll came out the way it did — conditions, exhaustion,
    /// cancellation. Shown so a roll never silently changes itself.
    pub last_resolution: Vec<String>,

    // -- loading a character -----------------------------------------------
    /// The portrait lives here so swapping characters replaces it too.
    pub portrait: Option<Portrait>,
    pub load_buffer: String,
    pub load_status: LoadStatus,
    /// Set by the keymap, acted on by the event loop after it has drawn a
    /// "fetching" frame — otherwise the UI freezes with no explanation while
    /// the network call blocks.
    pub pending_load: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    Idle,
    Fetching,
    Failed(String),
}

/// How many rolls the log keeps.
pub const ROLL_LOG_CAP: usize = 40;

impl App {
    pub fn new(
        sheet: Sheet,
        character: Character,
        session: Session,
        session_path: PathBuf,
        portrait: Option<Portrait>,
    ) -> App {
        App {
            sheet,
            character,
            tab: Tab::Vitals,
            selected: [0; Tab::ALL.len()],
            detail_scroll: 0,
            mode: Mode::List,
            filter: String::new(),
            quit: false,
            page_rows: 15,
            session,
            session_path,
            number_buffer: String::new(),
            condition_cursor: 0,
            last_error: None,
            rng: Rng::from_entropy(),
            rolls: Vec::new(),
            dice_buffer: String::new(),
            dice_error: None,
            ability_cursor: 0,
            hit_die_cursor: 0,
            rest_healed: 0,
            last_resolution: Vec::new(),
            portrait,
            load_buffer: String::new(),
            load_status: LoadStatus::Idle,
            pending_load: None,
        }
    }

    // -- loading a character -----------------------------------------------

    pub fn open_load(&mut self) {
        self.load_buffer.clear();
        self.load_status = LoadStatus::Idle;
        self.mode = Mode::Load;
    }

    pub fn push_load(&mut self, c: char) {
        if self.load_buffer.len() < 200 {
            self.load_buffer.push(c);
            self.load_status = LoadStatus::Idle;
        }
    }

    pub fn pop_load(&mut self) {
        self.load_buffer.pop();
        self.load_status = LoadStatus::Idle;
    }

    /// Validate here and hand the id to the event loop, which owns the network.
    /// A bad URL should say so instantly rather than after a round trip.
    pub fn submit_load(&mut self) {
        match crate::ddb::fetch::parse_character_id(&self.load_buffer) {
            Ok(id) => {
                self.load_status = LoadStatus::Fetching;
                self.pending_load = Some(id);
            }
            Err(e) => self.load_status = LoadStatus::Failed(format!("{e}")),
        }
    }

    /// Replace everything: character, sheet, session, portrait. The old
    /// session file is left alone — switching characters is not deleting one.
    pub fn adopt(
        &mut self,
        character: Character,
        session: Session,
        session_path: PathBuf,
        portrait: Option<Portrait>,
    ) {
        self.sheet = crate::derive::derive_with(&character, &session.ability_overrides);
        self.character = character;
        self.session = session;
        self.session_path = session_path;
        self.portrait = portrait;
        self.tab = Tab::Vitals;
        self.selected = [0; Tab::ALL.len()];
        self.filter.clear();
        self.rolls.clear();
        self.last_resolution.clear();
        self.load_buffer.clear();
        self.load_status = LoadStatus::Idle;
        self.mode = Mode::List;
    }

    pub fn load_failed(&mut self, message: String) {
        self.load_status = LoadStatus::Failed(message);
        self.mode = Mode::Load;
    }

    /// True when there is nothing to show yet — a first run.
    pub fn is_empty(&self) -> bool {
        self.character.name.is_empty()
    }

    /// Deterministic dice, for tests.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = Rng::from_seed(seed);
        self
    }

    // -- dice --------------------------------------------------------------

    pub fn last_roll(&self) -> Option<&Roll> {
        self.rolls.first()
    }

    fn record(&mut self, r: Roll) {
        self.rolls.insert(0, r);
        self.rolls.truncate(ROLL_LOG_CAP);
    }

    /// Roll whatever the cursor is on.
    ///
    /// The advantage you ask for is a *request*, not an instruction: the rules
    /// engine folds in the character's standing advantages, every condition in
    /// play, and exhaustion, then applies the cancellation rule. Being Poisoned
    /// and pressing `a` correctly produces a straight roll, which is exactly
    /// the case people get wrong at a table.
    pub fn roll_selected(&mut self, advantage: Advantage) {
        let Some(row) = self.selected_row() else { return };
        let Some(spec) = row.roll else { return };

        let kind = match spec.kind {
            RollKind::Initiative => rules::TestKind::Initiative,
            RollKind::Attack => rules::TestKind::Attack,
            RollKind::DeathSave => rules::TestKind::DeathSave,
            RollKind::Save => rules::TestKind::Save {
                ability: spec.ability.unwrap_or(Ability::Dex),
            },
            RollKind::Check => rules::TestKind::Check {
                ability: spec.ability.unwrap_or(Ability::Dex),
                skill: spec.skill,
            },
        };

        let res = rules::resolve(
            kind,
            &self.session.conditions,
            self.session.exhaustion,
            &self.sheet.advantages,
            advantage,
        );
        self.last_resolution = res.sources.clone();

        let expr = Expr::d20(spec.modifier + res.penalty);
        let result = dice::roll(&mut self.rng, &row.name, expr, res.advantage);

        if spec.kind == RollKind::DeathSave {
            // A natural 20 on a death save brings you back with one hit point;
            // a natural 1 counts as two failures.
            if result.is_nat20() {
                self.session.clear_death_saves();
                self.session.heal(1, self.max_hp());
            } else if result.is_nat1() {
                self.session.fail_death_save();
                self.session.fail_death_save();
            } else if result.total >= 10 {
                self.session.succeed_death_save();
            } else {
                self.session.fail_death_save();
            }
            self.persist();
        }

        self.record(result);
    }

    /// Roll the damage of the selected attack. Conditions and exhaustion do
    /// not touch damage — they modify d20 tests, and this is not one.
    pub fn roll_damage(&mut self) {
        let Some(row) = self.selected_row() else { return };
        let Some(d) = row.damage else { return };
        let expr = Expr { count: d.count.max(1), sides: d.sides.max(2), modifier: d.modifier };
        let label = if d.damage_type.is_empty() {
            format!("{} damage", row.name)
        } else {
            format!("{} {}", row.name, d.damage_type.to_lowercase())
        };
        let r = dice::roll(&mut self.rng, &label, expr, Advantage::Normal);
        self.last_resolution.clear();
        self.record(r);
    }

    /// Whether the cursor is on something that rolls, so the footer and the
    /// enter key can do the obvious thing for this row rather than this tab.
    pub fn selected_is_rollable(&self) -> bool {
        self.selected_row().is_some_and(|r| r.roll.is_some())
    }

    pub fn selected_has_damage(&self) -> bool {
        self.selected_row().is_some_and(|r| r.damage.is_some())
    }

    pub fn start_dice(&mut self) {
        self.dice_buffer.clear();
        self.dice_error = None;
        self.mode = Mode::Dice;
    }

    pub fn push_dice(&mut self, c: char) {
        if self.dice_buffer.len() < 16 {
            self.dice_buffer.push(c);
            self.dice_error = None;
        }
    }

    pub fn pop_dice(&mut self) {
        self.dice_buffer.pop();
        self.dice_error = None;
    }

    /// Stays in the prompt on a parse error so you can fix the typo instead of
    /// retyping it.
    pub fn commit_dice(&mut self, advantage: Advantage) {
        match Expr::parse(&self.dice_buffer) {
            Ok(expr) => {
                let label = self.dice_buffer.clone();
                let r = dice::roll(&mut self.rng, &label, expr, advantage);
                self.record(r);
                self.dice_buffer.clear();
                self.dice_error = None;
                self.mode = Mode::List;
            }
            Err(e) => self.dice_error = Some(e),
        }
    }

    pub fn open_roll_log(&mut self) {
        self.mode = Mode::RollLog;
    }

    /// Passive Perception with the SRD's +/-5 for Advantage or Disadvantage on
    /// Perception checks — being Poisoned genuinely lowers how alert you are.
    pub fn effective_passive_perception(&self) -> i32 {
        let bonus = self
            .sheet
            .skills
            .iter()
            .find(|e| e.name == "Perception")
            .map(|e| e.value)
            .unwrap_or(0);
        rules::passive_perception(bonus, &self.session.conditions, &self.sheet.advantages)
    }

    /// Walking speed after exhaustion and any condition that pins you down.
    pub fn effective_speed(&self) -> i32 {
        rules::effective_speed(
            self.sheet.walking_speed,
            self.session.exhaustion,
            &self.session.conditions,
        )
    }

    pub fn is_incapacitated(&self) -> bool {
        rules::is_incapacitated(&self.session.conditions)
    }

    // -- ability overrides -------------------------------------------------

    pub fn open_abilities(&mut self) {
        self.ability_cursor = 0;
        self.mode = Mode::Abilities;
    }

    pub fn move_ability_cursor(&mut self, delta: isize) {
        let n = Ability::ALL.len() as isize;
        self.ability_cursor = ((self.ability_cursor as isize + delta).rem_euclid(n)) as usize;
    }

    fn cursor_ability(&self) -> Ability {
        Ability::ALL[self.ability_cursor.min(Ability::ALL.len() - 1)]
    }

    /// Nudge the score at the cursor. The first nudge seeds the override from
    /// whatever was computed, so you adjust from the current number rather
    /// than from zero.
    pub fn adjust_ability(&mut self, delta: i32) {
        let a = self.cursor_ability();
        let current = self.sheet.score(a);
        self.session.set_ability_override(a.abbrev(), current + delta);
        self.recompute();
    }

    pub fn clear_ability_override(&mut self) {
        let a = self.cursor_ability();
        self.session.clear_ability_override(a.abbrev());
        self.recompute();
    }

    pub fn ability_is_overridden(&self, a: Ability) -> bool {
        self.session.ability_override(a.abbrev()).is_some()
    }

    /// Rebuild the sheet after anything that feeds it changes, and persist.
    fn recompute(&mut self) {
        self.sheet = crate::derive::derive_with(&self.character, &self.session.ability_overrides);
        self.persist();
    }

    // -- limited uses ------------------------------------------------------

    /// Spend one use of whatever the cursor is on. Silently does nothing on a
    /// row that is not a limited resource — most rows are not.
    pub fn spend_use(&mut self) {
        let Some(row) = self.selected_row() else { return };
        let Some(u) = row.uses else { return };
        if self.session.remaining(&u.key, u.max) == 0 {
            return;
        }
        self.session.spend_use(&u.key, u.max, &u.reset);
        self.persist();
    }

    pub fn restore_use(&mut self) {
        let Some(row) = self.selected_row() else { return };
        let Some(u) = row.uses else { return };
        self.session.restore_use(&u.key);
        self.persist();
    }

    /// Uses remaining on a row, for the list renderer.
    pub fn remaining_uses(&self, row: &Row) -> Option<(u32, u32)> {
        let u = row.uses.as_ref()?;
        Some((self.session.remaining(&u.key, u.max), u.max))
    }

    // -- play state --------------------------------------------------------

    pub fn max_hp(&self) -> i32 {
        self.sheet.hp.max.value
    }

    pub fn current_hp(&self) -> i32 {
        self.session.current_hp(self.max_hp())
    }

    /// Written after every mutation rather than on quit: a uConsole running
    /// off two 18650s can lose power mid-session, and re-entering an hour of
    /// combat tracking is worse than a few milliseconds of IO.
    fn persist(&mut self) {
        if let Err(e) = self.session.save(&self.session_path) {
            self.last_error = Some(format!("could not save session: {e}"));
        } else {
            self.last_error = None;
        }
    }

    pub fn start_number(&mut self, target: NumberTarget) {
        self.number_buffer.clear();
        self.mode = Mode::Number(target);
    }

    pub fn push_digit(&mut self, c: char) {
        if c.is_ascii_digit() && self.number_buffer.len() < 4 {
            self.number_buffer.push(c);
        }
    }

    pub fn pop_digit(&mut self) {
        self.number_buffer.pop();
    }

    pub fn commit_number(&mut self) {
        let Mode::Number(target) = self.mode else {
            return;
        };
        let amount: i32 = self.number_buffer.parse().unwrap_or(0);
        let max = self.max_hp();
        match target {
            NumberTarget::Damage => self.session.take_damage(amount, max),
            NumberTarget::Heal => self.session.heal(amount, max),
            NumberTarget::TempHp => self.session.set_temp_hp(amount),
        }
        self.number_buffer.clear();
        self.mode = Mode::List;
        self.persist();
    }

    pub fn open_conditions(&mut self) {
        self.condition_cursor = 0;
        self.mode = Mode::Conditions;
    }

    pub fn move_condition_cursor(&mut self, delta: isize) {
        // One past the conditions is the exhaustion counter.
        let len = CONDITIONS.len() + 1;
        let next = (self.condition_cursor as isize + delta).rem_euclid(len as isize);
        self.condition_cursor = next as usize;
    }

    pub fn on_exhaustion_row(&self) -> bool {
        self.condition_cursor == CONDITIONS.len()
    }

    pub fn toggle_condition_at_cursor(&mut self) {
        if self.on_exhaustion_row() {
            // Toggling a counter is meaningless; step it instead.
            self.session.adjust_exhaustion(1);
        } else {
            let name = CONDITIONS[self.condition_cursor];
            self.session.toggle_condition(name);
        }
        self.persist();
    }

    pub fn adjust_at_cursor(&mut self, delta: i8) {
        if self.on_exhaustion_row() {
            self.session.adjust_exhaustion(delta);
            self.persist();
        }
    }

    pub fn death_save(&mut self, success: bool) {
        if success {
            self.session.succeed_death_save();
        } else {
            self.session.fail_death_save();
        }
        self.persist();
    }

    pub fn is_dying(&self) -> bool {
        self.session.is_dying(self.max_hp())
    }

    pub fn toggle_inspiration(&mut self) {
        self.session.inspiration = !self.session.inspiration;
        self.persist();
    }

    /// "To start a Short Rest, you must have at least 1 Hit Point." The same
    /// gate applies to a Long Rest.
    pub fn can_rest(&self) -> bool {
        rules::can_rest(self.current_hp())
    }

    pub fn rest(&mut self, long: bool) {
        if !self.can_rest() {
            self.last_error = Some("you need at least 1 hit point to rest".into());
            self.mode = Mode::List;
            return;
        }
        if long {
            self.session.long_rest();
            self.mode = Mode::List;
        } else {
            // A short rest is not instantaneous bookkeeping: you spend dice one
            // at a time and decide after each roll, so it gets its own screen.
            self.session.short_rest();
            self.hit_die_cursor = 0;
            self.rest_healed = 0;
            self.mode = Mode::ShortRest;
        }
        self.last_error = None;
        self.persist();
    }

    // -- hit dice ----------------------------------------------------------

    pub fn hit_dice(&self) -> Vec<(String, u32, u32)> {
        self.sheet
            .hit_dice
            .iter()
            .map(|p| {
                let label = p.label();
                let total = p.total.max(0) as u32;
                (label.clone(), self.session.hit_dice_left(&label, total), total)
            })
            .collect()
    }

    pub fn move_hit_die_cursor(&mut self, delta: isize) {
        let n = self.sheet.hit_dice.len() as isize;
        if n == 0 {
            return;
        }
        self.hit_die_cursor = ((self.hit_die_cursor as isize + delta).rem_euclid(n)) as usize;
    }

    /// Spend one die: roll it, add Constitution, heal at least 1.
    pub fn spend_hit_die(&mut self) {
        let Some(pool) = self.sheet.hit_dice.get(self.hit_die_cursor).cloned() else {
            return;
        };
        let label = pool.label();
        let total = pool.total.max(0) as u32;
        if !self.session.spend_hit_die(&label, total) {
            return;
        }

        let con = self.sheet.modifier(Ability::Con);
        let expr = Expr { count: 1, sides: pool.die.max(2) as u32, modifier: con };
        // The breakdown already names the die, so the label must not repeat it.
        let r = dice::roll(&mut self.rng, "Hit die", expr, Advantage::Normal);
        // The healing floors at 1; the logged roll keeps its real total so the
        // log does not quietly disagree with the dice.
        let healed = rules::hit_die_healing(r.kept, con);
        self.record(r);

        self.session.heal(healed, self.max_hp());
        self.rest_healed += healed;
        self.persist();
    }

    pub fn finish_short_rest(&mut self) {
        self.rest_healed = 0;
        self.mode = Mode::List;
    }

    /// Rows for the current tab with the filter applied.
    pub fn rows(&self) -> Vec<Row> {
        let all = rows_for(self.tab, &self.character, &self.sheet);
        if self.filter.is_empty() {
            return all;
        }
        let needle = self.filter.to_lowercase();
        all.into_iter()
            .filter(|r| {
                r.name.to_lowercase().contains(&needle)
                    || r.snippet.to_lowercase().contains(&needle)
                    || r.meta.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn selected(&self) -> usize {
        self.selected[self.tab.index()]
    }

    fn set_selected(&mut self, n: usize) {
        let idx = self.tab.index();
        self.selected[idx] = n;
    }

    pub fn selected_row(&self) -> Option<Row> {
        let rows = self.rows();
        rows.get(self.selected().min(rows.len().saturating_sub(1))).cloned()
    }

    /// Tabs with no list (VITALS, SKILLS) are fixed panes — j/k and / are
    /// meaningless there and must not appear to do something when they do not.
    pub fn is_list_tab(&self) -> bool {
        !matches!(self.tab, Tab::Vitals | Tab::Skills)
    }

    pub fn go_to_tab(&mut self, tab: Tab) {
        if tab != self.tab {
            self.tab = tab;
            // A filter is scoped to the tab you typed it on. Carrying "fire"
            // from SPELLS into GEAR would silently hide most of your kit.
            self.filter.clear();
            self.mode = Mode::List;
            self.clamp();
        }
    }

    pub fn cycle_tab(&mut self, forward: bool) {
        let n = Tab::ALL.len();
        let i = self.tab.index();
        let next = if forward { (i + 1) % n } else { (i + n - 1) % n };
        self.go_to_tab(Tab::ALL[next]);
    }

    pub fn move_selection(&mut self, delta: isize) {
        if !self.is_list_tab() {
            return;
        }
        let len = self.rows().len();
        if len == 0 {
            return;
        }
        let cur = self.selected() as isize;
        let next = (cur + delta).clamp(0, len as isize - 1);
        self.set_selected(next as usize);
    }

    pub fn select_first(&mut self) {
        self.set_selected(0);
    }

    pub fn select_last(&mut self) {
        let len = self.rows().len();
        self.set_selected(len.saturating_sub(1));
    }

    pub fn open_detail(&mut self) {
        if self.is_list_tab() && !self.rows().is_empty() {
            self.mode = Mode::Detail;
            self.detail_scroll = 0;
        }
    }

    pub fn scroll_detail(&mut self, delta: isize) {
        let next = self.detail_scroll as isize + delta;
        self.detail_scroll = next.max(0) as usize;
    }

    pub fn start_filter(&mut self) {
        if self.is_list_tab() {
            self.mode = Mode::Filter;
        }
    }

    pub fn push_filter(&mut self, c: char) {
        self.filter.push(c);
        self.clamp();
    }

    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.clamp();
    }

    /// Esc backs out one layer rather than quitting outright:
    /// overlay -> detail -> filtered list -> unfiltered list -> quit.
    pub fn escape(&mut self) {
        match self.mode {
            Mode::Number(_) => {
                self.number_buffer.clear();
                self.mode = Mode::List;
            }
            Mode::Conditions | Mode::Rest | Mode::RollLog | Mode::Abilities => {
                self.mode = Mode::List
            }
            // Escaping the load screen on a first run would leave an empty
            // sheet with no way back, so it stays put until something loads.
            Mode::Load => {
                if !self.is_empty() {
                    self.load_buffer.clear();
                    self.load_status = LoadStatus::Idle;
                    self.mode = Mode::List;
                }
            }
            Mode::ShortRest => self.finish_short_rest(),
            Mode::Dice => {
                self.dice_buffer.clear();
                self.dice_error = None;
                self.mode = Mode::List;
            }
            Mode::Detail => {
                self.mode = Mode::List;
                self.detail_scroll = 0;
            }
            Mode::Filter => {
                self.filter.clear();
                self.mode = Mode::List;
                self.clamp();
            }
            Mode::List => {
                if self.filter.is_empty() {
                    self.quit = true;
                } else {
                    self.filter.clear();
                    self.clamp();
                }
            }
        }
    }

    /// Keep the selection inside the filtered list.
    fn clamp(&mut self) {
        let len = self.rows().len();
        let max = len.saturating_sub(1);
        if self.selected() > max {
            self.set_selected(max);
        }
    }
}
