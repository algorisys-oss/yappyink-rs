//! AppKit characters to the shared [`Key`] vocabulary.
//!
//! AppKit reports a key press as a *string*: `charactersIgnoringModifiers`.
//! For letters and digits that is the character itself, which is convenient.
//! For the keys that are not characters it is a code point from the private
//! use area or an ASCII control character, and those are the ones worth naming.
//!
//! What a key then *means* is decided by [`ink_app::keymap`], shared with every
//! backend. This module only does the part that is specific to AppKit, and
//! takes no AppKit types, so it compiles and runs its tests anywhere.

use ink_app::keymap::Key;

/// The code points AppKit uses for keys that are not characters.
///
/// From `NSText.h` and `NSEvent.h`. Spelled out rather than imported so this
/// module stays platform-free; they are part of a published interface and do
/// not change.
pub mod code {
    /// `NSBackspaceCharacter`. Rare: most keyboards send DELETE instead.
    pub const BACKSPACE: char = '\u{0008}';
    /// `NSCarriageReturnCharacter`, what Return actually sends.
    pub const CARRIAGE_RETURN: char = '\u{000D}';
    /// `NSEnterCharacter`, the numeric keypad's Enter.
    pub const ENTER: char = '\u{0003}';
    /// Newline, for completeness; some input paths send it instead of CR.
    pub const NEWLINE: char = '\u{000A}';
    pub const ESCAPE: char = '\u{001B}';
    /// `NSDeleteCharacter`. **This is the key labelled Delete on a Mac**, which
    /// deletes backwards and is Backspace everywhere else. Getting this the
    /// wrong way round would make the main delete key erase the selected
    /// object instead of the last character typed.
    pub const DELETE_BACKWARD: char = '\u{007F}';
    /// `NSDeleteFunctionKey`, forward delete: fn-Delete, or Del on a full
    /// keyboard.
    pub const DELETE_FORWARD: char = '\u{F728}';
}

/// Turns a character from `charactersIgnoringModifiers` into a key.
///
/// Lower-cased, because the shared table is written in lower case so that
/// shift cannot change what a key does.
pub fn translate(character: char) -> Option<Key> {
    match character {
        code::ESCAPE => Some(Key::Escape),
        code::DELETE_BACKWARD | code::BACKSPACE => Some(Key::Backspace),
        code::DELETE_FORWARD => Some(Key::Delete),
        code::CARRIAGE_RETURN | code::ENTER | code::NEWLINE => Some(Key::Enter),
        // Anything else in the private use area is a function key or an arrow
        // we do not bind. Letting them through would turn F5 into whatever
        // character its code point happens to be.
        c if ('\u{F700}'..='\u{F8FF}').contains(&c) => None,
        c if c.is_control() => None,
        c => Some(Key::Char(c.to_ascii_lowercase())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ink_app::{Action, Tool, keymap};

    /// The Mac Delete key deletes *backwards*. If this were mapped to
    /// `Key::Delete` the main delete key would erase the selected object
    /// rather than the last character, which is a bad afternoon for anyone
    /// editing text.
    #[test]
    fn the_mac_delete_key_is_backspace() {
        assert_eq!(translate(code::DELETE_BACKWARD), Some(Key::Backspace));
        assert_eq!(translate(code::DELETE_FORWARD), Some(Key::Delete));

        assert_eq!(
            keymap::editing(translate(code::DELETE_BACKWARD).unwrap()),
            Some(Action::BackspaceText),
            "the Delete key must edit text, not delete the object being edited"
        );
    }

    #[test]
    fn both_return_keys_are_enter() {
        for c in [code::CARRIAGE_RETURN, code::ENTER, code::NEWLINE] {
            assert_eq!(translate(c), Some(Key::Enter));
        }
    }

    #[test]
    fn shift_does_not_change_what_a_key_does() {
        assert_eq!(translate('D'), Some(Key::Char('d')));
        assert_eq!(translate('d'), Some(Key::Char('d')));
        assert_eq!(
            keymap::command(translate('D').unwrap()),
            Some(Action::EnterDraw)
        );
    }

    /// Function keys and arrows live in the private use area. Passed through,
    /// they would arrive as arbitrary characters and could collide with a
    /// binding.
    #[test]
    fn function_keys_are_not_characters() {
        // NSF1FunctionKey, NSUpArrowFunctionKey, NSLeftArrowFunctionKey.
        for c in ['\u{F704}', '\u{F700}', '\u{F702}'] {
            assert_eq!(translate(c), None, "{c:?} leaked through as a character");
        }
    }

    #[test]
    fn a_key_press_reaches_the_right_action() {
        let cases = [
            ('d', Action::EnterDraw),
            ('9', Action::SelectTool(Tool::Text)),
            ('e', Action::SelectTool(Tool::Eraser)),
            ('[', Action::AdjustWidth(-1)),
            ('=', Action::AdjustOpacity(1)),
        ];
        for (c, action) in cases {
            let key = translate(c).expect("should translate");
            assert_eq!(keymap::command(key), Some(action), "for {c:?}");
        }
    }

    #[test]
    fn quitting_works_through_the_shared_table() {
        assert!(keymap::quits(translate('q').unwrap()));
    }
}
