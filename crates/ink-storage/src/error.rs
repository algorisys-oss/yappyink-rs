//! Typed storage failures.
//!
//! Every variant says what was refused and why. A user whose document did not
//! load needs to know whether the file is corrupt, too new, or simply missing
//! (NFR-005).

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum StorageError {
    /// The file could not be read or written.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file is larger than the limit, so it was not even parsed.
    TooLarge {
        path: PathBuf,
        bytes: u64,
        limit: u64,
    },
    /// The bytes are not the JSON this format expects.
    Malformed { path: PathBuf, reason: String },
    /// Written by a newer version of the application.
    ///
    /// Deliberately its own variant: this is the one failure where the file is
    /// probably fine and the *application* is out of date, and telling the user
    /// to upgrade is different advice from telling them the file is broken.
    UnsupportedVersion {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    /// The structure parsed but the contents are not a usable document.
    Invalid { what: String, reason: String },
}

impl StorageError {
    /// A stable machine-readable class, for diagnostics and tests.
    pub fn class(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::TooLarge { .. } => "too_large",
            Self::Malformed { .. } => "malformed",
            Self::UnsupportedVersion { .. } => "unsupported_version",
            Self::Invalid { .. } => "invalid",
        }
    }

    pub(crate) fn invalid(what: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Invalid {
            what: what.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::TooLarge { path, bytes, limit } => write!(
                f,
                "{} is {bytes} bytes, over the {limit} byte limit, and was not read",
                path.display()
            ),
            Self::Malformed { path, reason } => {
                write!(f, "{} is not a valid document: {reason}", path.display())
            }
            Self::UnsupportedVersion {
                path,
                found,
                supported,
            } => write!(
                f,
                "{} was written by a newer version (schema {found}, this build understands \
                 {supported}). Upgrade rather than editing the file.",
                path.display()
            ),
            Self::Invalid { what, reason } => write!(f, "{what} is not valid: {reason}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
