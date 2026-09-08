//! Drawing. Budgeted against the uConsole's 22 rows, and degrading sensibly
//! when the host terminal is taller.

use super::state::{App, Mode};
use super::theme;
use crate::content::Tab;
use crate::portrait::Portrait;
use crate::session::CONDITIONS;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &mut App, portrait: Option<&Portrait>) {
    let area = f.area();
    f.render_widget(Block::default().style(theme::base()), area);

    // 2 status, 1 tab strip, content, 1 footer. The rules between them are
    // borders on the content block, so they cost nothing extra.
    // The status bar grows a third row only when there is something on it —
    // conditions or death saves. Twenty-two rows is too few to reserve space
    // for a line that is usually blank.
    let mut status_h = 2;
    if !app.session.condition_summary().is_empty() || app.is_dying() {
        status_h += 1;
    }
    if app.last_roll().is_some() {
        status_h += 1;
    }
    let [status, tabs, body, footer] = Layout::vertical([
        Constraint::Length(status_h),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_status(f, app, status);
    draw_tabs(f, app, tabs);
    app.page_rows = body.height.saturating_sub(2).max(1) as usize;

    match app.mode {
        Mode::RollLog => draw_roll_log(f, app, body),
        Mode::Abilities => draw_abilities(f, app, body),
        Mode::Conditions => draw_conditions(f, app, body),
        Mode::Rest => draw_rest(f, body),
        Mode::Detail => draw_detail(f, app, body),
        _ => match app.tab {
            Tab::Vitals => draw_vitals(f, app, body, portrait),
            Tab::Skills => draw_skills(f, app, body),
            _ => draw_list(f, app, body),
        },
    }
    draw_footer(f, app, footer);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.sheet;
    let max = app.max_hp();
    let cur = app.current_hp();
    let hp_ratio = if max > 0 { cur as f64 / max as f64 } else { 0.0 };
    let width = 10usize;
    let filled = (hp_ratio.max(0.0) * width as f64).round() as usize;
    let bar = format!("{}{}", "█".repeat(filled.min(width)), "░".repeat(width - filled.min(width)));

    // At zero the HP figure is the most important thing on the screen.
    let hp_style = if cur == 0 { theme::danger() } else { theme::bright() };
    let mut second = vec![
        Span::styled("HP ", theme::dim()),
        Span::styled(format!("{cur}/{max}"), hp_style),
        Span::styled(format!(" {bar}  "), theme::base()),
        Span::styled("AC ", theme::dim()),
        Span::styled(s.armor_class.value.to_string(), theme::bright()),
        Span::styled("  INIT ", theme::dim()),
        Span::styled(format!("{:+}", s.initiative.value), theme::base()),
        Span::styled("  PB ", theme::dim()),
        Span::styled(format!("{:+}", s.proficiency_bonus), theme::base()),
        Span::styled("  PP ", theme::dim()),
        Span::styled(s.passive_perception.to_string(), theme::base()),
    ];
    if app.session.temporary_hp > 0 {
        second.push(Span::styled(
            format!("  +{} temp", app.session.temporary_hp),
            theme::bright(),
        ));
    }
    if app.session.inspiration {
        second.push(Span::styled("  ★", theme::bright()));
    }

    let mut first = vec![
        Span::styled(s.name.to_uppercase(), theme::bright()),
        Span::styled(format!("  {} {}", s.race, s.classes), theme::dim()),
    ];
    if app.last_error.is_some() {
        first.push(Span::styled("  [session not saved]", theme::danger()));
    }

    let mut lines = vec![Line::from(first), Line::from(second)];

    // The most recent roll stays on screen. At a table you read the result
    // out, get asked "with what modifier?", and read it out again.
    if let Some(r) = app.last_roll() {
        let mut spans = vec![
            Span::styled(format!("{} ", r.label), theme::dim()),
            Span::styled(r.breakdown(), theme::base()),
        ];
        if r.advantage != crate::dice::Advantage::Normal {
            spans.push(Span::styled(format!(" ({})", r.advantage.label()), theme::dim()));
        }
        if r.is_nat20() {
            spans.push(Span::styled("  NAT 20", theme::crit()));
        } else if r.is_nat1() {
            spans.push(Span::styled("  NAT 1", theme::danger()));
        }
        lines.push(Line::from(spans));
    }

    // Conditions, and death saves when they are live.
    if lines.len() < area.height as usize {
        let mut third: Vec<Span> = Vec::new();
        if app.session.is_dead() {
            third.push(Span::styled("DEAD", theme::danger()));
        } else if app.is_dying() {
            third.push(Span::styled("DYING  ", theme::danger()));
            third.push(Span::styled(
                format!(
                    "saves {} / fails {}",
                    pips(app.session.death_successes),
                    pips(app.session.death_failures)
                ),
                theme::bright(),
            ));
            if app.session.is_stable() {
                third.push(Span::styled("  STABLE", theme::bright()));
            }
        }
        let conditions = app.session.condition_summary();
        if !conditions.is_empty() {
            if !third.is_empty() {
                third.push(Span::styled("   ", theme::base()));
            }
            third.push(Span::styled(conditions, theme::danger()));
        }
        lines.push(Line::from(third));
    }

    f.render_widget(Paragraph::new(lines).style(theme::base()), area);
}

/// Death saves read faster as filled circles than as a number.
fn pips(n: u8) -> String {
    format!("{}{}", "●".repeat(n as usize), "○".repeat(3usize.saturating_sub(n as usize)))
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    for t in Tab::ALL {
        let active = t == app.tab;
        spans.push(Span::styled(
            format!(" {} {} ", t.key(), t.label()),
            if active { theme::tab_active() } else { theme::dim() },
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)).style(theme::base()), area);
}

fn content_block() -> Block<'static> {
    Block::bordered()
        .border_style(theme::dim())
        .style(theme::base())
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = app.rows();
    let block = content_block();
    let inner = block.inner(area);
    f.render_widget(block, area);

    if rows.is_empty() {
        let msg = if app.filter.is_empty() {
            "nothing here for this character".to_string()
        } else {
            format!("no match for {:?}", app.filter)
        };
        f.render_widget(Paragraph::new(msg).style(theme::dim()), inner);
        return;
    }

    let name_w = 26usize;
    let meta_w = 14usize;
    let snip_w = (inner.width as usize).saturating_sub(name_w + meta_w + 4);

    // Rows with limited uses get a pip column. It only appears when the tab
    // actually has any, so nothing pays for it that does not use it.
    let use_w = if rows.iter().any(|r| r.uses.is_some()) { 8 } else { 0 };
    let snip_w = snip_w.saturating_sub(use_w);

    let items: Vec<ListItem> = rows
        .iter()
        .map(|r| {
            let mut spans = vec![
                Span::styled(format!("{:<name_w$}", trunc(&r.name, name_w)), theme::base()),
                Span::styled(format!("  {:<meta_w$}", trunc(&r.meta, meta_w)), theme::dim()),
            ];
            if use_w > 0 {
                let (text, style) = match app.remaining_uses(r) {
                    Some((left, max)) => (
                        use_pips(left, max),
                        if left == 0 { theme::danger() } else { theme::bright() },
                    ),
                    None => (String::new(), theme::dim()),
                };
                spans.push(Span::styled(format!("  {text:<6}"), style));
            }
            spans.push(Span::styled(format!("  {}", trunc(&r.snippet, snip_w)), theme::base()));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.selected().min(rows.len() - 1)));
    f.render_stateful_widget(
        List::new(items).highlight_style(theme::selection()),
        inner,
        &mut state,
    );
}

fn draw_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(row) = app.selected_row() else {
        app.mode = Mode::List;
        return;
    };
    let block = content_block().title(Line::from(vec![
        Span::styled(format!(" {} ", row.name.to_uppercase()), theme::bright()),
        Span::styled(
            if row.meta.is_empty() { String::new() } else { format!("{} ", row.meta) },
            theme::dim(),
        ),
    ]));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    // Clamp the scroll so paging past the end cannot blank the pane — a blank
    // pane is indistinguishable from a broken one.
    let wrapped = estimate_wrapped_lines(&row.detail, inner.width as usize);
    let max_scroll = wrapped.saturating_sub(inner.height as usize);
    app.detail_scroll = app.detail_scroll.min(max_scroll);

    f.render_widget(
        Paragraph::new(row.detail.clone())
            .style(theme::base())
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll as u16, 0)),
        inner,
    );
}

