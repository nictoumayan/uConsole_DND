//! Render real TUI frames to coloured HTML fragments, so the amber phosphor
//! and the selection bars survive into a preview.
//!
//!   cargo run --example dump_html -- <snapshot> <avatar> [keys...]
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Color;
use ratatui::Terminal;
use vellum::app::{keys, ui, App};
use vellum::portrait::Portrait;

fn hex(c: Color, fallback: &str) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => fallback.to_string(),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let ch: vellum::ddb::Character = serde_json::from_str(&std::fs::read_to_string(&a[0])?)?;
    let portrait = a
        .get(1)
        .filter(|p| !p.is_empty())
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| Portrait::decode(&b, 24, 12).ok());

    // Load the real session so the preview shows the corrected sheet.
    let session = vellum::session::Session::load_or_seed(
        std::path::Path::new(&a[2]),
        ch.id,
        ch.removed_hit_points,
        ch.temporary_hit_points,
        ch.inspiration,
    );
    let sheet = vellum::derive::derive_with(&ch, &session.ability_overrides);
    let mut app = App::new(sheet, ch, session, std::env::temp_dir().join("preview.json"))
        .with_seed(0xBEEF);

    for k in a.iter().skip(3) {
        let code = match k.as_str() {
            "ENTER" => KeyCode::Enter,
            "ESC" => KeyCode::Esc,
            "SPACE" => KeyCode::Char(' '),
            s => KeyCode::Char(s.chars().next().unwrap()),
        };
        keys::handle(&mut app, code, KeyModifiers::NONE);
    }

    let mut term = Terminal::new(TestBackend::new(80, 22))?;
    term.draw(|f| ui::draw(f, &mut app, portrait.as_ref()))?;
    let buf = term.backend().buffer().clone();

    // Coalesce runs of identical styling into one span, or the page becomes
    // 1,760 elements per frame.
    let mut out = String::new();
    for y in 0..22 {
        let mut run = String::new();
        let mut cur: Option<(String, String)> = None;
        for x in 0..80 {
            let cell = &buf[(x, y)];
            let style = (hex(cell.fg, "#ffb000"), hex(cell.bg, "#140e06"));
            if cur.as_ref() != Some(&style) {
                if let Some((f, b)) = cur.take() {
                    out.push_str(&format!(
                        "<span style=\"color:{f};background:{b}\">{}</span>",
                        esc(&run)
                    ));
                    run.clear();
                }
                cur = Some(style);
            }
            run.push_str(cell.symbol());
        }
        if let Some((f, b)) = cur {
            out.push_str(&format!(
                "<span style=\"color:{f};background:{b}\">{}</span>",
                esc(&run)
            ));
        }
        out.push('\n');
    }
    print!("{out}");
    Ok(())
}
