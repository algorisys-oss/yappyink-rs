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

#[cfg(test)]
mod tests {
    use super::*;

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
