//! The yappyink spike binary.
//!
//! Subcommands arrive with the task that needs them: `doctor` is T002, the
//! overlay is T011, and `toggle-draw` over the local control channel is T012.
//! An unimplemented subcommand exits non-zero rather than printing something
//! that could be mistaken for a result.

mod doctor;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("doctor") => {
            print!("{}", doctor::report());
            std::process::ExitCode::SUCCESS
        }
        Some("--version" | "version") => {
            println!("yappyink {}", env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        other => {
            if let Some(command) = other {
                eprintln!("unknown command: {command}");
            }
            eprintln!("usage: yappyink <doctor|version>");
            eprintln!(
                "No overlay backend exists yet. doctor reports what this machine offers; \
                 see tasks.md T005 onward."
            );
            std::process::ExitCode::from(2)
        }
    }
}
