//! Turns the payload into the flat, uniform rows each tab displays.
//!
//! Every tab is a list of `Row`s: a name, a short right-aligned meta chip, a
//! one-line snippet for the list view, and the full text for the detail view.
//! That uniformity is what makes one set of keybindings work everywhere —
//! j/k moves, Enter opens, / filters, regardless of which tab you are on.

use crate::ddb::schema::{Character, SpellEntry};
use crate::derive::Sheet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Vitals,
    Skills,
    Roll,
    Actions,
    Spells,
    Gear,
    Feats,
    Notes,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::Vitals,
        Tab::Skills,
        Tab::Roll,
        Tab::Actions,
        Tab::Spells,
        Tab::Gear,
        Tab::Feats,
        Tab::Notes,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Vitals => "VITALS",
            Tab::Skills => "SKILLS",
            Tab::Roll => "ROLL",
            Tab::Actions => "ACTIONS",
            Tab::Spells => "SPELLS",
            Tab::Gear => "GEAR",
            Tab::Feats => "FEATS",
            Tab::Notes => "NOTES",
        }
    }

    /// The digit that jumps straight here. Direct access beats cycling when
    /// you are three seconds into your turn.
    pub fn key(self) -> char {
        char::from_digit(self.index() as u32 + 1, 10).unwrap_or('?')
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap()
    }

    pub fn from_name(s: &str) -> Option<Tab> {
        let s = s.to_ascii_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|t| t.label().eq_ignore_ascii_case(&s))
            .or_else(|| {
                s.parse::<usize>()
                    .ok()
                    .filter(|n| (1..=Self::ALL.len()).contains(n))
                    .map(|n| Self::ALL[n - 1])
            })
    }
}

/// What a row rolls, when it rolls anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollSpec {
    pub modifier: i32,
    pub kind: RollKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollKind {
    Check,
    Save,
    Initiative,
    /// A raw d20 against DC 10, with its own success/failure bookkeeping.
    DeathSave,
}

#[derive(Debug, Clone, Default)]
pub struct Row {
    pub name: String,
    /// Short right-aligned chip: "Bonus Action", "Lvl 2 Illusion", "x3".
    pub meta: String,
    /// One line for the list view.
    pub snippet: String,
    /// Full text for the detail view. All of it is already in the payload.
    pub detail: String,
    /// Present only on the ROLL tab.
    pub roll: Option<RollSpec>,
}

impl Row {
    fn new(name: impl Into<String>, meta: impl Into<String>, snippet: &str, detail: &str) -> Row {
        let detail = html_to_text(detail);
        let mut short = first_line(&html_to_text(snippet));
        // Several feats and class features ship an empty snippet. Falling back
        // to the first line of the description beats a blank row.
        if short.trim().is_empty() {
            short = first_line(&detail);
        }
        Row { name: name.into(), meta: meta.into(), snippet: short, detail, roll: None }
    }

    fn rollable(
        name: impl Into<String>,
        meta: impl Into<String>,
        snippet: impl Into<String>,
        modifier: i32,
        kind: RollKind,
    ) -> Row {
        Row {
            name: name.into(),
            meta: meta.into(),
            snippet: snippet.into(),
            detail: String::new(),
            roll: Some(RollSpec { modifier, kind }),
        }
    }
}

pub fn rows_for(tab: Tab, ch: &Character, sheet: &Sheet) -> Vec<Row> {
    match tab {
        Tab::Vitals | Tab::Skills => Vec::new(), // rendered as fixed panes
        Tab::Roll => rollables(sheet),
        Tab::Actions => actions(ch),
        Tab::Spells => spells(ch),
        Tab::Gear => gear(ch),
        Tab::Feats => feats(ch, sheet.total_level),
        Tab::Notes => notes(ch),
    }
}

