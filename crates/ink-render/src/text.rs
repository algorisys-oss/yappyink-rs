//! Loading a font and measuring text.
//!
//! Deliberately small. This finds one sans-serif face on the system, rasterises
//! glyphs from it, and reports how wide a run is. It does no shaping, no
//! bidirectional layout, and no font fallback, which means it handles Latin and
//! little else. Scripts needing shaping or a composition engine arrive with IME
//! support; `docs/handoff.md` records that as the remaining half of T028.
//!
//! When no font can be found the text tool is unavailable and says so, rather
//! than drawing boxes and pretending.

use std::path::{Path, PathBuf};

use fontdue::{Font, FontSettings};

/// Where a sans-serif face usually lives.
///
/// Probed in order. Listing paths rather than calling out to `fc-match` keeps
/// this free of a process spawn and of a dependency on fontconfig being
/// installed; the trade is that an unusual system may have none of these, which
/// is reported rather than guessed around.
const CANDIDATES: [&str; 6] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/Library/Fonts/Arial.ttf",
];

/// A loaded face, ready to rasterise.
pub struct TextFont {
    font: Font,
    source: PathBuf,
}

impl TextFont {
    /// Finds and loads a face, or explains why it could not.
    pub fn discover() -> Result<Self, String> {
        let mut tried = Vec::new();
        for candidate in CANDIDATES {
            let path = Path::new(candidate);
            if !path.exists() {
                tried.push(candidate);
                continue;
            }
            return Self::load(path);
        }
        Err(format!(
            "no sans-serif font was found. Looked in: {}",
            tried.join(", ")
        ))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let font = Font::from_bytes(bytes, FontSettings::default())
            .map_err(|e| format!("{} is not a usable font: {e}", path.display()))?;
        Ok(Self {
            font,
            source: path.to_path_buf(),
        })
    }

    /// The file this came from, for diagnostics.
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// The width of one line at a given pixel size.
    pub fn line_width(&self, line: &str, pixels: f32) -> f32 {
        line.chars()
            .map(|character| self.font.metrics(character, pixels).advance_width)
            .sum()
    }

    /// Rasterises a character to a coverage bitmap.
    ///
    /// Returns the coverage, its dimensions, and where to place it relative to
    /// the pen position and baseline.
    pub fn glyph(&self, character: char, pixels: f32) -> (Vec<u8>, usize, usize, i32, i32, f32) {
        let (metrics, coverage) = self.font.rasterize(character, pixels);
        (
            coverage,
            metrics.width,
            metrics.height,
            metrics.xmin,
            metrics.ymin,
            metrics.advance_width,
        )
    }

    /// Distance from the top of a line to its baseline, at a given size.
    ///
    /// Taken from the face's own metrics so that lines sit where the designer
    /// intended rather than at a guessed fraction of the size.
    pub fn ascent(&self, pixels: f32) -> f32 {
        self.font
            .horizontal_line_metrics(pixels)
            .map_or(pixels * 0.8, |metrics| metrics.ascent)
    }
}
