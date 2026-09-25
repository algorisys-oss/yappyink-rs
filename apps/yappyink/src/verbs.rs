//! The remote-control verbs: `yappyink toggle-draw` and the rest.
//!
//! One closed list, shared by every way a verb can travel. On Linux it goes
//! over a Unix-domain socket (`control`), because a Wayland compositor gives
//! a client no global shortcut. On Windows it is posted to the overlay's window
//! as a message numbered by the verb's position here. The list lives in its own
//! module so that neither transport can grow a verb the other lacks.

use ink_app::Action;

/// What a client may ask for.
///
/// A closed set. Adding a verb is a deliberate act, which is the point: an
/// open-ended control API on a socket is how a convenience becomes a way to
/// drive someone else's session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlCommand {
    /// Draw if not drawing, PassThrough if drawing.
    ToggleDraw,
    /// Enter Draw directly.
    Draw,
    /// Enter PassThrough directly.
    PassThrough,
    /// Hide the ink, keeping it in memory.
    Hide,
    /// Withdraw everything at once, cancelling any gesture.
    EmergencyHide,
    /// Write the document to the session file.
    Save,
    /// Replace the document with the session file's contents.
    Load,
    /// Ask the running overlay to exit.
    Quit,
}

impl ControlCommand {
    /// Parses one line. Case-insensitive, surrounding whitespace ignored.
    pub fn parse(line: &str) -> Option<Self> {
        match line.trim().to_ascii_lowercase().as_str() {
            "toggle-draw" => Some(Self::ToggleDraw),
            "draw" => Some(Self::Draw),
            "pass-through" => Some(Self::PassThrough),
            "hide" => Some(Self::Hide),
            "emergency-hide" => Some(Self::EmergencyHide),
            "save" => Some(Self::Save),
            "load" => Some(Self::Load),
            "quit" => Some(Self::Quit),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToggleDraw => "toggle-draw",
            Self::Draw => "draw",
            Self::PassThrough => "pass-through",
            Self::Hide => "hide",
            Self::EmergencyHide => "emergency-hide",
            Self::Save => "save",
            Self::Load => "load",
            Self::Quit => "quit",
        }
    }

    /// The controller action this asks for, if it is one. `Quit` is not.
    pub fn action(self) -> Option<Action> {
        match self {
            Self::ToggleDraw => Some(Action::ToggleDraw),
            Self::Draw => Some(Action::EnterDraw),
            Self::PassThrough => Some(Action::ToggleDraw),
            Self::Hide => Some(Action::ToggleVisibility),
            Self::EmergencyHide => Some(Action::EmergencyHide),
            Self::Save => Some(Action::Save),
            Self::Load => Some(Action::Load),
            Self::Quit => None,
        }
    }

    /// Every verb. Used for help text and to keep the tests exhaustive.
    pub const fn all() -> [Self; 8] {
        [
            Self::ToggleDraw,
            Self::Draw,
            Self::PassThrough,
            Self::Hide,
            Self::EmergencyHide,
            Self::Save,
            Self::Load,
            Self::Quit,
        ]
    }
}

/// Only Windows carries a verb as a number.
#[cfg(any(windows, test))]
impl ControlCommand {
    /// The verb's position in [`ControlCommand::all`], which is what a
    /// Windows window message carries.
    pub fn index(self) -> u32 {
        Self::all()
            .iter()
            .position(|command| *command == self)
            .expect("every verb is in the list") as u32
    }

    /// The verb at a position, or `None` for a number that is not one.
    pub fn from_index(index: u32) -> Option<Self> {
        Self::all().get(index as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_round_trips_through_its_own_name() {
        for command in ControlCommand::all() {
            assert_eq!(ControlCommand::parse(command.as_str()), Some(command));
        }
    }

    #[test]
    fn parsing_ignores_case_and_surrounding_whitespace() {
        assert_eq!(
            ControlCommand::parse("  Toggle-Draw \n"),
            Some(ControlCommand::ToggleDraw)
        );
    }

    #[test]
    fn anything_outside_the_verb_set_is_refused() {
        for line in [
            "",
            "draw; rm -rf /",
            "../../etc/passwd",
            "DRAW EXTRA",
            "toggledraw",
            "quit()",
            "\0draw",
        ] {
            assert_eq!(ControlCommand::parse(line), None, "{line:?} must not parse");
        }
    }

    #[test]
    fn quit_is_the_only_verb_that_is_not_a_controller_action() {
        for command in ControlCommand::all() {
            let is_quit = command == ControlCommand::Quit;
            assert_eq!(command.action().is_none(), is_quit, "{command:?}");
        }
    }

    #[test]
    fn emergency_hide_maps_to_the_action_that_never_waits() {
        assert_eq!(
            ControlCommand::EmergencyHide.action(),
            Some(Action::EmergencyHide)
        );
    }

    /// A message number is all a Windows overlay receives, so the numbering
    /// has to be a bijection over the list and refuse anything past it.
    #[test]
    fn every_verb_round_trips_through_its_index() {
        for command in ControlCommand::all() {
            assert_eq!(ControlCommand::from_index(command.index()), Some(command));
        }
        assert_eq!(
            ControlCommand::from_index(ControlCommand::all().len() as u32),
            None
        );
        assert_eq!(ControlCommand::from_index(u32::MAX), None);
    }
}
