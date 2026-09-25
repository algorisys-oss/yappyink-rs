//! Live zoom on GNOME, through the Shell's own magnifier (FR-029, ADR-008,
//! T037).
//!
//! The Shell magnifies the whole stage, this overlay included, so yappyink
//! never receives a pixel and nothing asks for permission. It is driven by two
//! of the user's accessibility settings, which is why most of this module is
//! about putting them back.
//!
//! # Putting the user's settings back
//!
//! Before the first change, the current values are written to a restore file
//! beside the session file. Turning zoom off, or a normal exit, restores them
//! and deletes the file. If the process is killed while zoomed, nothing runs,
//! so the next launch finds the file and restores from it. GNOME's own
//! Alt+Super+8 also turns the magnifier off, and the banner says so.
//!
//! # Why `gsettings`
//!
//! The settings could be written over D-Bus, but no D-Bus client dependency has
//! been chosen yet (it also blocks the GlobalShortcuts portal and the file
//! picker). The `gsettings` command is present wherever GNOME is. It runs on a
//! worker thread, never on the event loop (NFR-003).
//!
//! Confirmed on the E001 machine (E017): zooming, drawing while zoomed, and
//! restoring the settings all work.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

const APPLICATIONS: &str = "org.gnome.desktop.a11y.applications";
const ENABLED: &str = "screen-magnifier-enabled";
const MAGNIFIER: &str = "org.gnome.desktop.a11y.magnifier";
const FACTOR: &str = "mag-factor";

/// The user's own values, as `gsettings get` printed them. Kept as text and
/// written back verbatim, so nothing is lost in a round trip through a type.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Saved {
    enabled: String,
    factor: String,
}

/// One `gsettings set`.
type Change = (&'static str, &'static str, String);

/// What to set for a factor, or to restore with `None`.
///
/// The factor is set before the magnifier is switched on, so it never opens
/// at the wrong level, and switched off before the factor is put back, so the
/// user never sees their own level flash up.
fn plan(saved: &Saved, factor: Option<f64>) -> Vec<Change> {
    match factor {
        Some(factor) => vec![
            (MAGNIFIER, FACTOR, format!("{factor:.1}")),
            (APPLICATIONS, ENABLED, "true".to_owned()),
        ],
        None => vec![
            (APPLICATIONS, ENABLED, saved.enabled.clone()),
            (MAGNIFIER, FACTOR, saved.factor.clone()),
        ],
    }
}

fn encode(saved: &Saved) -> String {
    format!("{}\n{}\n", saved.enabled, saved.factor)
}

/// Refuses anything but two plausible values, so a damaged file cannot be used
/// to write something arbitrary into the user's settings.
fn decode(text: &str) -> Option<Saved> {
    let mut lines = text.lines().map(str::trim);
    let enabled = lines.next()?;
    let factor = lines.next()?;
    if !matches!(enabled, "true" | "false") {
        return None;
    }
    let value: f64 = factor.parse().ok()?;
    if !(1.0..=32.0).contains(&value) {
        return None;
    }
    Some(Saved {
        enabled: enabled.to_owned(),
        factor: factor.to_owned(),
    })
}

fn get(schema: &str, key: &str) -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn apply(changes: &[Change]) -> Result<(), String> {
    for (schema, key, value) in changes {
        let status = Command::new("gsettings")
            .args(["set", schema, key, value])
            .status()
            .map_err(|e| format!("gsettings could not be run: {e}"))?;
        if !status.success() {
            return Err(format!("gsettings set {schema} {key} {value} failed"));
        }
    }
    Ok(())
}

fn restore_path() -> Option<PathBuf> {
    let session = ink_storage::default_session_path().ok()?;
    Some(session.with_file_name("magnifier-restore"))
}

/// Puts back values a previous run left in the restore file, if any.
fn recover(path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    match decode(&text) {
        Some(saved) => match apply(&plan(&saved, None)) {
            Ok(()) => {
                eprintln!(
                    "[zoom] a previous run ended while zoomed; your magnifier settings are \
                     restored (on: {}, factor: {})",
                    saved.enabled, saved.factor
                );
                let _ = std::fs::remove_file(path);
            }
            Err(error) => eprintln!("[zoom] could not restore a previous run's settings: {error}"),
        },
        None => {
            eprintln!(
                "[zoom] {} is not a restore file this version wrote; left alone",
                path.display()
            );
        }
    }
}

/// What `doctor` reports for live zoom here.
///
/// `Available` needs GNOME and the magnifier's settings, which is the
/// configuration E017 confirmed. Anywhere else there is no magnifier yappyink
/// can drive, and that is reported as unavailable with the reason.
pub fn state() -> ink_platform::CapabilityState {
    use ink_platform::CapabilityState;
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop
        .split(':')
        .any(|part| part.eq_ignore_ascii_case("GNOME"))
    {
        return CapabilityState::unavailable(format!(
            "no GNOME Shell magnifier: the desktop is {desktop:?}, and ADR-008 has no route \
             for it"
        ));
    }
    match get(APPLICATIONS, ENABLED) {
        Some(_) => CapabilityState::available(
            "E017: the GNOME Shell magnifier's settings are present here, and on GNOME 46 \
             switching them zoomed live with drawing intact",
        ),
        None => CapabilityState::unavailable(
            "GNOME, but `gsettings` or the magnifier schema is missing",
        ),
    }
}

