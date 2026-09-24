//! The `draw` subcommand on Windows.
//!
//! Requirements: FR-001, FR-002, FR-003, FR-005. Task: T003.
//!
//! Much shorter than the Linux one, and the difference is the point. On Wayland
//! the overlay needs a control socket, because a compositor will not give a
//! client a global shortcut and there is otherwise no way back once
//! pass-through is on. Windows binds a real chord with `RegisterHotKey`, so the
//! recovery route lives inside the adapter and there is no socket, no
//! forwarding thread and no vocabulary to translate.
//!
//! **Nothing here has been run.** See `docs/adr/ADR-005-windows-bindings.md`.

use ink_core::LogicalSize;
use ink_platform_windows::overlay::{self, OverlayConfig};

pub fn run() -> std::process::ExitCode {
    let size = LogicalSize::new(1280.0, 720.0).expect("a positive default size");

    match overlay::run(OverlayConfig { size }) {
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
