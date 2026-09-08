//! The tabbed layout, budgeted against the uConsole's 22 rows.
//!
//!   row  1-2   status  — name/class, then HP/AC/INIT/PB/PP. Always visible,
//!                        because that is what you glance at mid-combat.
//!   row  3     rule
//!   row  4     tab strip
//!   row  5     rule
//!   rows 6-20  content — 15 rows, the entire working budget
//!   row  21    rule
//!   row  22    keybinds + scroll indicator
//!
//! Fifteen rows is why this is a master-detail design rather than one long
//! scroll: a list of names fits, and the full rules text (already in the
//! payload) opens over the top of it.

use crate::content::{rows_for, Row, Tab};
use crate::ddb::schema::Character;
use crate::derive::Sheet;
use crate::portrait::Portrait;

pub const COLS: usize = 80;
pub const ROWS: usize = 22;
pub const CONTENT_ROWS: usize = 15;

pub struct View<'a> {
    pub sheet: &'a Sheet,
    pub character: &'a Character,
    pub tab: Tab,
    pub scroll: usize,
    pub portrait: Option<&'a Portrait>,
    pub ascii_portrait: bool,
}

pub fn render(v: &View) -> String {
    let mut out: Vec<String> = Vec::with_capacity(ROWS);
    let s = v.sheet;

    // -- status ------------------------------------------------------------
    out.push(clip(&format!("{} · {} {}", s.name.to_uppercase(), s.race, s.classes)));
    out.push(clip(&format!(
        "HP {}/{} {}  AC {}  INIT {:+}  PB {:+}  PP {}{}",
        s.hp.current,
        s.hp.max.value,
        bar(s.hp.current, s.hp.max.value, 10),
        s.armor_class.value,
        s.initiative.value,
        s.proficiency_bonus,
        s.passive_perception,
        if s.hp.temporary > 0 { format!("  +{} temp", s.hp.temporary) } else { String::new() },
    )));
    out.push("─".repeat(COLS));

    // -- tab strip ---------------------------------------------------------
    let strip = Tab::ALL
        .iter()
        .map(|t| {
            if *t == v.tab {
                format!("[{}·{}]", t.key(), t.label())
            } else {
                format!(" {} {} ", t.key(), t.label())
            }
        })
        .collect::<Vec<_>>()
        .join("");
    out.push(clip(&strip));
    out.push("─".repeat(COLS));

    // -- content -----------------------------------------------------------
    let rows = rows_for(v.tab, v.character, s.total_level);
    let total = match v.tab {
        Tab::Vitals => 0,
        Tab::Skills => 0,
        _ => rows.len(),
    };
    let body = match v.tab {
        Tab::Vitals => vitals_pane(v),
        Tab::Skills => skills_pane(s),
        _ => list_pane(&rows, v.scroll),
    };
    for i in 0..CONTENT_ROWS {
        out.push(clip(body.get(i).map(String::as_str).unwrap_or("")));
    }

    // -- footer ------------------------------------------------------------
    out.push("─".repeat(COLS));
    let scroll_hint = if total > CONTENT_ROWS {
        let scroll = v.scroll.min(total.saturating_sub(CONTENT_ROWS));
        format!("{}-{} of {}", scroll + 1, (scroll + CONTENT_ROWS).min(total), total)
    } else if total > 0 {
        format!("{total} items")
    } else {
        String::new()
    };
    let keys = "1-7 tab  j/k move  ↵ detail  / find  q quit";
    let gap = COLS.saturating_sub(keys.chars().count() + scroll_hint.chars().count());
    out.push(clip(&format!("{keys}{}{scroll_hint}", " ".repeat(gap))));

    out.truncate(ROWS);
    while out.len() < ROWS {
        out.push(String::new());
    }
    out.join("\n") + "\n"
}

