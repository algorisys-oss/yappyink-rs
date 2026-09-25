//! The arguments `yappyink draw` takes on Windows.
//!
//! Only `--monitor N` so far, because Windows is the only backend that can
//! place its window: a Wayland client cannot choose its output at all (E003),
//! and the macOS adapter does not enumerate screens yet. Compiled everywhere
//! under test so the parsing is checked on the development machine.

/// What was asked for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DrawArgs {
    /// 1-based, as printed at startup.
    pub monitor: Option<usize>,
}

/// Parses everything after `draw`.
///
/// Anything unrecognised is refused rather than ignored: a mistyped
/// `--moniter 2` that silently opened on the primary would be exactly the
/// wrong-screen surprise the option exists to prevent.
pub fn parse(args: &[String]) -> Result<DrawArgs, String> {
    let mut parsed = DrawArgs::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let value = match arg.as_str() {
            "--monitor" => rest
                .next()
                .ok_or("--monitor needs a number, such as --monitor 2")?,
            other => match other.strip_prefix("--monitor=") {
                Some(value) => value,
                None => return Err(format!("draw does not take {other:?}")),
            },
        };
        let number: usize = value
            .parse()
            .ok()
            .filter(|number| *number >= 1)
            .ok_or_else(|| format!("--monitor takes a number from 1, not {value:?}"))?;
        parsed.monitor = Some(number);
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn nothing_means_the_primary() {
        assert_eq!(parse(&[]), Ok(DrawArgs::default()));
    }

    #[test]
    fn a_monitor_can_be_given_either_way() {
        let want = Ok(DrawArgs { monitor: Some(2) });
        assert_eq!(parse(&args(&["--monitor", "2"])), want);
        assert_eq!(parse(&args(&["--monitor=2"])), want);
    }

    #[test]
    fn a_bad_monitor_is_refused_rather_than_ignored() {
        for bad in [
            &["--monitor"][..],
            &["--monitor", "0"],
            &["--monitor", "two"],
            &["--monitor=-1"],
            &["--moniter", "2"],
            &["2"],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?} was accepted");
        }
    }
}
