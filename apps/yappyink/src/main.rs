//! The yappyink binary.
//!
//! Run with no arguments, or double-clicked, it opens the overlay. `doctor`,
//! `version`, `help` and the control verbs are subcommands. An unknown
//! subcommand exits non-zero rather than printing something that could be
//! mistaken for a result. The routing rules are in `route`, where they are
//! tested.

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
mod route;

use route::Route;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match route::route(&args) {
        Route::Doctor => {
            print!("{}", doctor::report());
            std::process::ExitCode::SUCCESS
        }
        Route::Draw(rest) => draw(rest),
        Route::Version => {
            println!("yappyink {}", env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        Route::Help => {
            usage();
            std::process::ExitCode::SUCCESS
        }
        #[cfg(target_os = "linux")]
        Route::Other(verb) if control::ControlCommand::parse(verb).is_some() => {
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
        Route::Other(verb) if verbs::ControlCommand::parse(verb).is_some() => {
            let command = verbs::ControlCommand::parse(verb).expect("just checked");
            match ink_platform_windows::overlay::send_remote(command.index()) {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(reason) => {
                    eprintln!("{reason}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Route::Other(command) => {
            eprintln!("unknown command: {command}");
            usage();
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(target_os = "linux")]
fn draw(_rest: &[String]) -> std::process::ExitCode {
    draw::run()
}

#[cfg(windows)]
fn draw(rest: &[String]) -> std::process::ExitCode {
    draw_windows::run(rest)
}

#[cfg(target_os = "macos")]
fn draw(_rest: &[String]) -> std::process::ExitCode {
    draw_macos::run()
}

/// Present on every platform, and refuses on the ones without a backend.
/// Reporting `draw` as an unknown command there would be a lie about the
/// command rather than the truth about the platform.
#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn draw(_rest: &[String]) -> std::process::ExitCode {
    eprintln!(
        "[failed unsupported] there is no overlay backend for {}.",
        std::env::consts::OS
    );
    eprintln!("`yappyink doctor` works here and reports what this machine offers.");
    std::process::ExitCode::FAILURE
}

fn usage() {
    eprintln!("usage: yappyink [draw|doctor|version|help]");
    eprintln!("       with no command, draws: the same as double-clicking it");
    #[cfg(windows)]
    eprintln!("       yappyink draw --monitor N   cover monitor N, as listed at startup");
    // The verbs reach a running overlay over a local socket on Linux and as a
    // window message on Windows. macOS has no transport for them yet, so they
    // are not offered there: advertising a command that cannot succeed is
    // worse than not listing it.
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
        "draw starts the overlay on GNOME Wayland, Windows and macOS. doctor reports what \
         this machine offers and what has been confirmed on it. X11 and wlroots have no \
         backend (T005)."
    );
}