enum Request {
    Set(Option<f64>),
    Shutdown,
}

/// The GNOME Shell magnifier, driven from a worker thread.
pub struct Magnifier {
    requests: Sender<Request>,
    worker: Option<JoinHandle<()>>,
}

impl Magnifier {
    /// Finds the magnifier, restoring anything a previous run left behind.
    ///
    /// `None` off GNOME, or where `gsettings` or the schema is missing: the
    /// setting would do nothing there, and a zoom button that does nothing is
    /// what FR-029 rules out. GNOME is recognised from `XDG_CURRENT_DESKTOP`,
    /// because the magnifier belongs to the Shell and the same schema is
    /// installed on desktops that ignore it.
    pub fn probe() -> Option<Self> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if !desktop
            .split(':')
            .any(|part| part.eq_ignore_ascii_case("GNOME"))
        {
            return None;
        }
        get(APPLICATIONS, ENABLED)?;
        let restore = restore_path();
        if let Some(path) = &restore {
            recover(path);
        }
        let (requests, inbox) = channel();
        let worker = std::thread::Builder::new()
            .name("magnifier".to_owned())
            .spawn(move || serve(&inbox, restore.as_deref()))
            .ok()?;
        Some(Self {
            requests,
            worker: Some(worker),
        })
    }

    /// Zooms to a factor, or back to the user's own settings with `None`.
    /// Returns at once; the worker does the work.
    pub fn set(&self, factor: Option<f64>) {
        let _ = self.requests.send(Request::Set(factor));
    }
}

impl Drop for Magnifier {
    /// Restores the user's settings on a normal exit. Waits for it, briefly,
    /// because leaving the screen magnified after quitting is the failure this
    /// module most needs to avoid.
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(inbox: &Receiver<Request>, restore: Option<&Path>) {
    let mut saved: Option<Saved> = None;
    while let Ok(request) = inbox.recv() {
        let factor = match request {
            Request::Set(factor) => factor,
            Request::Shutdown => None,
        };
        let shutdown = matches!(request, Request::Shutdown);

        if factor.is_some() && saved.is_none() {
            let (Some(enabled), Some(level)) = (get(APPLICATIONS, ENABLED), get(MAGNIFIER, FACTOR))
            else {
                eprintln!("[zoom] the magnifier settings could not be read; not zooming");
                continue;
            };
            let current = Saved {
                enabled,
                factor: level,
            };
            if let Some(path) = restore
                && let Err(error) = std::fs::write(path, encode(&current))
            {
                // Without the file a crash would leave the screen magnified,
                // so refuse to start rather than hope.
                eprintln!(
                    "[zoom] could not write {}: {error}; not zooming",
                    path.display()
                );
                continue;
            }
            saved = Some(current);
        }

        match (factor, &saved) {
            (Some(_), Some(original)) | (None, Some(original)) => {
                match apply(&plan(original, factor)) {
                    Ok(()) if factor.is_none() => {
                        eprintln!("[zoom] off; your magnifier settings are restored");
                        if let Some(path) = restore {
                            let _ = std::fs::remove_file(path);
                        }
                        saved = None;
                    }
                    Ok(()) => eprintln!("[zoom] {:.0}x", factor.unwrap_or(1.0)),
                    Err(error) => eprintln!("[zoom] {error}"),
                }
            }
            // Asked to turn off while never zoomed: nothing to restore.
            (None, None) | (Some(_), None) => {}
        }

        if shutdown {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(enabled: &str, factor: &str) -> Saved {
        Saved {
            enabled: enabled.to_owned(),
            factor: factor.to_owned(),
        }
    }

    #[test]
    fn zooming_sets_the_factor_before_switching_on() {
        let changes = plan(&saved("false", "2.0"), Some(3.0));
        assert_eq!(
            changes,
            vec![
                (MAGNIFIER, FACTOR, "3.0".to_owned()),
                (APPLICATIONS, ENABLED, "true".to_owned()),
            ]
        );
    }

    /// Off means the user's own state, not "false": someone who uses the
    /// magnifier themselves must get it back as they had it.
    #[test]
    fn off_restores_exactly_what_the_user_had() {
        let changes = plan(&saved("true", "1.5"), None);
        assert_eq!(
            changes,
            vec![
                (APPLICATIONS, ENABLED, "true".to_owned()),
                (MAGNIFIER, FACTOR, "1.5".to_owned()),
            ]
        );
    }

    #[test]
    fn the_restore_file_round_trips() {
        let original = saved("false", "2.0");
        assert_eq!(decode(&encode(&original)), Some(original));
    }

    /// The file is read at startup and written into the user's settings, so
    /// anything unexpected in it is refused rather than applied.
    #[test]
    fn a_damaged_restore_file_is_refused() {
        for text in [
            "",
            "false\n",
            "yes\n2.0\n",
            "false\nlots\n",
            "false\n0.0\n",
            "false\n1000\n",
            "false\n2.0; rm -rf ~\n",
        ] {
            assert_eq!(decode(text), None, "{text:?} was accepted");
        }
    }
}
