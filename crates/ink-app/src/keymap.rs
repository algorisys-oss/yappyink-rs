//! What each key means, for every backend.
//!
//! The three platforms describe a key press three different ways: Wayland hands
//! over an X keysym, Win32 a virtual-key code, AppKit a string of characters.
//! None of those is the interesting part. What a key *means* is this
//! application's decision, and it must be the same decision everywhere, or a
//! user who moves between platforms has been given two products.
//!
//! So each adapter translates its native representation into [`Key`] — a small
//! job, close to the platform, that it is the only one able to do — and the
//! table below decides the rest. It lives in `ink-app` because an action is
//! `ink-app`'s vocabulary, and it takes no platform types, so it is tested like
//! anything else.
//!
//! Adding a binding here adds it to every backend at once. That is the point.

use crate::{Action, Tool};

/// A key press, with the platform's spelling removed.
///
/// Only the distinctions the bindings actually rest on. A key that produces a
/// character is [`Key::Char`]; the four that cannot are named, because the
/// text editor has to treat them differently from typed text and cannot do so
/// by looking at a character that does not exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Lower-cased by the adapter, so shift does not change what a key does.
    Char(char),
    Escape,
    Backspace,
    Enter,
    Delete,
}

impl Key {
    /// The character this key produces, if it produces one.
    pub fn as_char(self) -> Option<char> {
        match self {
            Key::Char(c) => Some(c),
            _ => None,
        }
    }
}

/// What a key means while the text editor is open.
///
/// FR-023: the editor owns the keyboard, so a key is a character rather than a
/// shortcut. Only the keys that cannot be characters keep a meaning; everything
/// else is typed. Without this, typing "quit" into a text box would quit.
///
/// `Delete` is absent deliberately. Forward-delete inside a text run is not
/// implemented, and mapping it to `DeleteSelection` would delete the object
/// being edited, which is a long way from what the key says it does.
pub fn editing(key: Key) -> Option<Action> {
    match key {
        Key::Escape => Some(Action::Escape),
        Key::Backspace => Some(Action::BackspaceText),
        Key::Enter => Some(Action::NewlineText),
        Key::Char(_) | Key::Delete => None,
    }
}

/// What a key means the rest of the time.
///
/// Quitting is not here, because it is not something the controller can be
/// asked to do; see [`quits`].
pub fn command(key: Key) -> Option<Action> {
    let character = match key {
        Key::Escape => return Some(Action::Escape),
        Key::Delete | Key::Backspace => return Some(Action::DeleteSelection),
        Key::Enter => return None,
        Key::Char(c) => c,
    };
    match character {
        'd' => Some(Action::EnterDraw),
        'p' => Some(Action::ToggleDraw),
        'h' => Some(Action::ToggleVisibility),
        'g' => Some(Action::TogglePark),
        'z' => Some(Action::CycleZoom),
        'w' => Some(Action::Save),
        'o' => Some(Action::Load),
        'u' => Some(Action::Undo),
        'r' => Some(Action::Redo),
        'x' => Some(Action::Clear),
        'c' => Some(Action::CycleColor),
        't' => Some(Action::ShowWindowMenu),
        '1' => Some(Action::SelectTool(Tool::Pen)),
        '2' => Some(Action::SelectTool(Tool::Highlighter)),
        '3' => Some(Action::SelectTool(Tool::Line)),
        '4' => Some(Action::SelectTool(Tool::Arrow)),
        '5' => Some(Action::SelectTool(Tool::Rectangle)),
        '6' => Some(Action::SelectTool(Tool::Ellipse)),
        '7' | 'e' => Some(Action::SelectTool(Tool::Eraser)),
        '8' | 's' => Some(Action::SelectTool(Tool::Select)),
        '9' => Some(Action::SelectTool(Tool::Text)),
        '[' => Some(Action::AdjustWidth(-1)),
        ']' => Some(Action::AdjustWidth(1)),
        '-' => Some(Action::AdjustOpacity(-1)),
        '=' | '+' => Some(Action::AdjustOpacity(1)),
        _ => None,
    }
}

/// Whether this key ends the program.
///
/// Separate from [`command`] because quitting is the adapter's to perform: the
/// controller has no notion of a process.
pub fn quits(key: Key) -> bool {
    key == Key::Char('q')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_keeps_only_the_keys_that_cannot_be_typed() {
        assert_eq!(editing(Key::Escape), Some(Action::Escape));
        assert_eq!(editing(Key::Backspace), Some(Action::BackspaceText));
        assert_eq!(editing(Key::Enter), Some(Action::NewlineText));
    }

    /// FR-023, and the failure is severe and quiet: typing a word into the text
    /// tool would quit, clear the drawing and change the colour.
    #[test]
    fn every_character_is_text_while_editing() {
        for c in "qxcdpu19abz".chars() {
            assert_eq!(
                editing(Key::Char(c)),
                None,
                "{c:?} acted as a command while the text editor was open"
            );
        }
    }

    #[test]
    fn quitting_is_not_also_an_action() {
        assert!(quits(Key::Char('q')));
        assert_eq!(
            command(Key::Char('q')),
            None,
            "q would both quit and do something else"
        );
    }

    /// Shift must not change what a key does, so adapters lower-case before
    /// they get here. This pins the expectation they are written against.
    #[test]
    fn the_table_expects_lower_case() {
        assert_eq!(command(Key::Char('d')), Some(Action::EnterDraw));
        assert_eq!(
            command(Key::Char('D')),
            None,
            "an upper-case key reached the table, so some adapter is not \
             lower-casing and shift will break its bindings"
        );
    }

    #[test]
    fn the_tools_are_bound_where_they_are_documented() {
        let expected = [
            ('1', Tool::Pen),
            ('2', Tool::Highlighter),
            ('3', Tool::Line),
            ('4', Tool::Arrow),
            ('5', Tool::Rectangle),
            ('6', Tool::Ellipse),
            ('7', Tool::Eraser),
            ('e', Tool::Eraser),
            ('8', Tool::Select),
            ('s', Tool::Select),
            ('9', Tool::Text),
        ];
        for (key, tool) in expected {
            assert_eq!(
                command(Key::Char(key)),
                Some(Action::SelectTool(tool)),
                "{key:?} is not bound to {tool:?}"
            );
        }
    }

    #[test]
    fn an_unbound_key_does_nothing() {
        for c in "0abfijklmnvy".chars() {
            assert_eq!(command(Key::Char(c)), None, "{c:?} is bound unexpectedly");
        }
    }

    /// Both delete keys remove the selection outside the editor, and neither
    /// does inside it, where backspace is a character operation.
    #[test]
    fn delete_and_backspace_agree_outside_the_editor() {
        assert_eq!(command(Key::Delete), Some(Action::DeleteSelection));
        assert_eq!(command(Key::Backspace), Some(Action::DeleteSelection));
        assert_eq!(editing(Key::Delete), None);
        assert_eq!(editing(Key::Backspace), Some(Action::BackspaceText));
    }
}
