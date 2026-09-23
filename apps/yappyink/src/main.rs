//! The yappyink spike binary.
//!
//! Subcommands arrive with the task that needs them: `doctor` is T002, the
//! overlay is T011, and `toggle-draw` over the local control channel is T012.
//! An unimplemented subcommand exits non-zero rather than printing something
//! that could be mistaken for a result.

#[cfg(target_os = "linux")]
mod draw;

mod doctor;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("doctor") => {
            print!("{}", doctor::report());
            std::process::ExitCode::SUCCESS
        }
        #[cfg(target_os = "linux")]
        Some("draw") => draw::run(),
        Some("--version" | "version") => {
            println!("yappyink {}", env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        other => {
            if let Some(command) = other {
                eprintln!("unknown command: {command}");
            }
            eprintln!("usage: yappyink <draw|doctor|version>");
            eprintln!(
                "draw starts the overlay on Wayland. doctor reports what this machine offers. \
                 Windows, macOS, and X11 have no backend yet (T003, T004, T005)."
            );
            std::process::ExitCode::from(2)
        }
    }
}
