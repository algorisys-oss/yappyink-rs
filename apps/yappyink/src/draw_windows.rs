//! The `draw` subcommand on Windows.
//!
//! Requirements: FR-001, FR-002, FR-003, FR-005, FR-015. Task: T003.
//!
//! Much shorter than the Linux one, and the difference is the point. On Wayland
//! the overlay needs a control socket, because a compositor will not give a
//! client a global shortcut. Windows binds a real chord with `RegisterHotKey`,
//! and the CLI verbs arrive as window messages, so there is no socket and no
//! forwarding thread here.
//!
//! **Nothing here has been run.** See `docs/adr/ADR-005-windows-bindings.md`.

use ink_platform_windows::overlay::{self, OverlayConfig, Remote};

use crate::draw_args;
use crate::verbs::ControlCommand;

/// Turns a verb number from a window message into what it asks for. The
/// numbering is the application's, so the decoding is too.
fn decode(index: u32) -> Option<Remote> {
    let command = ControlCommand::from_index(index)?;
    Some(match command.action() {
        Some(action) => Remote::Act(action),
        None => Remote::Quit,
    })
}

pub fn run(args: &[String]) -> std::process::ExitCode {
    let parsed = match draw_args::parse(args) {
        Ok(parsed) => parsed,
        Err(reason) => {
            eprintln!("{reason}");
            return std::process::ExitCode::from(2);
        }
    };

    let config = OverlayConfig {
        monitor: parsed.monitor,
        // The whole monitor. On Windows a transparent window may cover a
        // screen; the smaller default exists on Linux only because Mutter
        // stops compositing one that does (E002).
        size: None,
        remote: decode,
    };

    match overlay::run(config) {
        Ok(session) => {
            eprintln!(
                "\n[exit] {} object(s) were on screen.",
                session.document().len()
            );
            match ink_storage::default_session_path() {
                Ok(path) if path.exists() => {
                    eprintln!("[exit] The session file is {}.", path.display());
                }
                Ok(path) => {
                    eprintln!(
                        "[exit] Nothing was written. `w` saves to {}, and quitting does not.",
                        path.display()
                    );
                }
                Err(reason) => {
                    eprintln!(
                        "[exit] Nothing was written, and there is nowhere to write: {reason}"
                    );
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[failed {}] {error}", error.class());
            std::process::ExitCode::FAILURE
        }
    }
}
