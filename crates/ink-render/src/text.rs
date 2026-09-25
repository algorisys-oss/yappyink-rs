//! Loading a font and measuring text.
//!
//! Deliberately small. This finds one sans-serif face on the system, rasterises
//! glyphs from it, and reports how wide a run is. It does per-glyph fallback but
//! no shaping and no bidirectional layout, so it handles Latin, and for other
//! scripts it gets the right characters on screen without necessarily arranging
//! them correctly. `docs/handoff.md` records shaping as unfinished work.
//!
//! When no font can be found the text tool is unavailable and says so, rather
//! than drawing boxes and pretending.

use std::path::{Path, PathBuf};

use fontdue::{Font, FontSettings};

/// Where a sans-serif face usually lives, on Linux.
///
/// Probed in order. Listing paths rather than calling out to `fc-match` keeps
/// this free of a process spawn and of a dependency on fontconfig being
/// installed; the trade is that an unusual system may have none of these, which
/// is reported rather than guessed around.
const CANDIDATES: [&str; 5] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

/// The same on macOS, where the bundled faces live under `/System`.
///
/// Until 0.7.0 the only macOS entry was `/Library/Fonts/Arial.ttf`, which a
/// stock Mac does not have: the first person to run the macOS build got "no
/// usable font was found" and no text tool (E009). `Supplemental` is where
/// Arial and Verdana ship on every macOS since 10.15.
const MACOS_CANDIDATES: [&str; 4] = [
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Supplemental/Verdana.ttf",
    "/System/Library/Fonts/Supplemental/Tahoma.ttf",
    "/Library/Fonts/Arial.ttf",
];

/// macOS faces with wide coverage, for characters the main face lacks.
const MACOS_FALLBACKS: [&str; 1] = ["/System/Library/Fonts/Supplemental/Arial Unicode.ttf"];

/// Font files under the Windows fonts directory, which is found from `WINDIR`
/// rather than assumed to be on `C:`.
const WINDOWS_CANDIDATES: [&str; 3] = ["segoeui.ttf", "arial.ttf", "tahoma.ttf"];

/// Nirmala UI covers the Indic scripts and Microsoft YaHei the CJK ones.
const WINDOWS_FALLBACKS: [&str; 2] = ["Nirmala.ttf", "msyh.ttc"];

/// The candidates for the operating system this was built for.
fn candidates() -> Vec<PathBuf> {
    for_os(
        std::env::consts::OS,
        &CANDIDATES,
        &MACOS_CANDIDATES,
        &WINDOWS_CANDIDATES,
    )
}

fn fallbacks() -> Vec<PathBuf> {
    for_os(
        std::env::consts::OS,
        &FALLBACKS,
        &MACOS_FALLBACKS,
        &WINDOWS_FALLBACKS,
    )
}

/// Picks the list for an operating system, with the OS passed in so every
/// branch can be tested on any machine.
fn for_os(os: &str, linux: &[&str], macos: &[&str], windows: &[&str]) -> Vec<PathBuf> {
    match os {
        "macos" => macos.iter().map(PathBuf::from).collect(),
        "windows" => {
            let root = std::env::var_os("WINDIR")
                .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
            windows
                .iter()
                .map(|name| root.join("Fonts").join(name))
                .collect()
        }
        _ => linux.iter().map(PathBuf::from).collect(),
    }
}

