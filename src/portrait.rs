//! The character portrait, rendered as amber phosphor.
//!
//! A terminal cell can carry two colours: a foreground and a background. Half
//! blocks (U+2580) spend that on two vertical pixels. **Quadrant** blocks
//! spend it on four — a 2x2 subgrid — by picking the glyph whose filled
//! quadrants match which of the four pixels are the brighter of two levels.
//!
//! That doubles horizontal resolution for nothing, and it works here precisely
//! because the image is monochrome: two levels per cell is a real constraint
//! on a colour photo and almost none on an amber one.
//!
//! Colour is deliberately thrown away. A full-colour photo in the middle of an
//! amber phosphor sheet looks like a mistake; luminance mapped onto the amber
//! ramp looks like a CRT.

use anyhow::{Context, Result};

/// Amber at full intensity — the same phosphor the rest of the UI uses.
const AMBER: (u8, u8, u8) = (255, 176, 0);

/// Densest-to-lightest ramp for terminals without truecolour.
const RAMP: &[u8] = b"@%#*+=-:. ";

/// Indexed by a 4-bit mask of which quadrants are the brighter level, in the
/// order top-left, top-right, bottom-left, bottom-right.
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

/// One character cell: its glyph, foreground and background.
pub type Cell = (char, (u8, u8, u8), (u8, u8, u8));

pub struct Portrait {
    pub cols: usize,
    pub rows: usize,
    /// Luminance per half-cell, row-major, `rows * 2` tall.
    lum: Vec<u8>,
}

impl Portrait {
    /// Decode and downsample to a 2x2 subgrid per cell: the sampled image is
    /// `cols * 2` wide by `rows * 2` tall.
    pub fn decode(bytes: &[u8], cols: usize, rows: usize) -> Result<Portrait> {
        let img = image::load_from_memory(bytes).context("decoding portrait image")?;
        let small = img.resize_exact(
            (cols * 2) as u32,
            (rows * 2) as u32,
            // Lanczos over Triangle: at this size every pixel is load-bearing
            // and the sharper kernel keeps edges that Triangle smears.
            image::imageops::FilterType::Lanczos3,
        );
        let gray = small.to_luma8();
        Ok(Portrait { cols, rows, lum: gray.into_raw() })
    }

    fn px_w(&self) -> usize {
        self.cols * 2
    }

    fn at(&self, x: usize, y: usize) -> u8 {
        *self.lum.get(y * self.px_w() + x).unwrap_or(&0)
    }

    /// The four subpixels of one cell, clockwise from top-left.
    fn quad(&self, col: usize, row: usize) -> [u8; 4] {
        let (x, y) = (col * 2, row * 2);
        [
            self.at(x, y),
            self.at(x + 1, y),
            self.at(x, y + 1),
            self.at(x + 1, y + 1),
        ]
    }

    /// One line per cell row, using half-blocks and truecolour. `contrast`
    /// lifts the midtones — a 16x16 face loses a lot without it.
    pub fn to_amber_lines(&self, contrast: f32) -> Vec<String> {
        self.to_cells(contrast)
            .into_iter()
            .map(|row| {
                let mut line = String::new();
                for (glyph, fg, bg) in row {
                    line.push_str(&format!(
                        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m{glyph}",
                        fg.0, fg.1, fg.2, bg.0, bg.1, bg.2
                    ));
                }
                line.push_str("\x1b[0m");
                line
            })
            .collect()
    }

    /// Per-cell glyph plus its foreground and background colour.
    ///
    /// Returned as plain colour tuples rather than styled spans so this module
    /// stays free of any UI dependency — the TUI turns these into spans, the
    /// CLI turns them into ANSI, neither knows about the other.
    pub fn to_cells(&self, contrast: f32) -> Vec<Vec<Cell>> {
        (0..self.rows)
            .map(|row| (0..self.cols).map(|col| self.cell(col, row, contrast)).collect())
            .collect()
    }

    fn cell(&self, col: usize, row: usize, contrast: f32) -> Cell {
        let q = self.quad(col, row);
        let lo = *q.iter().min().unwrap();
        let hi = *q.iter().max().unwrap();

        // A flat cell needs no glyph at all — solid background reads cleaner
        // than a full block whose two colours happen to match.
        if hi.saturating_sub(lo) < 8 {
            let c = amber(hi, contrast);
            return (' ', c, c);
        }

        // Split at the midpoint of the cell's own range rather than a global
        // threshold, so a dark cell keeps its internal detail instead of
        // collapsing to black.
        let mid = (lo as u16 + hi as u16) / 2;
        let mut bits = 0u8;
        for (i, v) in q.iter().enumerate() {
            if *v as u16 > mid {
                bits |= 1 << i;
            }
        }
        (QUADRANTS[bits as usize], amber(hi, contrast), amber(lo, contrast))
    }

