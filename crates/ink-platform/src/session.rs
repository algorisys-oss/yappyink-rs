//! Which display server is actually reachable (FR-013, FR-020).
//!
//! The rule from `platform-matrix.md` is that detection is based on what
//! answers, not on a desktop name string. So this module resolves the socket a
//! variable points at and tries to connect to it. `XDG_CURRENT_DESKTOP` is
//! recorded as context for a human reading a report and never decides anything.
//!
//! Environment and socket access are injected so the tests are headless and
//! deterministic (NFR-004).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The display server this process can actually talk to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionKind {
    /// A Wayland socket accepted a connection.
    Wayland { socket: PathBuf },
    /// An X11 display socket accepted a connection.
    X11 { display: String, socket: PathBuf },
    /// Nothing answered. `reasons` says what was tried and what happened; it is
    /// never empty.
    None { reasons: Vec<String> },
}

impl SessionKind {
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Wayland { .. } => "wayland",
            Self::X11 { .. } => "x11",
            Self::None { .. } => "none",
        }
    }
}

/// A snapshot of the environment variables the probe is allowed to read.
#[derive(Clone, Debug, Default)]
pub struct EnvSnapshot {
    vars: BTreeMap<String, String>,
}

impl EnvSnapshot {
    /// Reads the variables this probe uses from the real process environment.
    pub fn from_process_env() -> Self {
        const READ: [&str; 6] = [
            "WAYLAND_DISPLAY",
            "WAYLAND_SOCKET",
            "XDG_RUNTIME_DIR",
            "DISPLAY",
            "XDG_SESSION_TYPE",
            "XDG_CURRENT_DESKTOP",
        ];
        let mut vars = BTreeMap::new();
        for key in READ {
            if let Ok(value) = std::env::var(key) {
                vars.insert(key.to_owned(), value);
            }
        }
        Self { vars }
    }

    /// Builds a snapshot for tests.
    pub fn from_pairs<K: Into<String>, V: Into<String>>(
        pairs: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        Self {
            vars: pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    /// The desktop name, for display in a report only. Nothing branches on it.
    pub fn reported_desktop(&self) -> Option<&str> {
        self.get("XDG_CURRENT_DESKTOP")
    }

    /// What the session claims to be, for display only. The probe result may
    /// contradict it, and the probe result wins.
    pub fn claimed_session_type(&self) -> Option<&str> {
        self.get("XDG_SESSION_TYPE")
    }
}

/// Can a socket path be connected to?
///
/// Implemented over `UnixStream` in production and faked in tests.
pub trait SocketProbe {
    /// `Ok(())` when a connection was established and immediately dropped.
    fn can_connect(&self, path: &Path) -> Result<(), String>;
}

/// Connects with `std::os::unix::net::UnixStream`.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UnixSocketProbe;

#[cfg(unix)]
impl SocketProbe for UnixSocketProbe {
    fn can_connect(&self, path: &Path) -> Result<(), String> {
        std::os::unix::net::UnixStream::connect(path)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Resolves the reachable display server.
///
/// Wayland is tried first: on a session that runs both, the Wayland socket is
/// the native one and XWayland is a compatibility path, which is a distinction
/// the overlay work depends on.
pub fn detect_session(env: &EnvSnapshot, probe: &dyn SocketProbe) -> SessionKind {
    let mut reasons = Vec::new();

    match wayland_socket_path(env) {
        Ok(path) => match probe.can_connect(&path) {
            Ok(()) => return SessionKind::Wayland { socket: path },
            Err(e) => reasons.push(format!(
                "wayland: {} did not accept a connection: {e}",
                path.display()
            )),
        },
        Err(reason) => reasons.push(format!("wayland: {reason}")),
    }

    match x11_socket_path(env) {
        Ok((display, path)) => match probe.can_connect(&path) {
            Ok(()) => {
                return SessionKind::X11 {
                    display,
                    socket: path,
                };
            }
            Err(e) => reasons.push(format!(
                "x11: {} did not accept a connection: {e}",
                path.display()
            )),
        },
        Err(reason) => reasons.push(format!("x11: {reason}")),
    }

    SessionKind::None { reasons }
}

/// Where `WAYLAND_DISPLAY` points, following the protocol's own rule: an
/// absolute path is used as-is, a bare name is relative to `XDG_RUNTIME_DIR`.
fn wayland_socket_path(env: &EnvSnapshot) -> Result<PathBuf, String> {
    if env.get("WAYLAND_SOCKET").is_some() {
        // An inherited file descriptor, not a path. We cannot re-connect to it
        // and must not claim the session is absent.
        return Err(
            "WAYLAND_SOCKET is set, so the display is an inherited fd this probe cannot reopen"
                .to_owned(),
        );
    }
    let display = env
        .get("WAYLAND_DISPLAY")
        .ok_or("WAYLAND_DISPLAY is not set")?;
    if display.is_empty() {
        return Err("WAYLAND_DISPLAY is empty".to_owned());
    }
    let path = Path::new(display);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let runtime_dir = env.get("XDG_RUNTIME_DIR").ok_or_else(|| {
        format!("WAYLAND_DISPLAY is the relative name {display:?} but XDG_RUNTIME_DIR is not set")
    })?;
    Ok(Path::new(runtime_dir).join(path))
}

/// The abstract-free Unix socket for a local `DISPLAY` such as `:0` or `:1.0`.
///
/// A remote or TCP display returns an error: this probe does not open network
/// sockets.
fn x11_socket_path(env: &EnvSnapshot) -> Result<(String, PathBuf), String> {
    let display = env.get("DISPLAY").ok_or("DISPLAY is not set")?;
    let rest = display.strip_prefix(':').ok_or_else(|| {
        format!("DISPLAY {display:?} is not a local display; this probe does not open TCP sockets")
    })?;
    let number = rest.split('.').next().unwrap_or_default();
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("DISPLAY {display:?} has no usable display number"));
    }
    Ok((
        display.to_owned(),
        PathBuf::from(format!("/tmp/.X11-unix/X{number}")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_wayland_display_resolves_against_the_runtime_dir() {
        let env = EnvSnapshot::from_pairs([
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_RUNTIME_DIR", "/run/user/1001"),
        ]);
        assert_eq!(
            wayland_socket_path(&env).unwrap(),
            PathBuf::from("/run/user/1001/wayland-0")
        );
    }

    #[test]
    fn absolute_wayland_display_is_used_as_is() {
        let env = EnvSnapshot::from_pairs([("WAYLAND_DISPLAY", "/custom/wl.sock")]);
        assert_eq!(
            wayland_socket_path(&env).unwrap(),
            PathBuf::from("/custom/wl.sock")
        );
    }

    #[test]
    fn x11_display_number_is_taken_from_before_the_screen() {
        let env = EnvSnapshot::from_pairs([("DISPLAY", ":1.0")]);
        let (display, path) = x11_socket_path(&env).unwrap();
        assert_eq!(display, ":1.0");
        assert_eq!(path, PathBuf::from("/tmp/.X11-unix/X1"));
    }

    #[test]
    fn remote_x11_display_is_refused_rather_than_guessed() {
        let env = EnvSnapshot::from_pairs([("DISPLAY", "host:0")]);
        assert!(x11_socket_path(&env).is_err());
    }
}
