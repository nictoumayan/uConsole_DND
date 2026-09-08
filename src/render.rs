//! A plain-text rendering of the sheet.
//!
//! This is scaffolding for phase 2, not a placeholder to throw away: it is the
//! acceptance test for phase 1. Put it next to your live D&D Beyond sheet and
//! every number should match. The layout deliberately previews the eventual
//! ratatui panes at 1280x720 / 80-ish columns.

use crate::derive::Sheet;

/// Inner width. The uConsole's 1280x720 panel with an 8x16 bitmap font at 2x
/// gives exactly 80 columns by 22 rows, so 76 + borders fits with margin.
///
/// Twenty-two rows is the real constraint: this sheet is ~35 lines, which is
/// why phase 2's TUI has to be tabbed rather than one scrolling page.
const W: usize = 76;

fn rule(ch: char) -> String {
    std::iter::repeat_n(ch, W).collect()
}

pub fn render(s: &Sheet) -> String {
    let mut o = String::new();

    o.push_str(&format!("+{}+\n", rule('=')));
    o.push_str(&row(&s.name.to_uppercase()));
    o.push_str(&row(&format!(
        "{} - {}   PB {:+}",
        s.race, s.classes, s.proficiency_bonus
    )));
    o.push_str(&format!("+{}+\n", rule('=')));

    // Vitals bar — always visible in the TUI, so it goes first here too.
    let bar = hp_bar(s.hp.current, s.hp.max.value, 12);
    o.push_str(&row(&format!(
        "HP {bar} {}/{}{}   AC {}   INIT {:+}",
        s.hp.current,
        s.hp.max.value,
        if s.hp.temporary > 0 { format!(" (+{} temp)", s.hp.temporary) } else { String::new() },
        s.armor_class.value,
        s.initiative.value,
    )));
    o.push_str(&row(&format!("    AC = {}", s.armor_class.formula)));
    o.push_str(&row(&format!("    HP = {}", s.hp.max.formula)));
    o.push_str(&format!("+{}+\n", rule('-')));

    // Ability scores
    let scores: Vec<String> = s
        .scores
        .iter()
        .map(|sc| format!("{} {:>2}({:+})", sc.ability.abbrev(), sc.score, sc.modifier))
        .collect();
    o.push_str(&row(&scores.join("  ")));
    o.push_str(&format!("+{}+\n", rule('-')));

    // Saves
    let saves: Vec<String> = s
        .saves
        .iter()
        .map(|e| format!("{} {:+}{}", e.name, e.value, if e.proficient { "*" } else { " " }))
        .collect();
    o.push_str(&row(&format!("SAVES  {}", saves.join(" "))));
    o.push_str(&row(&format!(
        "PASSIVE PERCEPTION {}",
        s.passive_perception
    )));
    o.push_str(&format!("+{}+\n", rule('-')));

    // Skills — two columns, proficient first so the useful ones are at the top.
    o.push_str(&row("SKILLS                              * prof   ** expertise"));
    let mut skills: Vec<&crate::derive::Entry> = s.skills.iter().collect();
    skills.sort_by_key(|e| (!(e.proficient || e.expertise), e.name.clone()));
    for pair in skills.chunks(2) {
        let cell = |e: &crate::derive::Entry| {
            let mark = if e.expertise { "**" } else if e.proficient { "* " } else { "  " };
            format!("{mark}{:<18}{:>3}", e.name, format!("{:+}", e.value))
        };
        let line = match pair {
            [a, b] => format!("{}  {}", cell(a), cell(b)),
            [a] => cell(a),
            _ => String::new(),
        };
        o.push_str(&row(&line));
    }

    if !s.senses.is_empty() || !s.class_notes.is_empty() {
        o.push_str(&format!("+{}+\n", rule('-')));
        for (sense, range) in &s.senses {
            o.push_str(&row(&format!("{} {} ft", title(sense), range)));
        }
        for note in &s.class_notes {
            o.push_str(&row(note));
        }
    }

    if !s.advantages.is_empty() || !s.immunities.is_empty() || !s.resistances.is_empty() {
        o.push_str(&format!("+{}+\n", rule('-')));
        for (label, list) in [
            ("ADVANTAGE", &s.advantages),
            ("IMMUNE", &s.immunities),
            ("RESIST", &s.resistances),
        ] {
            if !list.is_empty() {
                o.push_str(&row(&format!("{label}: {}", list.join(", "))));
            }
        }
    }

    // Conditional modifiers are never folded into a number — they are the ones
    // you have to make a ruling about, so they get their own visible section.
    if !s.conditionals.is_empty() {
        o.push_str(&format!("+{}+\n", rule('-')));
        o.push_str(&row("CONDITIONAL - not included in the numbers above"));
        for c in &s.conditionals {
            o.push_str(&row(&format!("  {}", c.subject)));
            for chunk in wrap(&c.condition, W - 8) {
                o.push_str(&row(&format!("      {chunk}")));
            }
        }
    }

    o.push_str(&format!("+{}+\n", rule('=')));
    o
}

fn row(content: &str) -> String {
    let mut line = String::from("| ");
    let mut width = 0;
    let mut truncated = false;
    for c in content.chars() {
        if width >= W - 2 {
            truncated = true;
            break;
        }
        line.push(c);
        width += 1;
    }
    // Silent truncation hid a clipped ability-score row for one build. Make it
    // visible: a lost character is a wrong sheet.
    if truncated {
        line.pop();
        line.push('>');
    }
    line.push_str(&" ".repeat(W - 1 - width));
    line.push_str("|\n");
    line
}

fn hp_bar(current: i32, max: i32, width: usize) -> String {
    let filled = if max <= 0 {
        0
    } else {
        ((current.max(0) as f64 / max as f64) * width as f64).round() as usize
    };
    let filled = filled.min(width);
    format!(
        "[{}{}]",
        "#".repeat(filled),
        ".".repeat(width - filled)
    )
}

fn title(s: &str) -> String {
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
        if !line.is_empty() && line.len() + 1 + word.len() > width {
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
    out
}
