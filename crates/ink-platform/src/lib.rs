//! Typed contracts between the application and its platform adapters.
//!
//! This crate holds the vocabulary: what can be asked of a backend
//! ([`Capability`]), what is known about it ([`CapabilityState`]), how a native
//! operation fails ([`PlatformError`]), and which display server answered
//! ([`session`]). Adapters such as `ink-platform-wayland` implement against it.
//!
//! No toolkit, window, or GPU type belongs here, and nothing in this crate may
//! report a capability as available without an observation behind it.

#![forbid(unsafe_code)]

pub mod capability;
pub mod error;
pub mod session;

pub use capability::{Capability, CapabilityFinding, CapabilityReport, CapabilityState};
pub use error::PlatformError;
pub use session::{EnvSnapshot, PlatformSocketProbe, SessionKind, SocketProbe, detect_session};

/// One display output as a probe observed it.
///
/// Fields are optional because protocols deliver them separately and some
/// versions never send a name at all. A missing value is `None`, never a made
/// up default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputInfo {
    /// The compositor's name for the output, e.g. `DP-1`. Requires
    /// `wl_output` version 4 on Wayland.
    pub name: Option<String>,
    pub description: Option<String>,
    /// Physical resolution in pixels.
    pub resolution: Option<(i32, i32)>,
    pub refresh_mhz: Option<i32>,
    /// Integer scale factor. Fractional scaling is reported separately by the
    /// compositor and is not established by this value.
    pub scale: Option<i32>,
    /// Transform as the protocol reports it, e.g. `normal`, `90`.
    pub transform: Option<String>,
}