/// Everything you can be asked to roll, in the order a DM asks for it:
/// initiative, then saves, then skills. The list is uniform with every other
/// tab, so `/` filters it — typing "ste" and hitting enter is the fastest
/// path to a Stealth check there is.
fn rollables(sheet: &Sheet) -> Vec<Row> {
    let mut out = vec![Row::rollable(
        "Initiative",
        "initiative",
        format!("{:+}", sheet.initiative.value),
        sheet.initiative.value,
        RollKind::Initiative,
    )];

    for save in &sheet.saves {
        out.push(Row::rollable(
            format!("{} save", save.name),
            if save.proficient { "save · prof" } else { "save" },
            format!("{:+}", save.value),
            save.value,
            RollKind::Save,
        ));
    }

    for skill in &sheet.skills {
        // Must fit the 14-column meta chip; "check · expertise" did not.
        let meta = if skill.expertise {
            "expertise"
        } else if skill.proficient {
            "proficient"
        } else {
            "check"
        };
        out.push(Row::rollable(
            &skill.name,
            meta,
            format!("{:+}", skill.value),
            skill.value,
            RollKind::Check,
        ));
    }

    out.push(Row::rollable(
        "Death save",
        "DC 10",
        "d20, no modifier",
        0,
        RollKind::DeathSave,
    ));
    out
}

fn actions(ch: &Character) -> Vec<Row> {
    let mut out = Vec::new();
    for (src, list) in [
        ("class", &ch.actions.class),
        ("race", &ch.actions.race),
        ("feat", &ch.actions.feat),
    ] {
        let _ = src;
        for a in list {
            let snippet = if a.snippet.is_empty() { &a.description } else { &a.snippet };
            out.push(Row::new(&a.name, a.activation.label(), snippet, &a.description));
        }
    }
    out
}

fn spells(ch: &Character) -> Vec<Row> {
    let mut entries: Vec<&SpellEntry> = Vec::new();
    for b in [&ch.spells.race, &ch.spells.class, &ch.spells.item, &ch.spells.feat] {
        entries.extend(b.iter());
    }
    for cs in &ch.class_spells {
        entries.extend(cs.spells.iter());
    }

    let mut rows: Vec<(i32, Row)> = entries
        .into_iter()
        .map(|e| {
            let d = &e.definition;
            let lvl = if d.level == 0 { "Cantrip".to_string() } else { format!("L{}", d.level) };
            let mut meta = format!("{lvl} {}", school_abbrev(&d.school));
            // Compact markers: "Cantrip Illu C" is exactly the 14 columns the
            // meta chip gets. "·C" pushed it to 15 and truncated the school.
            if d.concentration {
                meta.push_str(" C");
            }
            if d.ritual {
                meta.push_str(" R");
            }
            let snippet = if d.snippet.is_empty() { &d.description } else { &d.snippet };
            (d.level, Row::new(&d.name, meta, snippet, &d.description))
        })
        .collect();

    // Cantrips first, then by level, then alphabetically — the order you scan
    // in when you are looking for something mid-turn.
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    // The same spell can arrive from two buckets — Faerie Fire from an elf
    // lineage and again from an item. It is still one spell you can cast.
    rows.dedup_by(|a, b| a.1.name == b.1.name);
    rows.into_iter().map(|(_, r)| r).collect()
}

/// Four-letter school so the meta column fits in twelve characters.
fn school_abbrev(school: &str) -> &str {
    match school {
        "Abjuration" => "Abju",
        "Conjuration" => "Conj",
        "Divination" => "Divi",
        "Enchantment" => "Ench",
        "Evocation" => "Evoc",
        "Illusion" => "Illu",
        "Necromancy" => "Necr",
        "Transmutation" => "Tran",
        other => other,
    }
}

fn gear(ch: &Character) -> Vec<Row> {
    let mut out: Vec<Row> = ch
        .inventory
        .iter()
        .map(|i| {
            let d = &i.definition;
            let mut meta = String::new();
            if i.equipped {
                meta.push_str("worn ");
            }
            if i.is_attuned {
                meta.push_str("attuned ");
            }
            if i.quantity > 1 {
                meta.push_str(&format!("x{}", i.quantity));
            }
            let sub = if d.rarity.is_empty() || d.rarity == "Common" {
                d.filter_type.clone()
            } else {
                format!("{} · {}", d.filter_type, d.rarity)
            };
            let mut row = Row::new(&d.name, meta.trim(), &sub, &d.description);
            row.snippet = sub;
            row
        })
        .collect();

    for c in &ch.custom_items {
        let mut row = Row::new(
            &c.name,
            c.quantity.filter(|q| *q > 1).map(|q| format!("x{q}")).unwrap_or_default(),
            c.description.as_deref().unwrap_or(""),
            c.description.as_deref().unwrap_or(""),
        );
        if row.snippet.is_empty() {
            row.snippet = "custom item".into();
        }
        if let Some(n) = c.notes.as_deref().filter(|n| !n.trim().is_empty()) {
            row.detail.push_str("\n\n");
            row.detail.push_str(&html_to_text(n));
        }
        out.push(row);
    }
    out
}

