//! The character portrait, rendered as amber phosphor.
//!
//! Terminal cells are roughly twice as tall as they are wide, so a cell drawn
//! as U+2580 UPPER HALF BLOCK with the foreground painted as the top pixel and
//! the background as the bottom one gives two near-square pixels per cell. A
//! 16x8 cell portrait is therefore a 16x16 image — low-res on purpose, and it
//! reads as a face at arm's length on a 5" panel.
//!
//! Colour is deliberately thrown away. A full-colour photo in the middle of an
//! amber phosphor sheet looks like a mistake; luminance mapped onto the amber
//! ramp looks like a CRT.

use anyhow::{Context, Result};

/// Amber at full intensity — the same phosphor the rest of the UI uses.
const AMBER: (u8, u8, u8) = (255, 176, 0);

/// Densest-to-lightest ramp for terminals without truecolour.
const RAMP: &[u8] = b"@%#*+=-:. ";

/// One character cell: the RGB of its top pixel and of its bottom pixel.
pub type Cell = ((u8, u8, u8), (u8, u8, u8));

pub struct Portrait {
    pub cols: usize,
    pub rows: usize,
    /// Luminance per half-cell, row-major, `rows * 2` tall.
    lum: Vec<u8>,
}

impl Portrait {
    /// Decode and downsample. `cols` x `rows` are character cells; the sampled
    /// image is `cols` x `rows * 2` pixels.
    pub fn decode(bytes: &[u8], cols: usize, rows: usize) -> Result<Portrait> {
        let img = image::load_from_memory(bytes).context("decoding portrait image")?;
        let target_h = (rows * 2) as u32;
        let small = img.resize_exact(
            cols as u32,
            target_h,
            image::imageops::FilterType::Triangle,
        );
        let gray = small.to_luma8();
        Ok(Portrait { cols, rows, lum: gray.into_raw() })
    }

    fn at(&self, x: usize, y: usize) -> u8 {
        *self.lum.get(y * self.cols + x).unwrap_or(&0)
    }

    /// One line per cell row, using half-blocks and truecolour. `contrast`
    /// lifts the midtones — a 16x16 face loses a lot without it.
    pub fn to_amber_lines(&self, contrast: f32) -> Vec<String> {
        (0..self.rows)
            .map(|row| {
                let mut line = String::new();
                for x in 0..self.cols {
                    let top = amber(self.at(x, row * 2), contrast);
                    let bottom = amber(self.at(x, row * 2 + 1), contrast);
                    line.push_str(&format!(
                        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
                        top.0, top.1, top.2, bottom.0, bottom.1, bottom.2
                    ));
                }
                line.push_str("\x1b[0m");
                line
            })
            .collect()
    }

    /// Per-cell (top, bottom) RGB pairs, one inner Vec per cell row.
    ///
    /// Returned as plain colour tuples rather than styled spans so this module
    /// stays free of any UI dependency — the TUI turns these into spans, the
    /// CLI turns them into ANSI, neither knows about the other.
    pub fn to_cells(&self, contrast: f32) -> Vec<Vec<Cell>> {
        (0..self.rows)
            .map(|row| {
                (0..self.cols)
                    .map(|x| {
                        (
                            amber(self.at(x, row * 2), contrast),
                            amber(self.at(x, row * 2 + 1), contrast),
                        )
                    })
                    .collect()
            })
            .collect()
    }

    /// ASCII fallback. Averages the two half-cells, so it is half the vertical
    /// resolution — but it works on any terminal, and it is the more
    /// authentically retro of the two.
    pub fn to_ascii_lines(&self) -> Vec<String> {
        (0..self.rows)
            .map(|row| {
                (0..self.cols)
                    .map(|x| {
                        let avg =
                            (self.at(x, row * 2) as u16 + self.at(x, row * 2 + 1) as u16) / 2;
                        let idx = (avg as usize * (RAMP.len() - 1)) / 255;
                        RAMP[idx] as char
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
    fn decodes_and_downsamples_to_requested_cells() {
        let p = Portrait::decode(&checkerboard(), 8, 4).unwrap();
        assert_eq!(p.cols, 8);
        assert_eq!(p.rows, 4);
        assert_eq!(p.lum.len(), 8 * 8, "rows*2 pixels tall");
    }

    #[test]
    fn amber_lines_are_one_per_cell_row() {
        let p = Portrait::decode(&checkerboard(), 8, 4).unwrap();
        assert_eq!(p.to_amber_lines(1.0).len(), 4);
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
