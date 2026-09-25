//! What the first argument asks for.
//!
//! Pure, and compiled everywhere, so the rules are tested on any machine. The
//! platform-specific parts, which verbs exist and which backend draws, stay in
//! `main`.

/// Where an invocation goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Route<'a> {
    /// Start the overlay, with whatever followed `draw`.
    Draw(&'a [String]),
    Doctor,
    Version,
    Help,
    /// Anything else: a control verb on platforms that have them, otherwise an
    /// unknown command. `main` decides, because only it knows the platform.
    Other(&'a str),
}

/// Routes the arguments after the program name.
///
/// **No arguments means draw.** That is what a double-click does on Windows and
/// macOS, and until 0.8.0 it printed the usage and exited, so the first thing a
/// new user saw was a console flashing up and vanishing. The toolbar is the
/// expectation, so the toolbar is the default.
///
/// An unrecognised word is *not* treated as draw. `yappyink toggel-draw`, a
/// typo for a control verb, would otherwise open a second overlay instead of
/// saying what went wrong.
pub fn route(args: &[String]) -> Route<'_> {
    match args.first().map(String::as_str) {
        None => Route::Draw(&[]),
        Some("draw") => Route::Draw(&args[1..]),
        Some("doctor") => Route::Doctor,
        Some("version" | "--version" | "-V") => Route::Version,
        Some("help" | "--help" | "-h") => Route::Help,
        Some(other) => Route::Other(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    /// A double-click passes no arguments. It must open the overlay.
    #[test]
    fn no_arguments_draws() {
        assert_eq!(route(&[]), Route::Draw(&[]));
    }

    #[test]
    fn draw_keeps_its_own_arguments() {
        let given = args(&["draw", "--monitor", "2"]);
        assert_eq!(route(&given), Route::Draw(&given[1..]));
    }

    #[test]
    fn help_is_a_request_not_an_error() {
        for word in ["help", "--help", "-h"] {
            assert_eq!(route(&args(&[word])), Route::Help, "{word}");
        }
    }

    #[test]
    fn the_other_subcommands_are_unchanged() {
        assert_eq!(route(&args(&["doctor"])), Route::Doctor);
        assert_eq!(route(&args(&["version"])), Route::Version);
        assert_eq!(route(&args(&["--version"])), Route::Version);
    }

    /// A typo must not start an overlay; it has to reach the unknown-command
    /// path, where the verbs are listed.
    #[test]
    fn an_unknown_word_is_not_draw() {
        assert_eq!(route(&args(&["toggel-draw"])), Route::Other("toggel-draw"));
        assert_eq!(route(&args(&["--monitor", "2"])), Route::Other("--monitor"));
    }
}
