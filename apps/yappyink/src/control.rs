//! The local control channel (T012, FR-005).
//!
//! A running overlay listens on a Unix-domain socket. A second invocation of
//! the binary, such as `yappyink toggle-draw`, connects and sends one command.
//! That is what makes activation possible at all: once PassThrough is working,
//! the application underneath owns the keyboard, so no shortcut inside our own
//! surface can reach us (E004). The user binds this command to a chord in their
//! desktop's settings and it works regardless of what has focus.
//!
//! # Security
//!
//! The socket lives in `XDG_RUNTIME_DIR`, which the system creates per user
//! with mode 0700, and is additionally chmod 0600. It is a local, same-user
//! channel: not a network listener, and not a shell. The command set is a
//! closed enum of five verbs with no arguments, the line length is bounded, and
//! anything unrecognised is refused. Nothing read from this socket reaches a
//! process, a path, or a document.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use ink_app::Action;

/// The longest line the server will read. A control verb is a dozen bytes; this
/// is generous and still bounded (NFR-003).
const MAX_LINE: u64 = 64;

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

/// Where the socket lives.
///
/// `XDG_RUNTIME_DIR` is per user and per session and is cleaned up on logout,
/// which is exactly the lifetime a control socket should have. Without it there
/// is no directory we can be confident is private, so we refuse rather than
/// falling back to somewhere world-writable like `/tmp`.
pub fn socket_path() -> Result<PathBuf, String> {
    socket_path_in(std::env::var("XDG_RUNTIME_DIR").ok().as_deref())
}

/// The path resolution itself, with the directory injected so it can be tested
/// without touching the process environment.
fn socket_path_in(runtime_dir: Option<&str>) -> Result<PathBuf, String> {
    match runtime_dir {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir).join("yappyink.sock")),
        Some(_) => Err("XDG_RUNTIME_DIR is empty".to_owned()),
        None => Err(
            "XDG_RUNTIME_DIR is not set, so there is no private directory for the \
                     control socket"
                .to_owned(),
        ),
    }
}

/// Sends one command to a running overlay.
///
/// Returns an error if nothing is listening, which is the ordinary case when
/// the overlay is not running and must be reported plainly rather than by
/// starting one.
pub fn send(command: ControlCommand) -> Result<String, String> {
    let path = socket_path()?;
    let mut stream = UnixStream::connect(&path).map_err(|e| {
        format!(
            "no overlay is listening on {} ({e}). Start one with `yappyink draw`.",
            path.display()
        )
    })?;
    writeln!(stream, "{}", command.as_str())
        .map_err(|e| format!("the command could not be sent: {e}"))?;
    stream.flush().ok();

    let mut reply = String::new();
    BufReader::new(&stream)
        .take(MAX_LINE)
        .read_line(&mut reply)
        .map_err(|e| format!("no reply: {e}"))?;
    Ok(reply.trim().to_owned())
}

/// Listens for commands and forwards them on `sender`.
///
/// Runs on its own thread: accepting a connection blocks, and the compositor
/// must keep being served. Returns the socket's path so the caller can clean it
/// up.
pub fn serve(sender: Sender<ControlCommand>) -> Result<PathBuf, String> {
    let path = socket_path()?;

    // A socket file left behind by a crashed run would make bind fail. Probe it
    // first: if something answers, another overlay is genuinely running and we
    // must not steal its socket.
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            return Err(format!(
                "another yappyink is already listening on {}. Use `yappyink toggle-draw` to talk \
                 to it, or quit it first.",
                path.display()
            ));
        }
        std::fs::remove_file(&path).map_err(|e| {
            format!(
                "a stale socket at {} could not be removed: {e}",
                path.display()
            )
        })?;
    }

    let listener = UnixListener::bind(&path).map_err(|e| {
        format!(
            "the control socket {} could not be created: {e}",
            path.display()
        )
    })?;

    // XDG_RUNTIME_DIR is already 0700, so this is belt and braces. It costs
    // nothing and removes any doubt about the socket's reachability.
    if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
        eprintln!("[warn] the control socket's permissions could not be tightened: {e}");
    }

    let thread_path = path.clone();
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(stream) = connection else { continue };
            if !handle(stream, &sender) {
                break;
            }
        }
        let _ = &thread_path;
    });

    Ok(path)
}

/// Serves one connection. Returns false when the listener should stop.
fn handle(stream: UnixStream, sender: &Sender<ControlCommand>) -> bool {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&stream).take(MAX_LINE);
        if reader.read_line(&mut line).is_err() {
            return true;
        }
    }

    let reply = |text: &str| {
        let mut out = &stream;
        let _ = writeln!(out, "{text}");
        let _ = out.flush();
    };

    match ControlCommand::parse(&line) {
        Some(command) => {
            if sender.send(command).is_err() {
                // The overlay has gone. Nothing left to forward to.
                reply("error the overlay is no longer running");
                return false;
            }
            reply("ok");
            true
        }
        None => {
            // Deliberately does not echo what was sent, so the socket cannot be
            // used to bounce arbitrary text back at a caller.
            reply("error unknown command");
            true
        }
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

    #[test]
    fn the_socket_path_refuses_to_guess_when_the_runtime_dir_is_missing() {
        // An absent XDG_RUNTIME_DIR must be a refusal, never a fallback to a
        // world-writable directory such as /tmp.
        assert!(socket_path_in(None).is_err());
        assert!(socket_path_in(Some("")).is_err());
    }

    #[test]
    fn the_socket_lives_in_the_runtime_directory() {
        let path = socket_path_in(Some("/run/user/1001")).unwrap();
        assert_eq!(path, PathBuf::from("/run/user/1001/yappyink.sock"));
    }
}
