//! The `draw` subcommand: the overlay running against the real compositor.
//!
//! Requirements: FR-001, FR-002, FR-003, FR-004, FR-005, FR-019.
//! Tasks: T011, T012.
//!
//! Everything about modes, gestures, and the document happens in `ink-app` and
//! `ink-core`; everything native happens in the platform adapter. This file
//! wires them together and runs the control socket.

use std::sync::mpsc;

use ink_core::{LogicalSize, Opacity, Rgb, Style, Width};
use ink_platform_wayland::overlay::{OverlayConfig, OverlayRequest, control_channel};

use crate::control::{self, ControlCommand};

/// The starting tool. A palette is T015/T016; one pen is enough for the slice.
fn default_style() -> Style {
    Style::new(
        Rgb::new(255, 0, 255),
        Width::new(4.0).expect("a positive default width"),
        Opacity::OPAQUE,
    )
}

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

    let config = OverlayConfig {
        style: default_style(),
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
        Ok(document) => {
            eprintln!("\n[exit] {} object(s) were drawn.", document.len());
            eprintln!(
                "[exit] They are not saved: explicit local files are T020, and this build has \
                 nowhere to put them."
            );
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
