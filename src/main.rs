//! vellum — an offline D&D Beyond character sheet for the ClockworkPi uConsole.
//!
//!   vellum fetch <id|url>   pull the character once and cache it
//!   vellum show [id]        derive and print the sheet, entirely offline
//!   vellum path             where the snapshot lives
//!
//! Phase 2 replaces `show` with a ratatui TUI. The derive layer underneath it
//! does not change.

use anyhow::{bail, Context, Result};
use vellum::ddb::{fetch, Character};
use vellum::content::Tab;
use vellum::portrait::{fetch_avatar, Portrait};
use vellum::app::{self, App};
use vellum::session::Session;
use vellum::{content, derive, device, paths, render, tabs};

fn main() {
    if let Err(e) = run() {
        eprintln!("vellum: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fetch") => {
            let target = args.get(1).context("usage: vellum fetch <id|url>")?;
            cmd_fetch(target)
        }
        Some("show") => cmd_show(&args[1..]),
        Some("tui") => cmd_tui(args.get(1).map(String::as_str)),
        Some("reset") => cmd_reset(args.get(1).map(String::as_str)),
        Some("path") => {
            println!("{}", paths::ensure_dir()?.display());
            Ok(())
        }
        Some(other) => bail!("unknown command {other:?}\n{USAGE}"),
        // On the device you want the sheet, not a usage screen. Fall back to
        // usage only when there is nothing cached to show.
        None => match default_id() {
            Ok(_) => cmd_tui(None),
            Err(_) => {
                println!("{USAGE}");
                Ok(())
            }
        },
    }
}

const USAGE: &str = "\
usage:
  vellum                      open the sheet (same as `vellum tui`)
  vellum tui [id]             interactive sheet
  vellum fetch <id|url>       pull a character from D&D Beyond and cache it
  vellum show [id] [flags]    render one static screen (no network, scriptable)
  vellum path                 print the data directory
  vellum reset [id]           clear play state (hit points, conditions)

keys:
  1-7 jump to tab   tab/shift-tab cycle   j/k move   enter detail
  / filter          esc back a layer      q quit

show flags:
  --device[=uconsole|3x]   clip to the real panel geometry and draw a bezel.
                           reports anything that does not fit.
  --page N                 which screenful to show (default 1)
  --amber                  amber phosphor colours
  --plain                  no colour (default when not a tty)
  --tab NAME|N             vitals|skills|actions|spells|gear|feats|notes
  --scroll N               scroll offset within the tab
  --detail N               open row N of the tab full-screen
  --ascii                  render the portrait as ASCII instead of half-blocks

the character's privacy must be set to Public on dndbeyond.com for the
anonymous JSON route to serve it.";

fn cmd_fetch(target: &str) -> Result<()> {
    let id = fetch::parse_character_id(target)?;
    eprintln!("fetching {} ...", fetch::character_url(id));

    let raw = fetch::fetch_raw(id)?;

    // Parse before writing, so a broken payload never replaces a good snapshot.
    let parsed: Character =
        serde_json::from_str(&raw).context("D&D Beyond returned something that is not a character")?;
    if parsed.name.is_empty() {
        bail!("payload parsed but has no character name — refusing to cache it");
    }

    paths::ensure_dir()?;
    let path = paths::snapshot_path(id)?;
    std::fs::write(&path, raw.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    std::fs::write(paths::default_id_path()?, id.to_string())?;

    eprintln!(
        "cached {} ({} KB) -> {}",
        parsed.name,
        raw.len() / 1024,
        path.display()
    );

    // Portrait is best-effort: a missing avatar must never fail the import,
    // and `show` falls back to a placeholder.
    if parsed.avatar_url.is_empty() {
        eprintln!("no portrait on this character");
    } else {
        match fetch_avatar(&parsed.avatar_url) {
            Ok(bytes) => {
                let ap = paths::avatar_path(id)?;
                std::fs::write(&ap, &bytes)?;
                eprintln!("cached portrait ({} KB)", bytes.len() / 1024);
            }
            Err(e) => eprintln!("portrait unavailable ({e}) — sheet still works"),
        }
    }
    Ok(())
}

struct ShowOpts {
    id: Option<String>,
    profile: Option<device::Profile>,
    page: usize,
    amber: bool,
    tab: Option<Tab>,
    scroll: usize,
    detail: Option<usize>,
    ascii_portrait: bool,
}

fn parse_show_args(args: &[String]) -> Result<ShowOpts> {
    let mut o = ShowOpts {
        id: None,
        profile: None,
        page: 1,
        amber: false,
        tab: None,
        scroll: 0,
        detail: None,
        ascii_portrait: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--amber" => o.amber = true,
            "--plain" => o.amber = false,
            "--ascii" => o.ascii_portrait = true,
            "--tab" => {
                let n = it.next().context("--tab needs a name or number")?;
                o.tab = Some(Tab::from_name(n).with_context(|| format!("unknown tab {n:?}"))?);
            }
            "--scroll" => {
                o.scroll = it.next().context("--scroll needs a number")?.parse()?;
            }
            "--detail" => {
                o.detail = Some(it.next().context("--detail needs a row number")?.parse()?);
            }
            "--device" => o.profile = Some(device::UCONSOLE),
            "--page" => {
                let n = it.next().context("--page needs a number")?;
                o.page = n.parse().context("--page needs a number")?;
            }
            other if other.starts_with("--device=") => {
                let name = &other["--device=".len()..];
                o.profile = Some(
                    device::profile_by_name(name)
                        .with_context(|| format!("unknown device profile {name:?} (try uconsole, 3x)"))?,
                );
            }
            other if other.starts_with("--page=") => {
                o.page = other["--page=".len()..].parse().context("--page needs a number")?;
            }
            other if other.starts_with("--tab=") => {
                let n = &other["--tab=".len()..];
                o.tab = Some(Tab::from_name(n).with_context(|| format!("unknown tab {n:?}"))?);
            }
            other if other.starts_with("--scroll=") => {
                o.scroll = other["--scroll=".len()..].parse()?;
            }
            other if other.starts_with("--detail=") => {
                o.detail = Some(other["--detail=".len()..].parse()?);
            }
            other if other.starts_with("--") => bail!("unknown flag {other:?}"),
            other => o.id = Some(other.to_string()),
        }
    }
    Ok(o)
}

