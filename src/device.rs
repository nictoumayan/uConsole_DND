//! A local stand-in for the uConsole.
//!
//! The point of this module is to answer one question honestly while the
//! hardware is in transit: *does this actually fit?* Developing a TUI in an
//! 120x50 terminal and discovering at the table that the device gives you 22
//! rows is the failure mode worth designing against.
//!
//! Geometry: the uConsole's panel is 1280x720. At ~294 PPI a 1x bitmap font is
//! unreadable across a table, so the realistic setting is an 8x16 face at 2x,
//! i.e. 16x32 device pixels per cell — exactly 80 columns by 22 rows.

use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub name: &'static str,
    pub cols: usize,
    pub rows: usize,
    /// What the panel is doing to produce that grid, for the status line.
    pub note: &'static str,
}

pub const UCONSOLE: Profile = Profile {
    name: "uConsole",
    cols: 80,
    rows: 22,
    note: "1280x720 @ 8x16 font, 2x",
};

/// The same panel at 3x scaling — bigger text, less of it. Worth checking a
/// layout against both before committing to it.
pub const UCONSOLE_3X: Profile = Profile {
    name: "uConsole 3x",
    cols: 53,
    rows: 15,
    note: "1280x720 @ 8x16 font, 3x",
};

pub fn profile_by_name(name: &str) -> Option<Profile> {
    match name {
        "uconsole" | "2x" => Some(UCONSOLE),
        "uconsole3x" | "3x" => Some(UCONSOLE_3X),
        _ => None,
    }
}

/// Amber phosphor. Chosen over the classic green because it is materially
/// easier on the eyes across a four-hour session, and it matches the
/// uConsole's own orange colourway.
pub struct Palette {
    pub bg: &'static str,
    pub fg: &'static str,
    pub dim: &'static str,
    pub bright: &'static str,
}

pub const AMBER: Palette = Palette {
    bg: "\x1b[48;2;20;14;6m",
    fg: "\x1b[38;2;255;176;0m",
    dim: "\x1b[38;2;150;100;10m",
    bright: "\x1b[38;2;255;220;140m",
};

pub const RESET: &str = "\x1b[0m";

/// Actual terminal geometry, or None when not attached to a tty.
/// Shelling out to `stty` keeps the dependency list at zero — phase 2 gets
/// this from crossterm for free.
pub fn terminal_size() -> Option<(usize, usize)> {
    let out = Command::new("stty")
        .arg("size")
        .stdin(std::process::Stdio::inherit())
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut parts = text.split_whitespace();
    let rows = parts.next()?.parse().ok()?;
    let cols = parts.next()?.parse().ok()?;
    Some((cols, rows))
}

pub struct Framed {
    pub text: String,
    pub total_lines: usize,
    pub page: usize,
    pub pages: usize,
    pub widest: usize,
}

impl Framed {
    pub fn overflows_width(&self, p: &Profile) -> bool {
        self.widest > p.cols
    }
}

/// Clip `content` into the device viewport and draw a bezel around it, so what
/// you see is exactly what the panel will show — no more.
pub fn frame(content: &str, p: &Profile, page: usize, colour: bool) -> Framed {
    let lines: Vec<&str> = content.lines().collect();
    let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let total_lines = lines.len();
    let pages = total_lines.div_ceil(p.rows).max(1);
    let page = page.clamp(1, pages);

    let start = (page - 1) * p.rows;
    let window: Vec<&str> = lines.iter().skip(start).take(p.rows).copied().collect();

    let (bg, fg, dim, bright, reset) = if colour {
        (AMBER.bg, AMBER.fg, AMBER.dim, AMBER.bright, RESET)
    } else {
        ("", "", "", "", "")
    };

    let mut out = String::new();
    let label = format!(
        " {} · {} · {}x{} · page {}/{} ",
        p.name, p.note, p.cols, p.rows, page, pages
    );
    let pad = p.cols.saturating_sub(label.chars().count());
    out.push_str(&format!(
        "{dim}┌{}{}┐{reset}\n",
        label,
        "─".repeat(pad)
    ));

    for row in 0..p.rows {
        let raw = window.get(row).copied().unwrap_or("");
        let mut cell: String = raw.chars().take(p.cols).collect();
        let clipped = raw.chars().count() > p.cols;
        if clipped {
            cell.pop();
            cell.push('»');
        }
        let width = cell.chars().count();
        out.push_str(&format!(
            "{dim}│{reset}{bg}{fg}{cell}{}{reset}{dim}│{reset}\n",
            " ".repeat(p.cols - width)
        ));
    }

    out.push_str(&format!("{dim}└{}┘{reset}\n", "─".repeat(p.cols)));

    // The honest part. Anything the panel cannot show is reported, not hidden.
    let mut notes = Vec::new();
    if pages > 1 {
        // Lines still below the fold *after* this page — not "lines not on
        // this page", which reported 22 hidden on a last page that had none.
        let below = total_lines.saturating_sub(start + window.len());
        notes.push(format!(
            "{total_lines} lines of content, {} rows of panel — {pages} screens, {below} still below",
            p.rows
        ));
    }
    if widest > p.cols {
        notes.push(format!(
            "widest line is {widest} cols, panel is {} — clipped, marked »",
            p.cols
        ));
    }
    for n in notes {
        out.push_str(&format!("{bright}!{reset} {n}\n"));
    }

    Framed { text: out, total_lines, page, pages, widest }
}
