//! The `draw` subcommand: the overlay running against the real compositor.
//!
//! Requirements: FR-001, FR-002, FR-003, FR-004, FR-019. Task T011.
//!
//! Everything about modes, gestures, and the document happens in `ink-app` and
//! `ink-core`; everything native happens in the platform adapter. This file
//! only wires them together and provides the terminal recovery route.

use std::io::BufRead;
use std::sync::mpsc;

use ink_app::Action;
use ink_core::{LogicalSize, Opacity, Rgb, Style, Width};

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

    // Hidden withdraws the surface, and the keyboard goes with it. Until T012
    // provides a control channel and a global shortcut, the controlling
    // terminal is the way back. A thread, because reading a line blocks and
    // the compositor must keep being served.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(_) = line else { break };
            if sender.send(Action::EnterDraw).is_err() {
                break;
            }
        }
    });

    let config = ink_platform_wayland::overlay::OverlayConfig {
        style: default_style(),
        size,
        recovery: Some(receiver),
    };

    match ink_platform_wayland::overlay::run(config) {
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
