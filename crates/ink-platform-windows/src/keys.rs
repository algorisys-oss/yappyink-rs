//! Virtual-key codes to the shared [`Key`] vocabulary.
//!
//! This module used to hold the binding table itself. It does not any more:
//! the table lives in [`ink_app::keymap`] and is shared with every backend,
//! because three copies of "what does `d` do" is three chances to disagree.
//!
//! What is left is the part only Windows can do — turning a virtual-key code
//! into a key — and it is still free of Windows types, so it compiles and runs
//! its tests on any machine.

use ink_app::keymap::Key;

/// Virtual-key codes, from `WinUser.h`.
///
/// Spelled out rather than imported so this module stays platform-free. The
/// values are part of the Win32 ABI and cannot change.
pub mod vk {
    pub const BACK: u32 = 0x08;
    pub const RETURN: u32 = 0x0D;
    pub const ESCAPE: u32 = 0x1B;
    pub const DELETE: u32 = 0x2E;
    pub const KEY_0: u32 = 0x30;
    pub const KEY_9: u32 = 0x39;
    pub const A: u32 = 0x41;
    pub const D: u32 = 0x44;
    pub const H: u32 = 0x48;
    pub const Q: u32 = 0x51;
    pub const Z: u32 = 0x5A;
    pub const OEM_MINUS: u32 = 0xBD;
    pub const OEM_PLUS: u32 = 0xBB;
    pub const OEM_4: u32 = 0xDB;
    pub const OEM_6: u32 = 0xDD;
}

/// Turns a virtual-key code into a key, or `None` if it is not one we bind.
///
/// Letters and digits are deliberately derived arithmetically rather than
/// listed. Win32 assigns `VK_A`..`VK_Z` the ASCII values of the *upper-case*
/// letters and `VK_0`..`VK_9` those of the digits, so the mapping is subtraction
/// — and a list of twenty-six entries is twenty-six chances to mistype one.
///
/// The result is lower-cased, because the shared table is written in lower case
/// so that shift cannot change what a key does.
///
/// The `OEM_*` codes are the awkward ones: their meaning depends on the
/// keyboard layout, and these four are correct for a US layout. On a layout
/// where `[` is somewhere else, the bracket keys will be wrong. That is a real
/// limitation and it is recorded rather than hidden; the fix is `ToUnicodeEx`,
/// which needs the keyboard state and belongs in the adapter.
pub fn translate(virtual_key: u32) -> Option<Key> {
    match virtual_key {
        vk::ESCAPE => Some(Key::Escape),
        vk::BACK => Some(Key::Backspace),
        vk::RETURN => Some(Key::Enter),
        vk::DELETE => Some(Key::Delete),
        vk::A..=vk::Z => {
            let letter = char::from_u32(virtual_key)?.to_ascii_lowercase();
            Some(Key::Char(letter))
        }
        vk::KEY_0..=vk::KEY_9 => Some(Key::Char(char::from_u32(virtual_key)?)),
        vk::OEM_4 => Some(Key::Char('[')),
        vk::OEM_6 => Some(Key::Char(']')),
        vk::OEM_MINUS => Some(Key::Char('-')),
        vk::OEM_PLUS => Some(Key::Char('=')),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ink_app::{Action, Tool, keymap};

    #[test]
    fn the_named_keys_translate() {
        assert_eq!(translate(vk::ESCAPE), Some(Key::Escape));
        assert_eq!(translate(vk::BACK), Some(Key::Backspace));
        assert_eq!(translate(vk::RETURN), Some(Key::Enter));
        assert_eq!(translate(vk::DELETE), Some(Key::Delete));
    }

    /// The arithmetic, checked at both ends and in the middle rather than
    /// trusted. An off-by-one here would silently shift every letter binding.
    #[test]
    fn every_letter_arrives_lower_cased() {
        for (code, expected) in [(vk::A, 'a'), (vk::D, 'd'), (vk::Q, 'q'), (vk::Z, 'z')] {
            assert_eq!(translate(code), Some(Key::Char(expected)));
        }
        for code in vk::A..=vk::Z {
            let Some(Key::Char(c)) = translate(code) else {
                panic!("virtual key {code:#04x} did not translate to a character");
            };
            assert!(c.is_ascii_lowercase(), "{code:#04x} gave {c:?}");
        }
    }

    #[test]
    fn every_digit_translates_to_itself() {
        for code in vk::KEY_0..=vk::KEY_9 {
            let Some(Key::Char(c)) = translate(code) else {
                panic!("virtual key {code:#04x} did not translate");
            };
            assert!(c.is_ascii_digit(), "{code:#04x} gave {c:?}");
        }
    }

    /// The end-to-end path an actual key press takes, so that a mistake in
    /// either half is caught here rather than on a machine nobody has.
    #[test]
    fn a_key_press_reaches_the_right_action() {
        let cases = [
            (vk::D, Action::EnterDraw),
            (vk::KEY_9, Action::SelectTool(Tool::Text)),
            (vk::OEM_4, Action::AdjustWidth(-1)),
            (vk::OEM_6, Action::AdjustWidth(1)),
            (vk::OEM_MINUS, Action::AdjustOpacity(-1)),
            (vk::OEM_PLUS, Action::AdjustOpacity(1)),
            (vk::DELETE, Action::DeleteSelection),
        ];
        for (code, action) in cases {
            let key = translate(code).expect("the key should translate");
            assert_eq!(keymap::command(key), Some(action), "for {code:#04x}");
        }
    }

    #[test]
    fn quitting_still_works_through_the_shared_table() {
        let key = translate(vk::Q).unwrap();
        assert!(keymap::quits(key));
    }

    #[test]
    fn an_unbound_code_translates_to_nothing() {
        // VK_F13, and VK_LSHIFT, neither of which is a key we act on.
        assert_eq!(translate(0x7C), None);
        assert_eq!(translate(0xA0), None);
    }
}