fn draw_vitals(f: &mut Frame, app: &App, area: Rect, portrait: Option<&Portrait>) {
    let s = &app.sheet;
    let block = content_block();
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [left, right] = Layout::horizontal([Constraint::Length(22), Constraint::Min(10)])
        .areas(inner);

    // Portrait: one span per cell, foreground the top pixel and background the
    // bottom, so each cell carries two near-square pixels.
    let art: Vec<Line> = match portrait {
        Some(p) => p
            .to_cells(1.6)
            .into_iter()
            .map(|row| {
                Line::from(
                    row.into_iter()
                        .map(|(top, bot)| {
                            Span::styled(
                                "▀",
                                Style::default()
                                    .fg(ratatui::style::Color::Rgb(top.0, top.1, top.2))
                                    .bg(ratatui::style::Color::Rgb(bot.0, bot.1, bot.2)),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect(),
        None => vec![
            Line::styled("  no portrait cached", theme::dim()),
            Line::styled("  run `vellum fetch`", theme::dim()),
        ],
    };
    f.render_widget(Paragraph::new(art), left);

    let mut lines: Vec<Line> = s
        .scores
        .iter()
        .map(|sc| {
            Line::from(vec![
                Span::styled(format!("{}  ", sc.ability.abbrev()), theme::dim()),
                Span::styled(format!("{:>2}  ", sc.score), theme::bright()),
                Span::styled(format!("{:+}", sc.modifier), theme::base()),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::styled("SAVES", theme::dim()));
    let saves: Vec<String> = s
        .saves
        .iter()
        .map(|e| format!("{} {:+}{}", e.name, e.value, if e.proficient { "*" } else { "" }))
        .collect();
    for chunk in saves.chunks(3) {
        lines.push(Line::styled(chunk.join("  "), theme::base()));
    }
    lines.push(Line::from(""));
    for (sense, range) in &s.senses {
        lines.push(Line::styled(format!("{} {} ft", cap(sense), range), theme::base()));
    }
    for n in &s.class_notes {
        lines.push(Line::styled(n.clone(), theme::bright()));
    }
    if !s.advantages.is_empty() {
        lines.push(Line::styled(format!("ADV: {}", s.advantages.join(", ")), theme::dim()));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), right);
}

fn draw_skills(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.sheet;
    let block = content_block();
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut skills: Vec<&crate::derive::Entry> = s.skills.iter().collect();
    skills.sort_by_key(|e| (!(e.proficient || e.expertise), e.name.clone()));

    let cell = |e: &crate::derive::Entry| {
        let mark = if e.expertise { "**" } else if e.proficient { " *" } else { "  " };
        let style = if e.proficient || e.expertise { theme::bright() } else { theme::base() };
        vec![
            Span::styled(format!("{mark} "), theme::dim()),
            Span::styled(format!("{:<18}", e.name), style),
            Span::styled(format!("{:>3}", format!("{:+}", e.value)), style),
        ]
    };

    let mut lines: Vec<Line> = skills
        .chunks(2)
        .map(|pair| {
            let mut spans = cell(pair[0]);
            if let Some(b) = pair.get(1) {
                spans.push(Span::raw("    "));
                spans.extend(cell(b));
            }
            Line::from(spans)
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::styled(
        format!(
            "* proficient   ** expertise            PASSIVE PERCEPTION {}",
            s.passive_perception
        ),
        theme::dim(),
    ));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let left = match app.mode {
        Mode::Filter => format!("/{}_", app.filter),
        Mode::Number(t) => format!("{}: {}_   ↵ apply   esc cancel", t.prompt(), app.number_buffer),
        Mode::Conditions => "j/k move   space toggle   +/- exhaustion   esc close".to_string(),
        Mode::Rest => "rest:  s short   l long   esc cancel".to_string(),
        Mode::Dice => match &app.dice_error {
            Some(e) => format!("roll: {}_   {e}", app.dice_buffer),
            None => format!("roll: {}_   ↵ roll   esc cancel", app.dice_buffer),
        },
        Mode::RollLog => "esc close".to_string(),
        Mode::Abilities => {
            "j/k pick   +/- adjust   0 clear override   esc close".to_string()
        }
        Mode::Detail => "esc back   j/k scroll   n/p next·prev row".to_string(),
        // While dying, the thing you need is the death-save keys, not the
        // navigation you already know.
        Mode::List if app.is_dying() => {
            "s success   f fail   h heal   c conditions   esc".to_string()
        }
        Mode::List if app.tab == Tab::Roll => {
            "↵ roll   a adv   z dis   / find   x dice   l log".to_string()
        }
        Mode::List if app.is_list_tab() => {
            "d dmg  h heal  c cond  r rest  u use  / find  ↵ detail".to_string()
        }
        Mode::List => "d dmg  h heal  t temp  c cond  r rest  o scores  q quit".to_string(),
    };

    let right = if app.is_list_tab() && matches!(app.mode, Mode::List | Mode::Filter) {
        let n = app.rows().len();
        if app.filter.is_empty() {
            format!("{n} items")
        } else {
            format!("{n} matching {:?}", app.filter)
        }
    } else {
        String::new()
    };

    let gap = (area.width as usize).saturating_sub(left.chars().count() + right.chars().count());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                left,
                match app.mode {
                    Mode::Filter | Mode::Number(_) => theme::bright(),
                    Mode::List if app.is_dying() => theme::danger(),
                    _ => theme::dim(),
                },
            ),
            Span::raw(" ".repeat(gap)),
            Span::styled(right, theme::dim()),
        ]))
        .style(theme::base()),
        area,
    );
}

fn draw_conditions(f: &mut Frame, app: &App, area: Rect) {
    let block = content_block().title(Span::styled(" CONDITIONS ", theme::bright()));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for (i, name) in CONDITIONS.iter().enumerate() {
        let on = app.session.has_condition(name);
        let selected = app.condition_cursor == i;
        let style = if selected {
            theme::selection()
        } else if on {
            theme::danger()
        } else {
            theme::base()
        };
        lines.push(Line::styled(
            format!(" [{}] {}", if on { "x" } else { " " }, name),
            style,
        ));
    }
    let selected = app.on_exhaustion_row();
    lines.push(Line::styled(
        format!(
            " Exhaustion  {}  {}",
            app.session.exhaustion,
            "▮".repeat(app.session.exhaustion as usize)
        ),
        if selected {
            theme::selection()
        } else if app.session.exhaustion > 0 {
            theme::danger()
        } else {
            theme::base()
        },
    ));

    // Two columns: fifteen rows will not fit fifteen rows of content plus a
    // border on a 22-row panel.
    let half = lines.len().div_ceil(2);
    let [left, right] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(inner);
    f.render_widget(Paragraph::new(lines[..half].to_vec()), left);
    f.render_widget(Paragraph::new(lines[half..].to_vec()), right);
}

fn draw_roll_log(f: &mut Frame, app: &App, area: Rect) {
    let block = content_block().title(Span::styled(" ROLL LOG ", theme::bright()));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    if app.rolls.is_empty() {
        f.render_widget(
            Paragraph::new("nothing rolled yet").style(theme::dim()),
            inner,
        );
        return;
    }

    let lines: Vec<Line> = app
        .rolls
        .iter()
        .take(inner.height as usize)
        .map(|r| {
            Line::from(vec![
                Span::styled(format!("{:<22}", trunc(&r.label, 22)), theme::base()),
                Span::styled(format!("{:<6}", r.advantage.label()), theme::dim()),
                Span::styled(format!("{:<28}", r.breakdown()), theme::dim()),
                Span::styled(format!("{:>4}", r.total), roll_style(r)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// A natural 20 and a natural 1 are the two results everyone at the table
/// reacts to, so they get their own colour rather than being a number you
/// have to read carefully.
fn roll_style(r: &crate::dice::Roll) -> Style {
    if r.is_nat20() {
        theme::crit()
    } else if r.is_nat1() {
        theme::danger()
    } else {
        theme::bright()
    }
}

fn draw_abilities(f: &mut Frame, app: &App, area: Rect) {
    let block = content_block().title(Span::styled(" ABILITY SCORES ", theme::bright()));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for (i, a) in crate::derive::tables::Ability::ALL.iter().enumerate() {
        let score = app.sheet.score(*a);
        let modifier = app.sheet.modifier(*a);
        let overridden = app.ability_is_overridden(*a);
        let style = if i == app.ability_cursor {
            theme::selection()
        } else if overridden {
            theme::bright()
        } else {
            theme::base()
        };
        lines.push(Line::styled(
            format!(
                " {}  {:>2}  {:+}   {}",
                a.abbrev(),
                score,
                modifier,
                if overridden { "corrected" } else { "" }
            ),
            style,
        ));
    }

    lines.push(Line::from(""));
    lines.push(Line::styled(
        "D&D Beyond applies some increases it does not ship in the character",
        theme::dim(),
    ));
    lines.push(Line::styled(
        "data — 2024 background increases are the known case. Correct the",
        theme::dim(),
    ));
    lines.push(Line::styled(
        "score here and armour class, passive perception, saves, skills and",
        theme::dim(),
    ));
    lines.push(Line::styled(
        "initiative all follow.",
        theme::dim(),
    ));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_rest(f: &mut Frame, area: Rect) {
    let block = content_block().title(Span::styled(" REST ", theme::bright()));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::styled("  s   short rest", theme::base()),
            Line::styled(
                "      restores short-rest uses; hit dice are yours to spend",
                theme::dim(),
            ),
            Line::from(""),
            Line::styled("  l   long rest", theme::base()),
            Line::styled(
                "      full hit points, temp hp cleared, death saves cleared,",
                theme::dim(),
            ),
            Line::styled(
                "      one level of exhaustion removed, all uses restored",
                theme::dim(),
            ),
            Line::from(""),
            Line::styled("  esc cancel", theme::dim()),
        ]),
        inner,
    );
}

/// Pips while they fit; a fraction once they do not. Six charges of Second
/// Wind read fine as circles; twenty do not.
fn use_pips(left: u32, max: u32) -> String {
    if max <= 5 {
        format!(
            "{}{}",
            "●".repeat(left as usize),
            "○".repeat(max.saturating_sub(left) as usize)
        )
    } else {
        format!("{left}/{max}")
    }
}

/// Enough to clamp scrolling; ratatui does the real wrapping when it draws.
fn estimate_wrapped_lines(text: &str, width: usize) -> usize {
    if width == 0 {
        return text.lines().count();
    }
    text.lines()
        .map(|l| (l.chars().count().max(1)).div_ceil(width))
        .sum()
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
