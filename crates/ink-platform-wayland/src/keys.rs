//! Wayland key presses, in the shared keymap's terms.
//!
//! What a key *means* is decided once, in `ink_app::keymap`, for every
//! backend. This only names the key. Until 0.9.0 the Wayland adapter kept its
//! own list of which keysym did what, a second copy of the keymap that the
//! other backends had stopped keeping, and a key added to the shared one never
//! reached Linux: `z` for zoom did nothing here, on the only platform where
//! zoom works.

use ink_app::keymap::Key;
use smithay_client_toolkit::seat::keyboard::Keysym;

/// The key a press names, or `None` for one the keymap has no use for.
///
/// Characters come from the text the layout produced, lower-cased so Shift
/// does not change what a key does; the keys that produce no character are
/// matched by keysym.
pub fn key_for(keysym: Keysym, text: Option<&str>) -> Option<Key> {
    match keysym {
        Keysym::Escape => Some(Key::Escape),
        Keysym::BackSpace => Some(Key::Backspace),
        Keysym::Delete | Keysym::KP_Delete => Some(Key::Delete),
        Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
        _ => text?
            .chars()
            .next()
            .filter(|character| !character.is_control())
            .map(|character| Key::Char(character.to_ascii_lowercase())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ink_app::Action;
    use ink_app::keymap;

    /// The bug: `z` was bound in the shared keymap and never reached it here.
    #[test]
    fn z_zooms_on_wayland() {
        let key = key_for(Keysym::z, Some("z")).unwrap();
        assert_eq!(keymap::command(key), Some(Action::CycleZoom));
    }

    #[test]
    fn shift_does_not_change_what_a_key_does() {
        assert_eq!(key_for(Keysym::Z, Some("Z")), key_for(Keysym::z, Some("z")));
        assert_eq!(key_for(Keysym::D, Some("D")), Some(Key::Char('d')));
    }

    #[test]
    fn keys_without_text_are_named_by_keysym() {
        assert_eq!(key_for(Keysym::Escape, None), Some(Key::Escape));
        assert_eq!(key_for(Keysym::BackSpace, None), Some(Key::Backspace));
        assert_eq!(key_for(Keysym::Delete, None), Some(Key::Delete));
        assert_eq!(key_for(Keysym::KP_Enter, None), Some(Key::Enter));
    }

    /// Everything the old hand-written table did, the shared keymap now does,
    /// so moving to it lost nothing.
    #[test]
    fn every_key_the_old_table_bound_still_works() {
        let expected = [
            ("d", Action::EnterDraw),
            ("p", Action::ToggleDraw),
            ("h", Action::ToggleVisibility),
            ("g", Action::TogglePark),
            ("w", Action::Save),
            ("o", Action::Load),
            ("t", Action::ShowWindowMenu),
            ("u", Action::Undo),
            ("r", Action::Redo),
            ("x", Action::Clear),
            ("c", Action::CycleColor),
            ("[", Action::AdjustWidth(-1)),
            ("]", Action::AdjustWidth(1)),
            ("-", Action::AdjustOpacity(-1)),
            ("=", Action::AdjustOpacity(1)),
            ("+", Action::AdjustOpacity(1)),
        ];
        for (text, action) in expected {
            let key = key_for(Keysym::NoSymbol, Some(text)).unwrap();
            assert_eq!(keymap::command(key), Some(action), "{text:?}");
        }
        assert!(keymap::quits(key_for(Keysym::q, Some("q")).unwrap()));
        assert_eq!(
            keymap::command(key_for(Keysym::Escape, None).unwrap()),
            Some(Action::Escape)
        );
    }

    #[test]
    fn a_control_character_is_not_a_key() {
        assert_eq!(key_for(Keysym::NoSymbol, Some("\u{1b}")), None);
        assert_eq!(key_for(Keysym::NoSymbol, None), None);
    }
}
