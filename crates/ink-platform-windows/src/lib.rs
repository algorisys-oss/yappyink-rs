//! The Windows adapter (T003).
//!
//! # How this crate is split, and why
//!
//! Development happens on Linux. An adapter written entirely in `unsafe` Win32
//! calls would be untestable here in its entirety, and the Wayland adapter has
//! already shown what that costs: it has no tests, it cannot easily have any,
//! and six bugs were found by a person using the application rather than by the
//! suite (`docs/learning.md` §1).
//!
//! So the decidable parts are separated out and take no Windows types at all:
//!
//! - [`keys`] turns a virtual-key code into an [`ink_app::Action`]. A key code
//!   is a `u32`, and what it *means* is this project's decision, not the
//!   platform's.
//! - [`surface`] is buffer-layout arithmetic: stride, top-down height, the DPI
//!   scale.
//!
//! Both compile and run their tests on any machine. What is left in `overlay`
//! is the irreducible part — creating a window, pumping messages, handing
//! pixels to the window manager — and that genuinely cannot be checked without
//! Windows.
//!
//! **That split is a mitigation, not a substitute.** Nothing here has been seen
//! working. Every capability this crate reports is `unknown`.

pub mod keys;
pub mod surface;

#[cfg(windows)]
pub mod overlay;

use ink_platform::{Capability, CapabilityFinding, CapabilityReport, CapabilityState};

/// The backend name used in capability findings.
pub const BACKEND: &str = "windows-layered";

/// What this backend could offer, and what is actually known about it.
///
/// Every state here is `Unknown`. That is not modesty and it is not a
/// placeholder: the constitution distinguishes "we did not probe this" from
/// "this does not work", and reporting a documented behaviour as `Available`
/// would be claiming a measurement that nobody has taken.
///
/// The reasons name what the mechanism *would* be, so a reader can see the plan
/// without mistaking it for a result. They become `Available` one at a time, as
/// evidence files land.
pub fn capabilities() -> CapabilityReport {
    let mut report = CapabilityReport::new();
    let unproven = [
        (
            Capability::LiveOverlay,
            "a WS_EX_LAYERED window painted with UpdateLayeredWindow should composite \
             per-pixel alpha above other applications; nobody has watched it",
        ),
        (
            Capability::DrawPointerCapture,
            "without WS_EX_TRANSPARENT the window should receive clicks on transparent \
             pixels; nobody has watched it",
        ),
        (
            Capability::VisiblePassthrough,
            "WS_EX_TRANSPARENT should let the window manager route clicks underneath \
             with nothing forwarded by us; nobody has watched it",
        ),
        (
            Capability::KeyboardRelease,
            "dropping the foreground window should return the keyboard; nobody has \
             watched it",
        ),
        (
            Capability::GlobalShortcut,
            "RegisterHotKey should bind a chord and refuse one another application owns; \
             nobody has watched it",
        ),
        (
            Capability::OutputEnumeration,
            "EnumDisplayMonitors should report every monitor with its own DPI; nobody \
             has watched it",
        ),
        (
            Capability::SimultaneousOutputs,
            "one window per monitor should be possible, but no second window has been \
             created",
        ),
        (
            Capability::FullscreenOverlay,
            "a top-most window is expected to lose to an exclusive-fullscreen \
             application; entirely unmeasured and the most likely to disappoint",
        ),
        (
            Capability::WorkspaceOverlay,
            "Windows virtual desktops have not been considered at all",
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
    /// It would be very easy, once the probe comes back positive, to mark these
    /// `Available` from the documentation instead of from evidence. This test
    /// fails the moment anyone does, which forces them to delete it
    /// deliberately and, ideally, to attach an evidence file when they do.
    #[test]
    fn nothing_is_claimed_before_it_is_measured() {
        for finding in capabilities().findings() {
            assert!(
                matches!(finding.state, CapabilityState::Unknown { .. }),
                "{:?} claims {:?}, but no Windows machine has run this code. \
                 If that has changed, cite the evidence file when you change this test.",
                finding.capability,
                finding.state
            );
        }
    }

    #[test]
    fn every_capability_is_accounted_for() {
        // A capability missing from the report reads as "not applicable" when
        // it actually means "nobody thought about it".
        assert_eq!(
            capabilities().findings().len(),
            11,
            "a capability was added or dropped without a reason being written"
        );
    }
}
