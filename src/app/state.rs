//! Interaction state, deliberately free of ratatui and of any terminal.
//!
//! Every keypress is a method on `App`. That means the whole interaction model
//! — tab switching, selection memory, filtering, the detail overlay — is
//! testable headlessly, which matters when the target hardware is in the post.

use crate::content::{rows_for, RollKind, Row, Tab};
use crate::dice::{self, Advantage, Expr, Rng, Roll};
use crate::ddb::Character;
use crate::derive::{tables::Ability, Sheet};
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
    /// Correcting ability scores by hand.
    Abilities,
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
    session_path: PathBuf,
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
}

/// How many rolls the log keeps.
pub const ROLL_LOG_CAP: usize = 40;

impl App {
    pub fn new(sheet: Sheet, character: Character, session: Session, session_path: PathBuf) -> App {
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
        }
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

    /// Roll whatever the cursor is on. Death saves also apply themselves —
    /// rolling one and then having to record it by hand is the kind of
    /// double-entry that gets skipped mid-fight.
    pub fn roll_selected(&mut self, advantage: Advantage) {
        if self.tab != Tab::Roll {
            return;
        }
        let Some(row) = self.selected_row() else { return };
        let Some(spec) = row.roll else { return };

        let result = dice::roll(&mut self.rng, &row.name, Expr::d20(spec.modifier), advantage);

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

    pub fn rest(&mut self, long: bool) {
        if long {
            self.session.long_rest();
        } else {
            self.session.short_rest();
        }
        self.mode = Mode::List;
        self.persist();
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
