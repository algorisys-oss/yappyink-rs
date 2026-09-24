//! The `draw` subcommand on macOS.
//!
//! Requirements: FR-001, FR-002, FR-003. Task: T004.
//!
//! Note what is *not* in the requirement list: FR-005, activation and escape.
//! macOS has no global shortcut without Carbon or an accessibility grant, so
//! there is no way back from pass-through except this terminal. That is stated
//! here rather than quietly omitted, and `docs/adr/ADR-006-macos-bindings.md`
//! records it as needing its own decision.
//!
//! **Nothing here has been run.** Nobody on this project has a Mac.

use ink_core::LogicalSize;
use ink_platform_macos::overlay::{self, OverlayConfig};

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