/// Portrait on the left, the numbers you look up most on the right.
fn vitals_pane(v: &View) -> Vec<String> {
    let s = v.sheet;
    let pw: usize = 20;
    let art: Vec<String> = match v.portrait {
        Some(p) if v.ascii_portrait => p.to_ascii_lines(),
        Some(p) => p.to_amber_lines(1.6),
        None => vec![
            "┌──────────────────┐".into(),
            "│                  │".into(),
            "│   no portrait    │".into(),
            "│  vellum fetch    │".into(),
            "│  caches it       │".into(),
            "│                  │".into(),
            "└──────────────────┘".into(),
        ],
    };

    let mut right: Vec<String> = Vec::new();
    for sc in &s.scores {
        right.push(format!("{}  {:>2}  {:+}", sc.ability.abbrev(), sc.score, sc.modifier));
    }
    right.push(String::new());
    right.push("SAVES".into());
    let saves: Vec<String> = s
        .saves
        .iter()
        .map(|e| format!("{} {:+}{}", e.name, e.value, if e.proficient { "*" } else { "" }))
        .collect();
    for chunk in saves.chunks(3) {
        right.push(chunk.join("  "));
    }

    let mut lines: Vec<String> = Vec::new();
    for i in 0..CONTENT_ROWS {
        let l = art.get(i).cloned().unwrap_or_default();
        // Portrait rows carry ANSI colour, so pad by visible width, not len().
        let visible = if v.ascii_portrait || v.portrait.is_none() {
            l.chars().count()
        } else {
            v.portrait.map(|p| p.cols).unwrap_or(0)
        };
        let pad = pw.saturating_sub(visible);
        let r = right.get(i).cloned().unwrap_or_default();
        lines.push(format!("{l}{}  {r}", " ".repeat(pad)));
    }

    // Senses and class notes fill whatever the two columns did not use.
    let mut extra = Vec::new();
    for (sense, range) in &s.senses {
        extra.push(format!("{} {} ft", cap(sense), range));
    }
    extra.extend(s.class_notes.iter().cloned());
    if !s.advantages.is_empty() {
        extra.push(format!("ADV: {}", s.advantages.join(", ")));
    }
    let start = right.len().max(7) + 1;
    for (i, e) in extra.iter().enumerate() {
        let row = start + i;
        if row < CONTENT_ROWS {
            let l = lines[row].trim_end().to_string();
            let base = if l.is_empty() { " ".repeat(pw + 2) } else { format!("{l}  ") };
            lines[row] = format!("{base}{e}");
        }
    }
    lines
}

fn skills_pane(s: &Sheet) -> Vec<String> {
    let mut skills: Vec<&crate::derive::Entry> = s.skills.iter().collect();
    // Proficient first — the ones you actually roll.
    skills.sort_by_key(|e| (!(e.proficient || e.expertise), e.name.clone()));
    let cell = |e: &crate::derive::Entry| {
        let mark = if e.expertise { "**" } else if e.proficient { " *" } else { "  " };
        format!("{mark} {:<18}{:>3}", e.name, format!("{:+}", e.value))
    };
    let mut lines: Vec<String> = skills
        .chunks(2)
        .map(|p| match p {
            [a, b] => format!("{}    {}", cell(a), cell(b)),
            [a] => cell(a),
            _ => String::new(),
        })
        .collect();
    lines.push(String::new());
    lines.push(format!(
        "* proficient   ** expertise            PASSIVE PERCEPTION {}",
        s.passive_perception
    ));
    lines
}

/// The uniform list view every content tab shares.
fn list_pane(rows: &[Row], scroll: usize) -> Vec<String> {
    if rows.is_empty() {
        return vec![String::new(), "  nothing here for this character".into()];
    }
    let name_w = 26;
    let meta_w = 14;
    // Clamp rather than render an empty pane: scrolling past the end with no
    // cue looks identical to "this tab is broken".
    let max_scroll = rows.len().saturating_sub(CONTENT_ROWS);
    let scroll = scroll.min(max_scroll);
    rows.iter()
        .skip(scroll)
        .take(CONTENT_ROWS)
        .map(|r| {
            let snippet_w = COLS.saturating_sub(name_w + meta_w + 4);
            format!(
                "{:<name_w$}  {:<meta_w$}  {}",
                trunc(&r.name, name_w),
                trunc(&r.meta, meta_w),
                trunc(&r.snippet, snippet_w),
            )
        })
        .collect()
}

/// The detail overlay: full rules text, wrapped, scrollable.
pub fn detail(row: &Row, scroll: usize) -> String {
    let mut out: Vec<String> = Vec::new();
    out.push(clip(&format!("{}  {}", row.name.to_uppercase(), row.meta)));
    out.push("─".repeat(COLS));

    let mut body: Vec<String> = Vec::new();
    for para in row.detail.lines() {
        if para.trim().is_empty() {
            body.push(String::new());
        } else {
            body.extend(wrap(para, COLS - 2));
        }
    }
    let visible = ROWS - 4;
    let scroll = scroll.min(body.len().saturating_sub(visible));
    for i in 0..visible {
        out.push(clip(body.get(scroll + i).map(String::as_str).unwrap_or("")));
    }
    out.push("─".repeat(COLS));
    let more = body.len().saturating_sub(scroll + visible);
    out.push(clip(&format!(
        "esc back  j/k scroll{}",
        if more > 0 { format!("   ▼ {more} more lines") } else { String::new() }
    )));
    out.truncate(ROWS);
    out.join("\n") + "\n"
}

fn bar(cur: i32, max: i32, w: usize) -> String {
    let filled = if max <= 0 { 0 } else { ((cur.max(0) as f64 / max as f64) * w as f64).round() as usize };
    let filled = filled.min(w);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(w - filled))
}

fn clip(s: &str) -> String {
    s.chars().take(COLS).collect()
}

fn trunc(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        return s.to_string();
    }
    let mut t: String = s.chars().take(w.saturating_sub(1)).collect();
    t.push('…');
    t
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}