fn feats(ch: &Character, level: i32) -> Vec<Row> {
    let mut out = Vec::new();

    for f in &ch.feats {
        let d = &f.definition;
        out.push(Row::new(&d.name, "feat", &d.snippet, &d.description));
    }

    for class in &ch.classes {
        let mut features: Vec<_> = class
            .definition
            .class_features
            .iter()
            // `classFeatures` lists all twenty levels. Showing a level-8 rogue
            // their level-20 capstone is noise.
            .filter(|f| f.required_level <= level)
            .collect();
        features.sort_by_key(|f| (f.required_level, f.display_order));
        for f in features {
            out.push(Row::new(
                &f.name,
                format!("{} {}", class.definition.name, f.required_level),
                &f.description,
                &f.description,
            ));
        }
    }

    for t in &ch.race.racial_traits {
        let d = &t.definition;
        if d.hide_in_sheet || d.name.is_empty() {
            continue;
        }
        out.push(Row::new(&d.name, "racial", &d.snippet, &d.description));
    }

    out
}

fn notes(ch: &Character) -> Vec<Row> {
    let mut out = Vec::new();

    let bg = &ch.background.definition;
    if !bg.name.is_empty() {
        let d = if bg.short_description.is_empty() { &bg.description } else { &bg.short_description };
        out.push(Row::new(format!("Background: {}", bg.name), "", d, d));
    }

    // Sheet order, not payload order: the things you read aloud at a table
    // come before the reference material.
    let sections: [(&str, &Option<String>); 12] = [
        ("Personality", &ch.traits.personality_traits),
        ("Ideals", &ch.traits.ideals),
        ("Bonds", &ch.traits.bonds),
        ("Flaws", &ch.traits.flaws),
        ("Appearance", &ch.traits.appearance),
        ("Backstory", &ch.notes.backstory),
        ("Allies", &ch.notes.allies),
        ("Organizations", &ch.notes.organizations),
        ("Enemies", &ch.notes.enemies),
        ("Other holdings", &ch.notes.other_holdings),
        ("Possessions", &ch.notes.personal_possessions),
        ("Other notes", &ch.notes.other_notes),
    ];
    for (name, text) in sections {
        if let Some(t) = text.as_deref().filter(|t| !t.trim().is_empty()) {
            out.push(Row::new(name, "", t, t));
        }
    }
    out
}

/// Minimal HTML to text. D&D Beyond wraps everything in `<p>`, uses `<em>`
/// and `<strong>` for emphasis and `<ul>/<li>` for lists. A full HTML parser
/// would be a heavy dependency for five tags.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    let mut tag = String::new();

    while let Some(c) = chars.next() {
        match c {
            '<' => {
                tag.clear();
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                    tag.push(t);
                }
                let name = tag.trim_start_matches('/').split_whitespace().next().unwrap_or("");
                match name.to_ascii_lowercase().as_str() {
                    "p" | "div" | "tr" | "h1" | "h2" | "h3" | "h4" => out.push('\n'),
                    "br" => out.push('\n'),
                    "li" => {
                        if !tag.starts_with('/') {
                            out.push_str("\n- ");
                        }
                    }
                    "td" | "th" => out.push(' '),
                    _ => {}
                }
            }
            '&' => {
                let mut ent = String::new();
                while let Some(&n) = chars.peek() {
                    if n == ';' {
                        chars.next();
                        break;
                    }
                    if ent.len() > 8 || n == '<' || n == ' ' {
                        break;
                    }
                    ent.push(n);
                    chars.next();
                }
                out.push_str(entity(&ent));
            }
            _ => out.push(c),
        }
    }

    // Collapse the blank lines all those <p> boundaries produce.
    let mut lines: Vec<String> = out
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    lines.dedup_by(|a, b| a.is_empty() && b.is_empty());
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn entity(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" | "#39" | "rsquo" | "lsquo" => "'",
        "ldquo" | "rdquo" => "\"",
        "nbsp" | "#160" => " ",
        "mdash" | "#8212" => "-",
        "ndash" | "#8211" => "-",
        "hellip" => "...",
        "times" => "x",
        "deg" => "°",
        _ => "",
    }
}

fn first_line(text: &str) -> String {
    text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_string()
}
