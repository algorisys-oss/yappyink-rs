//! Virtual-key codes to actions.
//!
//! Deliberately free of every Windows type. A `WM_KEYDOWN` hands us a virtual
//! key code, which is a `u32`, and what that code should mean is a decision
//! this project makes rather than something the OS tells us. Written against
//! plain integers, it compiles and its tests run on the development machine,
//! which is the difference between this being checked and being hoped about.
//!
//! The bindings match the Wayland adapter's exactly. A user moving between the
//! two should not have to relearn anything, and a divergence here would be a
//! bug nobody would notice for months.

use ink_app::{Action, Tool};

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
    pub const KEY_1: u32 = 0x31;
    pub const KEY_2: u32 = 0x32;
    pub const KEY_3: u32 = 0x33;
    pub const KEY_4: u32 = 0x34;
    pub const KEY_5: u32 = 0x35;
    pub const KEY_6: u32 = 0x36;
    pub const KEY_7: u32 = 0x37;
    pub const KEY_8: u32 = 0x38;
    pub const KEY_9: u32 = 0x39;
    pub const A: u32 = 0x41;
    pub const C: u32 = 0x43;
    pub const D: u32 = 0x44;
    pub const E: u32 = 0x45;
    pub const G: u32 = 0x47;
    pub const H: u32 = 0x48;
    pub const O: u32 = 0x4F;
    pub const P: u32 = 0x50;
    pub const Q: u32 = 0x51;
    pub const R: u32 = 0x52;
    pub const S: u32 = 0x53;
    pub const T: u32 = 0x54;
    pub const U: u32 = 0x55;
    pub const W: u32 = 0x57;
    pub const X: u32 = 0x58;
    pub const OEM_MINUS: u32 = 0xBD;
    pub const OEM_PLUS: u32 = 0xBB;
    pub const OEM_4: u32 = 0xDB;
    pub const OEM_6: u32 = 0xDD;
}

/// What a key means while the text editor is open.
///
/// FR-023: the editor owns the keyboard. A key is a character, not a shortcut,
/// or typing "q" would quit and typing "x" would erase the drawing. Only the
/// three keys that cannot be characters keep their meaning.
///
/// Returns `None` for anything that should be treated as typed text instead;
/// the caller has the translated character and this module deliberately does
/// not, because turning a key code into a character is the OS's job and
/// depends on the active layout.
pub fn editing(virtual_key: u32) -> Option<Action> {
    match virtual_key {
        vk::ESCAPE => Some(Action::Escape),
        vk::BACK => Some(Action::BackspaceText),
        vk::RETURN => Some(Action::NewlineText),
        _ => None,
    }
}

/// What a key means the rest of the time.
///
/// `Quit` is absent on purpose: quitting is the caller's to perform, not an
/// `Action` the controller understands, and the Wayland adapter treats it the
/// same way.
pub fn command(virtual_key: u32) -> Option<Action> {
    match virtual_key {
        vk::D => Some(Action::EnterDraw),
        vk::P => Some(Action::ToggleDraw),
        vk::H => Some(Action::ToggleVisibility),
        vk::ESCAPE => Some(Action::Escape),
        vk::KEY_1 => Some(Action::SelectTool(Tool::Pen)),
        vk::KEY_2 => Some(Action::SelectTool(Tool::Highlighter)),
        vk::KEY_3 => Some(Action::SelectTool(Tool::Line)),
        vk::KEY_4 => Some(Action::SelectTool(Tool::Arrow)),
        vk::KEY_5 => Some(Action::SelectTool(Tool::Rectangle)),
        vk::KEY_6 => Some(Action::SelectTool(Tool::Ellipse)),
        vk::KEY_7 | vk::E => Some(Action::SelectTool(Tool::Eraser)),
        vk::KEY_8 | vk::S => Some(Action::SelectTool(Tool::Select)),
        vk::KEY_9 => Some(Action::SelectTool(Tool::Text)),
        vk::G => Some(Action::TogglePark),
        vk::W => Some(Action::Save),
        vk::O => Some(Action::Load),
        vk::DELETE | vk::BACK => Some(Action::DeleteSelection),
        vk::U => Some(Action::Undo),
        vk::R => Some(Action::Redo),
        vk::X => Some(Action::Clear),
        vk::C => Some(Action::CycleColor),
        vk::OEM_4 => Some(Action::AdjustWidth(-1)),
        vk::OEM_6 => Some(Action::AdjustWidth(1)),
        vk::OEM_MINUS => Some(Action::AdjustOpacity(-1)),
        vk::OEM_PLUS => Some(Action::AdjustOpacity(1)),
        _ => None,
    }
}

