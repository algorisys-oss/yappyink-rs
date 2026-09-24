//! The `draw` subcommand: the overlay running against the real compositor.
//!
//! Requirements: FR-001, FR-002, FR-003, FR-004, FR-005, FR-019.
//! Tasks: T011, T012.
//!
//! Everything about modes, gestures, and the document happens in `ink-app` and
//! `ink-core`; everything native happens in the platform adapter. This file
//! wires them together and runs the control socket.

use std::sync::mpsc;

use ink_core::LogicalSize;
use ink_platform_wayland::overlay::{OverlayConfig, OverlayRequest, control_channel};

use crate::control::{self, ControlCommand};

pub fn run() -> std::process::ExitCode {
    let size = LogicalSize::new(1280.0, 720.0).expect("a positive default size");

    let (sender, receiver) = control_channel();
    let (commands, from_socket) = mpsc::channel::<ControlCommand>();

    let socket = match control::serve(commands) {
        Ok(path) => {
            eprintln!("[control] listening on {}", path.display());
            Some(path)
        }
        Err(reason) => {
            // Not fatal. The overlay still works from its own keyboard; what is
            // lost is activation while another window has focus. Saying so is
            // the point of NFR-005: a degraded capability must be visible, not
            // silently absent.
            eprintln!("[control] unavailable: {reason}");
            eprintln!("[control] the overlay will still run, but only its own keys will reach it.");
            None
        }
    };

    // Translate control verbs into overlay requests on a forwarding thread, so
    // the adapter never sees the socket's vocabulary.
    std::thread::spawn(move || {
        for command in from_socket {
            let request = match command.action() {
                Some(action) => OverlayRequest::Act(action),
                None => OverlayRequest::Quit,
            };
            if sender.send(request).is_err() {
                break;
            }
        }
    });

    print_orientation();

    // The tool and its style live in the controller, so nothing here chooses
    // a colour: the overlay starts with the pen and the user changes it.
    let config = OverlayConfig {
        size,
        control: Some(receiver),
    };
    let result = ink_platform_wayland::overlay::run(config);

    // The socket is ours, so we remove it. Leaving it behind would make the
    // next run think an overlay is already listening.
    if let Some(path) = socket {
        let _ = std::fs::remove_file(path);
    }

    match result {
        Ok(session) => {
            eprintln!(
                "\n[exit] {} object(s) were on screen.",
                session.document().len()
            );
            // This used to say saving did not exist, and kept saying it for
            // several commits after `w` started working. A parting message is
            // read at exactly the moment it is too late to act on, so it has
            // to be true.
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

fn print_orientation() {
    eprintln!(
        "Look for a thin outlined rectangle with a small square in its corner: that\n\
         is the overlay, and it is transparent everywhere else. You can only draw\n\
         inside it. Drag with the LEFT MOUSE BUTTON to draw; the keys below change\n\
         mode, they do not draw.\n\
         The corner square is cyan in draw mode and amber in pass-through.\n\n\
         To reach the overlay while another window has focus, run in another\n\
         terminal, or bind it to a chord in your desktop's keyboard settings:\n\n\
         \x20   yappyink toggle-draw\n\n\
         That is the supported activation route on this backend. Wayland gives an\n\
         ordinary application no way to register a global shortcut for itself; the\n\
         GlobalShortcuts portal is the other route and is not implemented yet.\n"
    );
}
