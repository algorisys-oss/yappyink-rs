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

use std::cell::OnceCell;
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
const MACOS_FALLBACKS: [(&str, Script); 1] = [(
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    Script::Any,
)];

/// Font files under the Windows fonts directory, which is found from `WINDIR`
/// rather than assumed to be on `C:`.
const WINDOWS_CANDIDATES: [&str; 3] = ["segoeui.ttf", "arial.ttf", "tahoma.ttf"];

/// Nirmala UI covers the Indic scripts and Microsoft YaHei the CJK ones.
const WINDOWS_FALLBACKS: [(&str, Script); 2] =
    [("Nirmala.ttf", Script::Indic), ("msyh.ttc", Script::Cjk)];

/// The candidates for the operating system this was built for.
fn candidates() -> Vec<PathBuf> {
    for_os(
        std::env::consts::OS,
        &CANDIDATES,
        &MACOS_CANDIDATES,
        &WINDOWS_CANDIDATES,
    )
}

fn fallbacks() -> Vec<(PathBuf, Script)> {
    let os = std::env::consts::OS;
    let table: &[(&str, Script)] = match os {
        "macos" => &MACOS_FALLBACKS,
        "windows" => &WINDOWS_FALLBACKS,
        _ => &FALLBACKS,
    };
    let names: Vec<&str> = table.iter().map(|(name, _)| *name).collect();
    for_os(os, &names, &names, &names)
        .into_iter()
        .zip(table.iter().map(|(_, script)| *script))
        .collect()
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
///
/// Each is tagged with the script it is there for, and loaded only when a
/// character in that script is first drawn. `fontdue` expands every glyph of a
/// face when it loads one, and loading all of these eagerly cost 345 MB and
/// 2.4 s before the window appeared, 329 MB of it the CJK face (E015).
const FALLBACKS: [(&str, Script); 6] = [
    (
        "/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf",
        Script::Indic,
    ),
    (
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        Script::Cjk,
    ),
    (
        "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
        Script::Arabic,
    ),
    (
        "/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf",
        Script::Hebrew,
    ),
    (
        "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf",
        Script::Thai,
    ),
    (
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        Script::Any,
    ),
];

/// What a fallback face is for.
///
/// Decided from Unicode blocks rather than by asking each face, because asking
/// a face means loading it, which is the cost being avoided. `Any` is tried
/// last, for characters no specific face was chosen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    Indic,
    Cjk,
    Arabic,
    Hebrew,
    Thai,
    Any,
}

/// The script a character belongs to, if it is one a fallback exists for.
/// `None` for Latin, Greek, Cyrillic, symbols and everything else the main
/// face is expected to cover.
fn script_of(character: char) -> Option<Script> {
    Some(match u32::from(character) {
        // Devanagari through Sinhala: the Indic blocks.
        0x0900..=0x0DFF => Script::Indic,
        0x0590..=0x05FF | 0xFB1D..=0xFB4F => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Script::Arabic,
        0x0E00..=0x0E7F => Script::Thai,
        // Hangul Jamo, CJK punctuation and kana, the unified ideographs,
        // Hangul syllables, compatibility forms, full-width forms, and the
        // supplementary ideograph planes.
        0x1100..=0x11FF
        | 0x2E80..=0x9FFF
        | 0xAC00..=0xD7AF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFF => Script::Cjk,
        _ => return None,
    })
}

/// A fallback face that is loaded the first time it is needed.
struct Fallback {
    path: PathBuf,
    script: Script,
    /// Empty until needed; `Some(None)` if loading failed, so a broken file is
    /// read once rather than on every character.
    face: OnceCell<Option<Font>>,
}

impl Fallback {
    fn face(&self) -> Option<&Font> {
        self.face
            .get_or_init(|| {
                let bytes = std::fs::read(&self.path).ok()?;
                Font::from_bytes(bytes, FontSettings::default()).ok()
            })
            .as_ref()
    }
}

