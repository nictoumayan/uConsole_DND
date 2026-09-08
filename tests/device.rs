//! The device simulation has to be trustworthy or it is worse than nothing —
//! a harness that says "fits" when it doesn't just moves the discovery to the
//! table.

use vellum::device::{frame, profile_by_name, UCONSOLE, UCONSOLE_3X};

fn lines(n: usize, width: usize) -> String {
    (0..n)
        .map(|i| format!("{:width$}", format!("line{i}"), width = width))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn uconsole_geometry_is_the_real_panel() {
    // 1280x720 with an 8x16 font at 2x -> 16x32 device px per cell.
    assert_eq!(UCONSOLE.cols, 1280 / 16);
    assert_eq!(UCONSOLE.rows, 720 / 32);
    assert_eq!((UCONSOLE.cols, UCONSOLE.rows), (80, 22));
    assert_eq!((UCONSOLE_3X.cols, UCONSOLE_3X.rows), (53, 15));
}

#[test]
fn content_that_fits_reports_one_page_and_no_warnings() {
    let f = frame(&lines(22, 80), &UCONSOLE, 1, false);
    assert_eq!(f.pages, 1);
    assert_eq!(f.total_lines, 22);
    assert!(!f.overflows_width(&UCONSOLE));
    assert!(!f.text.contains('!'), "should not warn when everything fits");
}

#[test]
fn paging_math_covers_every_line_exactly_once() {
    let f = frame(&lines(38, 40), &UCONSOLE, 1, false);
    assert_eq!(f.pages, 2, "38 lines / 22 rows");

    let p1 = frame(&lines(38, 40), &UCONSOLE, 1, false);
    let p2 = frame(&lines(38, 40), &UCONSOLE, 2, false);
    assert!(p1.text.contains("line0") && p1.text.contains("line21"));
    assert!(!p1.text.contains("line22"));
    assert!(p2.text.contains("line22") && p2.text.contains("line37"));
}

#[test]
fn last_page_reports_nothing_below_the_fold() {
    // Regression: this once reported "22 still below" on a final page that
    // had nothing after it, by counting lines-not-on-this-page.
    let last = frame(&lines(38, 40), &UCONSOLE, 2, false);
    assert!(last.text.contains("0 still below"), "got: {}", last.text);
    let first = frame(&lines(38, 40), &UCONSOLE, 1, false);
    assert!(first.text.contains("16 still below"));
}

#[test]
fn page_number_is_clamped_rather_than_panicking() {
    let f = frame(&lines(10, 10), &UCONSOLE, 99, false);
    assert_eq!(f.page, 1);
    assert_eq!(f.pages, 1);
    let f0 = frame(&lines(10, 10), &UCONSOLE, 0, false);
    assert_eq!(f0.page, 1);
}

#[test]
fn overlong_lines_are_clipped_and_flagged() {
    let f = frame(&lines(3, 200), &UCONSOLE, 1, false);
    assert!(f.overflows_width(&UCONSOLE));
    assert!(f.text.contains('»'), "clip marker missing");
    assert!(f.text.contains("clipped"), "no width warning");
}

#[test]
fn every_framed_row_is_exactly_panel_width() {
    // The bezel must not wobble, or you cannot trust what you are looking at.
    let f = frame(&lines(30, 55), &UCONSOLE, 1, false);
    let widths: std::collections::HashSet<usize> = f
        .text
        .lines()
        .filter(|l| l.starts_with('│'))
        .map(|l| l.chars().count())
        .collect();
    assert_eq!(widths.len(), 1, "rows disagree on width: {widths:?}");
    assert_eq!(*widths.iter().next().unwrap(), UCONSOLE.cols + 2);
}

#[test]
fn empty_content_does_not_panic() {
    let f = frame("", &UCONSOLE, 1, false);
    assert_eq!(f.pages, 1);
    assert_eq!(f.widest, 0);
}

#[test]
fn profiles_resolve_by_name() {
    assert_eq!(profile_by_name("uconsole").unwrap().cols, 80);
    assert_eq!(profile_by_name("3x").unwrap().cols, 53);
    assert!(profile_by_name("nonsense").is_none());
}
