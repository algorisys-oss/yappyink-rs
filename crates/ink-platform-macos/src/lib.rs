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
//! **Two capabilities have been seen working on one Mac (E011).** Every other
//! capability this crate reports is `unknown`, and `docs/adr/ADR-006-macos-bindings.md` records that if that
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
    // Observed on one Apple Silicon Mac at backing scale 2: shapes drawn over
    // a Finder window and the desktop, in areas that had been fully
    // transparent. See docs/evidence/E011.
    report.record(CapabilityFinding::new(
        Capability::LiveOverlay,
        CapabilityState::available(
            "E011: ink stayed visible above Finder and the desktop from a screen-saver-level \
             window, on one Retina Mac",
        ),
        BACKEND,
    ));
    report.record(CapabilityFinding::new(
        Capability::DrawPointerCapture,
        CapabilityState::available(
            "E011: clicks on empty canvas drew shapes rather than reaching Finder underneath",
        ),
        BACKEND,
    ));
    let unproven = [
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
    /// Exactly the capabilities an evidence file supports may be `Available`,
    /// and each must cite it. Everything else stays `Unknown`. Adding to the
    /// list below means adding the evidence first.
    #[test]
    fn nothing_is_claimed_before_it_is_measured() {
        let observed = [Capability::LiveOverlay, Capability::DrawPointerCapture];
        for finding in capabilities().findings() {
            match &finding.state {
                CapabilityState::Available { evidence } => {
                    assert!(
                        observed.contains(&finding.capability),
                        "{:?} claims availability with no evidence file",
                        finding.capability
                    );
                    assert!(evidence.starts_with("E011"), "{evidence}");
                }
                CapabilityState::Unknown { .. } => {}
                other => panic!("{:?} claims {other:?} without evidence", finding.capability),
            }
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
