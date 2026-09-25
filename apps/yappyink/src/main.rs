//! The yappyink spike binary.
//!
//! Subcommands arrive with the task that needs them: `doctor` is T002, the
//! overlay is T011, and `toggle-draw` over the local control channel is T012.
//! An unimplemented subcommand exits non-zero rather than printing something
//! that could be mistaken for a result.

#[cfg(target_os = "linux")]
mod control;
#[cfg(target_os = "linux")]
mod draw;
#[cfg(any(windows, test))]
mod draw_args;
#[cfg(target_os = "macos")]
mod draw_macos;
#[cfg(windows)]
mod draw_windows;
#[cfg(any(target_os = "linux", windows))]
mod verbs;

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
        #[cfg(windows)]
        Some("draw") => draw_windows::run(&args[1..]),
        #[cfg(target_os = "macos")]
        Some("draw") => draw_macos::run(),
        // Present on every platform, and refuses on the ones without a
        // backend. Reporting `draw` as an unknown command on Windows would be
        // a lie about the command rather than the truth about the platform,
        // and a reader cannot tell a missing feature from a typo.
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        Some("draw") => {
            eprintln!(
                "[failed unsupported] there is no overlay backend for {}.",
                std::env::consts::OS
            );
            eprintln!("`yappyink doctor` works here and reports what this machine offers.");
            std::process::ExitCode::FAILURE
        }
        #[cfg(target_os = "linux")]
        Some(verb) if control::ControlCommand::parse(verb).is_some() => {
            let command = control::ControlCommand::parse(verb).expect("just checked");
            match control::send(command) {
                Ok(reply) if reply == "ok" => std::process::ExitCode::SUCCESS,
                Ok(reply) => {
                    eprintln!("{reply}");
                    std::process::ExitCode::FAILURE
                }
                Err(reason) => {
                    eprintln!("{reason}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        // The same verbs on Windows, posted to the overlay's window instead of
        // written to a socket.
        #[cfg(windows)]
        Some(verb) if verbs::ControlCommand::parse(verb).is_some() => {
            let command = verbs::ControlCommand::parse(verb).expect("just checked");
            match ink_platform_windows::overlay::send_remote(command.index()) {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(reason) => {
                    eprintln!("{reason}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Some("--version" | "version") => {
            println!("yappyink {}", env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        other => {
            if let Some(command) = other {
                eprintln!("unknown command: {command}");
            }
            eprintln!("usage: yappyink <draw|doctor|version>");
            #[cfg(windows)]
            eprintln!("       yappyink draw --monitor N   cover monitor N, as listed at startup");
            // The verbs reach a running overlay over a local socket on Linux
            // and as a window message on Windows. macOS has no transport for
            // them yet, so they are not offered there: advertising a command
            // that cannot succeed is worse than not listing it.
            #[cfg(any(target_os = "linux", windows))]
            {
                let verbs: Vec<&str> = verbs::ControlCommand::all()
                    .iter()
                    .map(|c| c.as_str())
                    .collect();
                eprintln!("       yappyink <{}>", verbs.join("|"));
                eprintln!("         sends a command to an overlay that is already running;");
                eprintln!("         bind one to a chord in your desktop's keyboard settings.");
            }
            eprintln!(
                "draw starts the overlay: on GNOME Wayland, Windows and macOS, though the \
                 Windows and macOS backends have never been run by anyone (T003, T004). \
                 doctor reports what this machine offers. X11 and wlroots have no backend (T005)."
            );
            std::process::ExitCode::from(2)
        }
    }
}