/// A loaded face, ready to rasterise, with fallbacks behind it.
pub struct TextFont {
    font: Font,
    source: PathBuf,
    fallbacks: Vec<Fallback>,
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
        // Only checked for existence here. Nothing is read until a character
        // needs it (see `FALLBACKS`).
        let fallbacks = fallbacks()
            .into_iter()
            .filter(|(candidate, _)| candidate != path && candidate.exists())
            .map(|(path, script)| Fallback {
                path,
                script,
                face: OnceCell::new(),
            })
            .collect();

        Ok(Self {
            font,
            source: path.to_path_buf(),
            fallbacks,
        })
    }

    /// The fallback faces available, loaded or not, for diagnostics.
    pub fn fallbacks(&self) -> Vec<&Path> {
        self.fallbacks.iter().map(|f| f.path.as_path()).collect()
    }

    /// The fallback faces actually loaded so far.
    pub fn loaded_fallbacks(&self) -> Vec<&Path> {
        self.fallbacks
            .iter()
            .filter(|f| matches!(f.face.get(), Some(Some(_))))
            .map(|f| f.path.as_path())
            .collect()
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
        // The faces for this character's script first, then the general
        // ones. A face for any other script is never loaded.
        let script = script_of(character);
        let specific = self
            .fallbacks
            .iter()
            .filter(|f| f.script != Script::Any && Some(f.script) == script);
        let general = self.fallbacks.iter().filter(|f| f.script == Script::Any);
        specific
            .chain(general)
            .filter_map(Fallback::face)
            .find(|face| face.lookup_glyph_index(character) != 0)
            .unwrap_or(&self.font)
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

    #[test]
    fn characters_are_sorted_into_the_scripts_with_fallbacks() {
        assert_eq!(script_of('a'), None);
        assert_eq!(script_of('é'), None);
        assert_eq!(script_of('Ж'), None);
        assert_eq!(script_of('क'), Some(Script::Indic));
        assert_eq!(script_of('த'), Some(Script::Indic));
        assert_eq!(script_of('中'), Some(Script::Cjk));
        assert_eq!(script_of('あ'), Some(Script::Cjk));
        assert_eq!(script_of('한'), Some(Script::Cjk));
        assert_eq!(script_of('م'), Some(Script::Arabic));
        assert_eq!(script_of('ש'), Some(Script::Hebrew));
        assert_eq!(script_of('ก'), Some(Script::Thai));
    }

    /// E015: a Latin-only session must never pay for the CJK face.
    #[test]
    fn latin_text_loads_no_fallback() {
        let Ok(font) = TextFont::discover() else {
            eprintln!("skipped: no usable font on this machine");
            return;
        };
        assert!(font.loaded_fallbacks().is_empty(), "loaded at startup");
        for character in "Hello, world".chars() {
            let _ = font.can_draw(character);
            let _ = font.line_width(&character.to_string(), 20.0);
        }
        assert!(
            font.loaded_fallbacks().is_empty(),
            "Latin loaded {:?}",
            font.loaded_fallbacks()
        );
    }

    /// And a character in one script loads that script's face and no other.
    #[test]
    fn a_script_loads_only_its_own_face() {
        let Ok(font) = TextFont::discover() else {
            return;
        };
        let named = |needle: &str, paths: &[&Path]| {
            paths.iter().any(|p| p.to_string_lossy().contains(needle))
        };
        if !named("Devanagari", &font.fallbacks()) || !named("CJK", &font.fallbacks()) {
            eprintln!("skipped: this machine lacks the Noto fallbacks");
            return;
        }
        assert!(font.can_draw('क'));
        let loaded = font.loaded_fallbacks();
        assert!(named("Devanagari", &loaded), "{loaded:?}");
        assert!(
            !named("CJK", &loaded),
            "Devanagari text loaded the CJK face: {loaded:?}"
        );
    }
}
