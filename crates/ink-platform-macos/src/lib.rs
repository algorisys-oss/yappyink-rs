//! The macOS adapter (T004).
//!
//! # How this crate is split, and why
//!
//! The same split as `ink-platform-windows`, for the same reason and with more
//! force: development happens on Linux and **nobody on this project has a Mac**,
//! so anything that needs AppKit to run cannot be checked here at all.
//!
//! - [`keys`] turns an AppKit character into an [`ink_app::keymap::Key`]. What
//!   the key then means is decided by `ink_app::keymap`, shared with every
//!   backend.
//! - [`surface`] is bitmap-layout arithmetic and the backing scale.
//! - [`chords`] is the global shortcut's Carbon constants and what each chord
//!   does. A wrong constant registers the wrong chord silently, which is why
//!   they are tested rather than trusted.
//!
//! Both compile and run their tests anywhere. What is left in `overlay` is a
//! window, an event loop and a bitmap handed to Core Graphics, and that is
//! genuinely unverifiable without the hardware.
//!
//! **Nothing here has been run.** Every capability this crate reports is
//! `unknown`, and `docs/adr/ADR-006-macos-bindings.md` records that if that
//! never changes the honest outcome is to declare macOS unsupported rather
//! than ship it quietly.

pub mod chords;
pub mod keys;
pub mod surface;

#[cfg(target_os = "macos")]
mod hotkey;
#[cfg(target_os = "macos")]
pub mod overlay;

use ink_platform::{Capability, CapabilityFinding, CapabilityReport, CapabilityState};

/// The backend name used in capability findings.
pub const BACKEND: &str = "macos-appkit";

/// What this backend could offer, and what is actually known about it.
///
/// Every state is `Unknown`. The constitution distinguishes "we did not probe
/// this" from "this does not work", and reporting documented behaviour as
/// `Available` would be inventing a measurement.
pub fn capabilities() -> CapabilityReport {
    let mut report = CapabilityReport::new();
    let unproven = [
        (
            Capability::LiveOverlay,
            "a borderless NSWindow at screen-saver level with a clear background should \
             composite above other applications; nobody has watched it",
        ),
        (
            Capability::DrawPointerCapture,
            "with ignoresMouseEvents off, the view should receive clicks on transparent \
             pixels; nobody has watched it",
        ),
        (
            Capability::VisiblePassthrough,
            "setIgnoresMouseEvents: should let AppKit route clicks underneath with nothing \
             forwarded by us; nobody has watched it",
        ),
        (
            Capability::KeyboardRelease,
            "resigning key window should return the keyboard, but an accessory application \
             that accepts keys takes focus in the first place; unresolved, see ADR-006",
        ),
        (
            Capability::GlobalShortcut,
            "Carbon's RegisterEventHotKey binds Control+Option+D and Control+Option+H \
             without an accessibility grant (ADR-007); nobody has pressed them",
        ),
        (
            Capability::OutputEnumeration,
            "NSScreen should report every screen with its backing scale; nobody has watched it",
        ),
        (
            Capability::SimultaneousOutputs,
            "one window per screen should be possible; no second window has been created",
        ),
        (
            Capability::FullscreenOverlay,
            "NSWindowCollectionBehavior is set to join all Spaces and act as a fullscreen \
             auxiliary, which is the documented route; whether it is honoured at this window \
             level is the single most uncertain thing in this backend",
        ),
        (
            Capability::WorkspaceOverlay,
            "Spaces are covered by the same collection behaviour and are equally unmeasured",
        ),
        (
            Capability::Capture,
            "out of scope for this backend; capture is ADR-003 and deliberately unbuilt",
        ),
        (
            Capability::CaptureExclusion,
            "out of scope for this backend; capture is ADR-003 and deliberately unbuilt",
        ),
    ];
    for (capability, reason) in unproven {
        report.record(CapabilityFinding::new(
            capability,
            CapabilityState::unknown(reason),
            BACKEND,
        ));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The honesty rule, enforced rather than trusted.
    ///
    /// The temptation once a Mac appears will be to mark these `Available`
    /// from the documentation rather than from what was seen. This fails if
    /// anyone does, which makes them delete it deliberately and, ideally,
    /// attach an evidence file.
    #[test]
    fn nothing_is_claimed_before_it_is_measured() {
        for finding in capabilities().findings() {
            assert!(
                matches!(finding.state, CapabilityState::Unknown { .. }),
                "{:?} claims {:?}, but no Mac has run this code. If that has changed, \
                 cite the evidence file when you change this test.",
                finding.capability,
                finding.state
            );
        }
    }

    #[test]
    fn every_capability_is_accounted_for() {
        assert_eq!(
            capabilities().findings().len(),
            11,
            "a capability was added or dropped without a reason being written"
        );
    }
}