/// Whether this key ends the program.
///
/// Separate from [`command`] because quitting is not something the controller
/// can be asked to do.
pub fn quits(virtual_key: u32) -> bool {
    virtual_key == vk::Q
}

/// `t` opens the compositor's window menu on Wayland, where Always on Top
/// lives. Windows does not need it: the overlay is already top-most by its own
/// extended style, so the key is accepted and does nothing rather than being
/// silently absent and confusing anyone moving between platforms.
pub fn is_window_menu(virtual_key: u32) -> bool {
    virtual_key == vk::T
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_keeps_only_the_keys_that_cannot_be_characters() {
        assert_eq!(editing(vk::ESCAPE), Some(Action::Escape));
        assert_eq!(editing(vk::BACK), Some(Action::BackspaceText));
        assert_eq!(editing(vk::RETURN), Some(Action::NewlineText));
    }

    /// FR-023. The failure this guards is severe and quiet: typing a word into
    /// the text tool would quit, clear the drawing and change the colour.
    #[test]
    fn every_other_key_is_text_while_editing() {
        for key in [
            vk::Q,
            vk::X,
            vk::C,
            vk::D,
            vk::P,
            vk::U,
            vk::KEY_1,
            vk::KEY_9,
            vk::DELETE,
        ] {
            assert_eq!(
                editing(key),
                None,
                "virtual key {key:#04x} was treated as a command while editing"
            );
        }
    }

    #[test]
    fn quitting_is_not_an_action() {
        assert!(quits(vk::Q));
        assert_eq!(
            command(vk::Q),
            None,
            "q must not also be a controller action, or it would both quit and do something"
        );
    }

    /// The two adapters must agree. A user who learns the keys on one and finds
    /// them different on the other has been given two products.
    ///
    /// Checked by listing the bindings here rather than by reading the Wayland
    /// source, which a test cannot do. That makes this a statement of intent
    /// that fails when someone changes one side; it cannot notice a change made
    /// to both, and nothing in a test can.
    #[test]
    fn the_bindings_match_the_documented_set() {
        let expected: &[(u32, Action)] = &[
            (vk::D, Action::EnterDraw),
            (vk::P, Action::ToggleDraw),
            (vk::H, Action::ToggleVisibility),
            (vk::G, Action::TogglePark),
            (vk::W, Action::Save),
            (vk::O, Action::Load),
            (vk::U, Action::Undo),
            (vk::R, Action::Redo),
            (vk::X, Action::Clear),
            (vk::C, Action::CycleColor),
            (vk::KEY_1, Action::SelectTool(Tool::Pen)),
            (vk::KEY_2, Action::SelectTool(Tool::Highlighter)),
            (vk::KEY_3, Action::SelectTool(Tool::Line)),
            (vk::KEY_4, Action::SelectTool(Tool::Arrow)),
            (vk::KEY_5, Action::SelectTool(Tool::Rectangle)),
            (vk::KEY_6, Action::SelectTool(Tool::Ellipse)),
            (vk::KEY_7, Action::SelectTool(Tool::Eraser)),
            (vk::E, Action::SelectTool(Tool::Eraser)),
            (vk::KEY_8, Action::SelectTool(Tool::Select)),
            (vk::S, Action::SelectTool(Tool::Select)),
            (vk::KEY_9, Action::SelectTool(Tool::Text)),
            (vk::OEM_4, Action::AdjustWidth(-1)),
            (vk::OEM_6, Action::AdjustWidth(1)),
            (vk::OEM_MINUS, Action::AdjustOpacity(-1)),
            (vk::OEM_PLUS, Action::AdjustOpacity(1)),
        ];
        for (key, action) in expected {
            assert_eq!(
                command(*key),
                Some(*action),
                "virtual key {key:#04x} is not bound as documented"
            );
        }
    }

    #[test]
    fn an_unbound_key_does_nothing() {
        // VK_F13, bound to nothing on either platform.
        assert_eq!(command(0x7C), None);
        assert_eq!(command(vk::KEY_0), None);
        assert_eq!(command(vk::A), None);
    }
}