    /// ASCII fallback for terminals without truecolour: one character per
    /// cell, averaging the four subpixels.
    pub fn to_ascii_lines(&self) -> Vec<String> {
        (0..self.rows)
            .map(|row| {
                (0..self.cols)
                    .map(|col| {
                        let q = self.quad(col, row);
                        let avg = q.iter().map(|v| *v as u16).sum::<u16>() / 4;
                        RAMP[(avg as usize * (RAMP.len() - 1)) / 255] as char
                    })
                    .collect()
            })
            .collect()
    }
}

/// Map luminance onto the amber ramp, with a gamma lift so a dark portrait
/// does not collapse into an unreadable block.
fn amber(lum: u8, contrast: f32) -> (u8, u8, u8) {
    let t = (lum as f32 / 255.0).powf(1.0 / contrast.max(0.1)).clamp(0.0, 1.0);
    (
        (AMBER.0 as f32 * t) as u8,
        (AMBER.1 as f32 * t) as u8,
        (AMBER.2 as f32 * t) as u8,
    )
}

/// Avatars are on a separate CDN host from the character JSON and are served
/// without auth. Cached at fetch time so `show` never needs the network.
pub fn fetch_avatar(url: &str) -> Result<Vec<u8>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(20))
        .build();
    let resp = agent.get(url).call().context("fetching portrait")?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .context("reading portrait bytes")?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4x4 checkerboard PNG, built in-memory so the test ships no binary.
    fn checkerboard() -> Vec<u8> {
        let mut img = image::GrayImage::new(4, 4);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Luma([if (x + y) % 2 == 0 { 255 } else { 0 }]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_to_a_two_by_two_subgrid_per_cell() {
        let p = Portrait::decode(&checkerboard(), 8, 4).unwrap();
        assert_eq!(p.cols, 8);
        assert_eq!(p.rows, 4);
        assert_eq!(p.lum.len(), 16 * 8, "cols*2 wide by rows*2 tall");
    }

    #[test]
    fn quadrant_glyphs_cover_all_sixteen_masks() {
        assert_eq!(QUADRANTS.len(), 16);
        assert_eq!(QUADRANTS[0b0000], ' ');
        assert_eq!(QUADRANTS[0b1111], '█');
        assert_eq!(QUADRANTS[0b0011], '▀', "top-left + top-right is the upper half");
        assert_eq!(QUADRANTS[0b1100], '▄', "bottom pair is the lower half");
        assert_eq!(QUADRANTS[0b0101], '▌', "left pair is the left half");
    }

    #[test]
    fn a_flat_cell_renders_as_solid_background() {
        // No glyph beats a full block whose two colours happen to match.
        let mut img = image::GrayImage::new(4, 4);
        for p in img.pixels_mut() {
            *p = image::Luma([180]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        let p = Portrait::decode(&out.into_inner(), 2, 2).unwrap();
        for row in p.to_cells(1.0) {
            for (glyph, fg, bg) in row {
                assert_eq!(glyph, ' ');
                assert_eq!(fg, bg);
            }
        }
    }

    #[test]
    fn amber_lines_are_one_per_cell_row() {
        let p = Portrait::decode(&checkerboard(), 8, 4).unwrap();
        assert_eq!(p.to_amber_lines(1.0).len(), 4);
    }

    #[test]
    fn cells_carry_a_glyph_per_cell() {
        let p = Portrait::decode(&checkerboard(), 10, 5).unwrap();
        let cells = p.to_cells(1.0);
        assert_eq!(cells.len(), 5);
        assert!(cells.iter().all(|r| r.len() == 10));
        assert!(
            cells.iter().flatten().all(|(g, _, _)| QUADRANTS.contains(g)),
            "every glyph must come from the quadrant set"
        );
    }

    #[test]
    fn ascii_lines_have_exactly_cols_characters() {
        let p = Portrait::decode(&checkerboard(), 12, 6).unwrap();
        let lines = p.to_ascii_lines();
        assert_eq!(lines.len(), 6);
        for l in &lines {
            assert_eq!(l.chars().count(), 12);
        }
    }

    #[test]
    fn cells_match_the_amber_line_geometry() {
        let p = Portrait::decode(&checkerboard(), 10, 5).unwrap();
        let cells = p.to_cells(1.0);
        assert_eq!(cells.len(), 5);
        assert!(cells.iter().all(|r| r.len() == 10));
    }

    #[test]
    fn amber_maps_black_to_black_and_white_to_full_amber() {
        assert_eq!(amber(0, 1.0), (0, 0, 0));
        assert_eq!(amber(255, 1.0), AMBER);
    }

    #[test]
    fn rejects_bytes_that_are_not_an_image() {
        assert!(Portrait::decode(b"definitely not an image", 8, 4).is_err());
    }
}
