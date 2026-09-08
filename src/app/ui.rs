//! Drawing. Budgeted against the uConsole's 22 rows, and degrading sensibly
//! when the host terminal is taller.

use super::state::{App, Mode};
use super::theme;
use crate::content::Tab;
use crate::portrait::Portrait;
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
    let [status, tabs, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_status(f, app, status);
    draw_tabs(f, app, tabs);
    app.page_rows = body.height.saturating_sub(2).max(1) as usize;

    match app.mode {
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
    let hp_ratio = if s.hp.max.value > 0 {
        s.hp.current as f64 / s.hp.max.value as f64
    } else {
        0.0
    };
    let width = 10usize;
    let filled = (hp_ratio.max(0.0) * width as f64).round() as usize;
    let bar = format!("{}{}", "█".repeat(filled.min(width)), "░".repeat(width - filled.min(width)));

    let mut second = vec![
        Span::styled("HP ", theme::dim()),
        Span::styled(
            format!("{}/{}", s.hp.current, s.hp.max.value),
            theme::bright(),
        ),
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
    if s.hp.temporary > 0 {
        second.push(Span::styled(
            format!("  +{} temp", s.hp.temporary),
            theme::bright(),
        ));
    }

    let lines = vec![
        Line::from(vec![
            Span::styled(s.name.to_uppercase(), theme::bright()),
            Span::styled(format!("  {} {}", s.race, s.classes), theme::dim()),
        ]),
        Line::from(second),
    ];
    f.render_widget(Paragraph::new(lines).style(theme::base()), area);
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

    let items: Vec<ListItem> = rows
        .iter()
        .map(|r| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<name_w$}", trunc(&r.name, name_w)), theme::base()),
                Span::styled(format!("  {:<meta_w$}", trunc(&r.meta, meta_w)), theme::dim()),
                Span::styled(format!("  {}", trunc(&r.snippet, snip_w)), theme::base()),
            ]))
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
        Mode::Detail => "esc back   j/k scroll   n/p next·prev row".to_string(),
        Mode::List if app.is_list_tab() => {
            "1-7 tab   j/k move   ↵ detail   / find   q quit".to_string()
        }
        Mode::List => "1-7 tab   q quit".to_string(),
    };

    let right = if app.is_list_tab() && app.mode != Mode::Detail {
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
                if app.mode == Mode::Filter { theme::bright() } else { theme::dim() },
            ),
            Span::raw(" ".repeat(gap)),
            Span::styled(right, theme::dim()),
        ]))
        .style(theme::base()),
        area,
    );
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