fn default_id() -> Result<i64> {
    let p = paths::default_id_path()?;
    let s = std::fs::read_to_string(&p)
        .context("no default character — run `vellum fetch <id|url>` first")?;
    s.trim().parse::<i64>().context("corrupt default character id")
}

fn resolve_id(arg: Option<&str>) -> Result<i64> {
    match arg {
        Some(a) => fetch::parse_character_id(a),
        None => default_id(),
    }
}

fn load(id: i64) -> Result<Character> {
    let path = paths::snapshot_path(id)?;
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("no snapshot for {id} — run `vellum fetch {id}`"))?;
    serde_json::from_str(&raw).context("parsing cached snapshot")
}

fn load_portrait(id: i64, cols: usize, rows: usize) -> Option<Portrait> {
    let bytes = paths::avatar_path(id).ok().and_then(|p| std::fs::read(p).ok())?;
    Portrait::decode(&bytes, cols, rows).ok()
}

fn cmd_tui(id_arg: Option<&str>) -> Result<()> {
    let id = resolve_id(id_arg)?;
    let ch = load(id)?;
    let sheet = derive::derive(&ch);
    let portrait = load_portrait(id, 20, 8);

    paths::ensure_dir()?;
    let session_path = paths::session_path(id)?;
    // Seeded from whatever the snapshot last recorded, so a first launch
    // agrees with the website rather than starting you at full health.
    let session = Session::load_or_seed(
        &session_path,
        id,
        ch.removed_hit_points,
        ch.temporary_hit_points,
        ch.inspiration,
    );

    app::run(App::new(sheet, ch, session, session_path), portrait)
}

/// Deletes only the session. The snapshot and the portrait are untouched —
/// this is "start the campaign fresh", not "forget my character".
fn cmd_reset(id_arg: Option<&str>) -> Result<()> {
    let id = resolve_id(id_arg)?;
    let p = paths::session_path(id)?;
    match std::fs::remove_file(&p) {
        Ok(()) => eprintln!("cleared play state for {id}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("no play state for {id} — nothing to clear")
        }
        Err(e) => return Err(e).with_context(|| format!("removing {}", p.display())),
    }
    Ok(())
}

fn cmd_show(args: &[String]) -> Result<()> {
    let opts = parse_show_args(args)?;
    let id = resolve_id(opts.id.as_deref())?;
    let ch: Character = load(id)?;
    let derived = derive::derive(&ch);

    // Tabbed layout when a tab is named; the flat sheet otherwise.
    let sheet = match opts.tab {
        None => render::render(&derived),
        Some(tab) => {
            let rows = content::rows_for(tab, &ch, &derived);
            match opts.detail {
                Some(n) => {
                    let row = rows
                        .get(n.saturating_sub(1))
                        .with_context(|| format!("no row {n} on the {} tab ({} rows)", tab.label(), rows.len()))?;
                    tabs::detail(row, opts.scroll)
                }
                None => {
                    let portrait = load_portrait(id, 20, 7);
                    tabs::render(&tabs::View {
                        sheet: &derived,
                        character: &ch,
                        tab,
                        scroll: opts.scroll,
                        portrait: portrait.as_ref(),
                        ascii_portrait: opts.ascii_portrait,
                    })
                }
            }
        }
    };

    match opts.profile {
        None => print!("{sheet}"),
        Some(profile) => {
            // Warn when the host terminal is too small to honestly show the
            // simulated panel — otherwise the bezel itself gets wrapped and
            // the whole exercise lies to you.
            if let Some((cols, rows)) = device::terminal_size() {
                let need = (profile.cols + 2, profile.rows + 4);
                if cols < need.0 || rows < need.1 {
                    eprintln!(
                        "note: terminal is {cols}x{rows}, need at least {}x{} to draw the {} panel cleanly",
                        need.0, need.1, profile.name
                    );
                }
            }
            let framed = device::frame(&sheet, &profile, opts.page, opts.amber);
            print!("{}", framed.text);
        }
    }
    Ok(())
}
