//! The size the overlay first asks GNOME for.
//!
//! Mutter maximizes a new window that asks for most of the screen
//! (`org.gnome.mutter auto-maximize`, on by default), and a maximized window
//! loses *Always on Top*, resizing and moving (E002 finding 3). The default
//! 1280x720 is most of a 1366x768 laptop screen, so on the owner's machine the
//! overlay opened maximized every time, and the fix was restore, then Always on
//! Top, then resize, on every launch.
//!
//! So the first size is the requested one, shrunk to a share of the smallest
//! output. The smallest, because which output the window will open on is the
//! compositor's choice and cannot be known here. The user can still enlarge it
//! by hand afterwards; auto-maximize only applies when a window first appears.

/// The largest share of each side of an output the first size may take.
///
/// Mutter's cut-off is a share of the work area, which is the output less the
/// top bar and any dock, and cannot be read from the protocol. 80% of each
/// side is 64% of the output and about 70% of a typical work area, clear of
/// it; 85% came within half a point of it on a 1366x768 screen.
pub const FLOATING_SHARE: f64 = 0.8;

/// The first size to ask for, in logical pixels.
///
/// `outputs` are logical sizes. With none known, the request stands.
pub fn floating_size(requested: (u32, u32), outputs: &[(u32, u32)]) -> (u32, u32) {
    let Some(&(width, height)) = outputs
        .iter()
        .filter(|(w, h)| *w > 0 && *h > 0)
        .min_by_key(|(w, h)| u64::from(*w) * u64::from(*h))
    else {
        return requested;
    };
    let cap = |side: u32| ((f64::from(side) * FLOATING_SHARE).floor() as u32).max(1);
    (requested.0.min(cap(width)), requested.1.min(cap(height)))
}

/// The first size when the user chose one last time: theirs, capped by the
/// same [`FLOATING_SHARE`] as the default.
///
/// The cap is not caution; it is the most a plain Wayland client can restore
/// on GNOME. A client chooses its size and never its position. A larger size
/// is auto-maximized, and however it is brought back to floating (asked to
/// float, or grown after it appears), GNOME keeps the top-left corner where it
/// placed a smaller window and the rest runs off the screen. Observed on the
/// owner's 1366x768 screen with 1366x697 remembered. Only the Shell extension
/// can place the window, so only it can restore a full-screen size.
pub fn remembered_size(saved: (u32, u32), outputs: &[(u32, u32)]) -> (u32, u32) {
    floating_size(saved, outputs)
}

/// The remembered size, as the file holds it: `WIDTHxHEIGHT`.
pub fn encode(size: (u32, u32)) -> String {
    format!("{}x{}\n", size.0, size.1)
}

/// Refuses anything implausible, so a damaged file falls back to the default
/// rather than opening a window of no size or of thousands of screens.
pub fn decode(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.trim().split_once('x')?;
    let (w, h): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
    ((200..=16_384).contains(&w) && (120..=16_384).contains(&h)).then_some((w, h))
}

fn path() -> Option<std::path::PathBuf> {
    let session = ink_storage::default_session_path().ok()?;
    Some(session.with_file_name("window-size"))
}

/// The size the user left the overlay at last time, if there is one.
pub fn load() -> Option<(u32, u32)> {
    decode(&std::fs::read_to_string(path()?).ok()?)
}

/// Remembers the size for next time. A failure is reported and otherwise
/// ignored: the next launch simply opens at the default.
pub fn save(size: (u32, u32)) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(error) = std::fs::write(&path, encode(size)) {
        eprintln!("[output] could not remember the window size: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A size the user chose that GNOME can place is restored exactly.
    #[test]
    fn a_smaller_remembered_size_is_kept_as_the_user_left_it() {
        assert_eq!(remembered_size((900, 500), &[(1366, 768)]), (900, 500));
    }

    /// The owner's full-work-area size cannot be restored in place on GNOME,
    /// so it opens at the largest size that can be.
    #[test]
    fn a_full_screen_size_is_capped_to_what_gnome_places_correctly() {
        assert_eq!(remembered_size((1366, 697), &[(1366, 768)]), (1092, 614));
        assert_eq!(remembered_size((1900, 1000), &[(1366, 768)]), (1092, 614));
        assert_eq!(remembered_size((1900, 1000), &[]), (1900, 1000));
    }

    #[test]
    fn the_size_file_round_trips() {
        assert_eq!(decode(&encode((1366, 697))), Some((1366, 697)));
    }

    #[test]
    fn a_damaged_size_file_is_refused() {
        for text in [
            "",
            "1366",
            "x697",
            "0x0",
            "10x10",
            "1366x697x2",
            "wide x tall",
            "99999x697",
        ] {
            assert_eq!(decode(text), None, "{text:?} was accepted");
        }
    }

    /// The owner's screen, where the old default opened maximized.
    #[test]
    fn a_small_laptop_screen_gets_a_window_gnome_will_not_maximize() {
        let (w, h) = floating_size((1280, 720), &[(1366, 768)]);
        assert_eq!((w, h), (1092, 614));
        // Against the work area GNOME measured there, 1366x697, well under
        // the cut-off.
        let share = f64::from(w * h) / f64::from(1366 * 697);
        assert!(share < 0.75, "{share}");
    }

    #[test]
    fn a_large_screen_keeps_the_requested_size() {
        assert_eq!(floating_size((1280, 720), &[(1920, 1080)]), (1280, 720));
        assert_eq!(floating_size((1280, 720), &[(2560, 1440)]), (1280, 720));
    }

    /// The window may open on either output, so the smaller one decides.
    #[test]
    fn the_smallest_output_decides() {
        let size = floating_size((1280, 720), &[(1920, 1080), (1366, 768)]);
        assert_eq!(size, (1092, 614));
    }

    #[test]
    fn with_no_output_known_the_request_stands() {
        assert_eq!(floating_size((1280, 720), &[]), (1280, 720));
        assert_eq!(floating_size((1280, 720), &[(0, 0)]), (1280, 720));
    }
}
