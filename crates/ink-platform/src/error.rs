//! Typed platform failures (NFR-005).
//!
//! Each class stays distinguishable so callers can act on it: an unsupported
//! protocol is not a denied permission, and neither may be reported as success.

use std::fmt;

/// A platform operation that did not succeed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformError {
    /// The backend has no way to do this: a missing protocol, an absent API.
    Unsupported { operation: String, reason: String },
    /// The user or the system refused. Retrying without user action will fail
    /// the same way.
    PermissionDenied { operation: String, reason: String },
    /// A requested global binding is already taken.
    ShortcutConflict { binding: String, holder: String },
    /// The compositor, display server, or portal connection went away.
    Disconnected { reason: String },
    /// A native surface was lost and must be recreated (FR-019).
    SurfaceLost { reason: String },
    /// Data failed validation: a document, a settings value, a protocol reply
    /// (FR-017).
    InvalidData { what: String, reason: String },
}

impl PlatformError {
    /// A stable machine-readable class, for diagnostics and tests.
    pub fn class(&self) -> &'static str {
        match self {
            Self::Unsupported { .. } => "unsupported",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::ShortcutConflict { .. } => "shortcut_conflict",
            Self::Disconnected { .. } => "disconnected",
            Self::SurfaceLost { .. } => "surface_lost",
            Self::InvalidData { .. } => "invalid_data",
        }
    }

    /// Whether the same call could succeed later without the user doing
    /// anything. Used to decide between retrying and reporting.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Disconnected { .. } | Self::SurfaceLost { .. })
    }

    /// Whether the user can unblock this themselves.
    pub fn needs_user_action(&self) -> bool {
        matches!(
            self,
            Self::PermissionDenied { .. } | Self::ShortcutConflict { .. }
        )
    }

    pub fn unsupported(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Unsupported {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    pub fn disconnected(reason: impl Into<String>) -> Self {
        Self::Disconnected {
            reason: reason.into(),
        }
    }

    pub fn invalid_data(what: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidData {
            what: what.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { operation, reason } => {
                write!(f, "{operation} is not supported here: {reason}")
            }
            Self::PermissionDenied { operation, reason } => {
                write!(f, "{operation} was denied: {reason}")
            }
            Self::ShortcutConflict { binding, holder } => {
                write!(f, "the binding {binding} is already held by {holder}")
            }
            Self::Disconnected { reason } => write!(f, "the connection was lost: {reason}"),
            Self::SurfaceLost { reason } => write!(f, "the native surface was lost: {reason}"),
            Self::InvalidData { what, reason } => write!(f, "{what} is not valid: {reason}"),
        }
    }
}

impl std::error::Error for PlatformError {}
