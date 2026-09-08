//! Render real TUI frames headlessly:
//!   cargo run --example dump_tui -- <snapshot.json> [avatar] [keys...]
//! Keys are literal characters fed to the keymap, e.g. `4` `/` `f i r e`.
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use vellum::app::{keys, ui, App};
use vellum::portrait::Portrait;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let ch: vellum::ddb::Character = serde_json::from_str(&std::fs::read_to_string(&a[0])?)?;
    let portrait = a
        .get(1)
        .filter(|p| !p.is_empty())
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| Portrait::decode(&b, 20, 8).ok());

    let sheet = vellum::derive::derive(&ch);
    let session = vellum::session::Session::seed(
        ch.id, ch.removed_hit_points, ch.temporary_hit_points, ch.inspiration);
    let path = std::env::temp_dir().join("vellum-dump-session.json");
    let mut app = App::new(sheet, ch, session, path);
    for k in a.iter().skip(2) {
        let code = match k.as_str() {
            "ENTER" => KeyCode::Enter,
            "ESC" => KeyCode::Esc,
            "TAB" => KeyCode::Tab,
            "SPACE" => KeyCode::Char(' '),
            "BS" => KeyCode::Backspace,
            s => KeyCode::Char(s.chars().next().unwrap()),
        };
        keys::handle(&mut app, code, KeyModifiers::NONE);
    }

    let mut term = Terminal::new(TestBackend::new(80, 22))?;
    term.draw(|f| ui::draw(f, &mut app, portrait.as_ref()))?;
    let buf = term.backend().buffer().clone();
    for y in 0..22 {
        let line: String = (0..80).map(|x| buf[(x, y)].symbol().to_string()).collect();
        println!("{}", line.trim_end());
    }
    Ok(())
}
