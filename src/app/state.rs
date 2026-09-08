//! Interaction state, deliberately free of ratatui and of any terminal.
//!
//! Every keypress is a method on `App`. That means the whole interaction model
//! — tab switching, selection memory, filtering, the detail overlay — is
//! testable headlessly, which matters when the target hardware is in the post.

use crate::content::{rows_for, Row, Tab};
use crate::ddb::Character;
use crate::derive::Sheet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Browsing the list on the current tab.
    List,
    /// A row is open full-screen with its full rules text.
    Detail,
    /// Typing into the filter box; keys are text, not commands.
    Filter,
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
}

impl App {
    pub fn new(sheet: Sheet, character: Character) -> App {
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
        }
    }

    /// Rows for the current tab with the filter applied.
    pub fn rows(&self) -> Vec<Row> {
        let all = rows_for(self.tab, &self.character, self.sheet.total_level);
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
    /// detail -> filtered list -> unfiltered list -> quit.
    pub fn escape(&mut self) {
        match self.mode {
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
