//! The global chords, as Carbon's `RegisterEventHotKey` wants them.
//!
//! Free of AppKit and Carbon types like the rest of the decidable parts of
//! this crate, so the numbers below are checked on the development machine.
//! Every one of them is a constant copied from Apple's headers, and a wrong one
//! does not fail: it registers a different chord, or none, silently. That is
//! the reason they are tested at all.
//!
//! See `docs/adr/ADR-007-macos-global-shortcut.md` for why Carbon and not an
//! accessibility grant.

use ink_app::Action;

/// A Mac "OSType": four ASCII characters read as a big-endian number.
pub const fn four_cc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

/// `kEventClassKeyboard`.
pub const EVENT_CLASS_KEYBOARD: u32 = four_cc(b"keyb");
/// `kEventHotKeyPressed`.
pub const EVENT_HOT_KEY_PRESSED: u32 = 5;
/// `kEventParamDirectObject`.
pub const PARAM_DIRECT_OBJECT: u32 = four_cc(b"----");
/// `typeEventHotKeyID`.
pub const TYPE_HOT_KEY_ID: u32 = four_cc(b"hkid");
/// Our own signature, so a hot key event can be recognised as ours.
pub const SIGNATURE: u32 = four_cc(b"yink");

/// `controlKey`, from `Events.h`.
pub const CONTROL: u32 = 1 << 12;
/// `optionKey`.
pub const OPTION: u32 = 1 << 11;

/// Virtual key codes, from `HIToolbox/Events.h`. These are positions on an
/// ANSI keyboard, not characters, so `D` stays where D is on a US layout
/// whatever the active layout calls it. The same is true of Win32's virtual
/// keys, and it is why the chord is described by position in the docs.
pub const KEY_D: u32 = 0x02;
pub const KEY_H: u32 = 0x04;
pub const KEY_Z: u32 = 0x06;
pub const KEY_0: u32 = 0x1D;

/// One chord and what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    /// Carried in the hot key event, so the handler knows which one fired.
    pub id: u32,
    pub key: u32,
    pub modifiers: u32,
    pub name: &'static str,
    pub action: Action,
}

/// Control+Option+D and Control+Option+H: the Windows chords, with Option in
/// the place of Alt.
///
/// Control is not decoration. macOS 15 refuses a hot key whose only modifiers
/// are Option, or Option and Shift, because those produce characters on many
/// layouts. A chord that includes Control or Command is unaffected.
pub const CHORDS: [Chord; 4] = [
    Chord {
        id: 1,
        key: KEY_D,
        modifiers: CONTROL | OPTION,
        name: "Control+Option+D",
        action: Action::ToggleDraw,
    },
    Chord {
        id: 2,
        key: KEY_H,
        modifiers: CONTROL | OPTION,
        name: "Control+Option+H",
        action: Action::ToggleVisibility,
    },
    // Live zoom from anywhere, as on Windows (FR-029). In pass-through the
    // window takes no keys, so these are the only way to zoom there.
    Chord {
        id: 3,
        key: KEY_Z,
        modifiers: CONTROL | OPTION,
        name: "Control+Option+Z",
        action: Action::CycleZoom,
    },
    Chord {
        id: 4,
        key: KEY_0,
        modifiers: CONTROL | OPTION,
        name: "Control+Option+0",
        action: Action::ZoomOff,
    },
];

/// The action for a hot key event's id, if it is one of ours.
pub fn action_for(signature: u32, id: u32) -> Option<Action> {
    if signature != SIGNATURE {
        return None;
    }
    CHORDS
        .iter()
        .find(|chord| chord.id == id)
        .map(|chord| chord.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checked against the hexadecimal Apple documents for each constant, so
    /// a byte-order mistake in `four_cc` cannot hide behind itself.
    #[test]
    fn four_character_codes_match_apples_values() {
        assert_eq!(EVENT_CLASS_KEYBOARD, 0x6B65_7962);
        assert_eq!(PARAM_DIRECT_OBJECT, 0x2D2D_2D2D);
        assert_eq!(TYPE_HOT_KEY_ID, 0x686B_6964);
    }

    #[test]
    fn modifier_masks_match_apples_values() {
        assert_eq!(CONTROL, 0x1000);
        assert_eq!(OPTION, 0x0800);
    }

    /// The macOS 15 rule: without Control or Command the registration is
    /// refused, and the only way back from pass-through goes with it.
    #[test]
    fn every_chord_survives_the_option_only_rule() {
        for chord in CHORDS {
            assert!(
                chord.modifiers & CONTROL != 0,
                "{} lacks Control",
                chord.name
            );
        }
    }

    #[test]
    fn the_chords_are_distinct_and_recognised() {
        for (i, a) in CHORDS.iter().enumerate() {
            for b in &CHORDS[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.key, b.key);
            }
        }
        for chord in CHORDS {
            assert_eq!(action_for(SIGNATURE, chord.id), Some(chord.action));
        }
    }

    /// Another application's hot key, or an id we never registered, is not
    /// ours to act on.
    #[test]
    fn a_foreign_hot_key_is_ignored() {
        assert_eq!(action_for(four_cc(b"othr"), 1), None);
        assert_eq!(action_for(SIGNATURE, 99), None);
    }

    /// Toggling draw is the chord that matters: it is the only way back from
    /// pass-through, where the window takes no input.
    #[test]
    fn there_is_a_way_back_from_pass_through() {
        assert!(CHORDS.iter().any(|c| c.action == Action::ToggleDraw));
    }
}