/// Faces to try when the main one has no glyph for a character.
///
/// This is fallback, not shaping. It puts a character on screen when the primary
/// face has never heard of it, which is the difference between wrong text and no
/// text at all: DejaVuSans, first in `CANDIDATES` and the face this machine
/// picks, has zero Devanagari coverage, so Hindi rasterised to nothing.
///
/// It does not reorder or join anything. Scripts that need shaping still come
/// out as a row of separate base glyphs in visual order, which for Devanagari
/// means matras sit after their consonant instead of around it, and conjuncts do
/// not form. Correct text for those scripts needs a shaping engine.
const FALLBACKS: [&str; 6] = [
    "/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

/// A loaded face, ready to rasterise, with fallbacks behind it.
pub struct TextFont {
    font: Font,
    source: PathBuf,
    fallbacks: Vec<(Font, PathBuf)>,
}

impl TextFont {
    /// Finds and loads a face, or explains why it could not.
    pub fn discover() -> Result<Self, String> {
        let mut tried = Vec::new();
        for candidate in candidates() {
            if !candidate.exists() {
                tried.push(candidate.display().to_string());
                continue;
            }
            return Self::load(&candidate);
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
        let fallbacks = fallbacks()
            .into_iter()
            .filter(|candidate| candidate != path && candidate.exists())
            .filter_map(|candidate| {
                let bytes = std::fs::read(&candidate).ok()?;
                let face = Font::from_bytes(bytes, FontSettings::default()).ok()?;
                Some((face, candidate))
            })
            .collect();

        Ok(Self {
            font,
            source: path.to_path_buf(),
            fallbacks,
        })
    }

    /// The fallback faces that were loaded, for diagnostics.
    pub fn fallbacks(&self) -> Vec<&Path> {
        self.fallbacks.iter().map(|(_, at)| at.as_path()).collect()
    }

    /// The face that has a glyph for this character, preferring the main one.
    ///
    /// `lookup_glyph_index` answers 0 for "not in this face", which is .notdef:
    /// rasterising it gives an empty box or an empty bitmap, which is how text
    /// can be entirely correct and entirely invisible at the same time.
    fn face_for(&self, character: char) -> &Font {
        if self.font.lookup_glyph_index(character) != 0 {
            return &self.font;
        }
        self.fallbacks
            .iter()
            .find(|(face, _)| face.lookup_glyph_index(character) != 0)
            .map_or(&self.font, |(face, _)| face)
    }

    /// Whether any loaded face can draw this character.
    pub fn can_draw(&self, character: char) -> bool {
        character.is_whitespace() || self.face_for(character).lookup_glyph_index(character) != 0
    }

    /// The file this came from, for diagnostics.
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// The width of one line at a given pixel size.
    pub fn line_width(&self, line: &str, pixels: f32) -> f32 {
        line.chars()
            .map(|character| {
                self.face_for(character)
                    .metrics(character, pixels)
                    .advance_width
            })
            .sum()
    }

    /// Rasterises a character to a coverage bitmap.
    ///
    /// Returns the coverage, its dimensions, and where to place it relative to
    /// the pen position and baseline.
    pub fn glyph(&self, character: char, pixels: f32) -> (Vec<u8>, usize, usize, i32, i32, f32) {
        let (metrics, coverage) = self.face_for(character).rasterize(character, pixels);
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

#[cfg(test)]
mod platform_tests {
    use super::*;

    /// The E009 bug: a Mac was only offered a path that a stock Mac lacks.
    /// Every macOS candidate must be a system path, where the OS ships fonts,
    /// not a user-installed location only.
    #[test]
    fn a_mac_is_offered_the_fonts_it_ships_with() {
        let macos = for_os("macos", &CANDIDATES, &MACOS_CANDIDATES, &WINDOWS_CANDIDATES);
        assert!(
            macos
                .first()
                .is_some_and(|path| path.starts_with("/System/Library/Fonts")),
            "{macos:?}"
        );
    }

    #[test]
    fn windows_fonts_are_looked_for_under_the_fonts_directory() {
        let windows = for_os(
            "windows",
            &CANDIDATES,
            &MACOS_CANDIDATES,
            &WINDOWS_CANDIDATES,
        );
        assert_eq!(windows.len(), WINDOWS_CANDIDATES.len());
        for path in &windows {
            assert!(
                path.parent().is_some_and(|dir| dir.ends_with("Fonts")),
                "{path:?}"
            );
        }
    }

    #[test]
    fn linux_keeps_its_own_list() {
        let linux = for_os("linux", &CANDIDATES, &MACOS_CANDIDATES, &WINDOWS_CANDIDATES);
        assert_eq!(linux.first(), Some(&PathBuf::from(CANDIDATES[0])));
    }
}
