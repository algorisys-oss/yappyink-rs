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
//! - [`keys`] turns a virtual-key code into an [`ink_app::keymap::Key`]. What
//!   the key then *means* is decided by `ink_app::keymap`, shared with every
//!   backend, because three copies of "what does `d` do" is three chances to
//!   disagree.
//! - [`surface`] is buffer-layout arithmetic: stride, top-down height, the DPI
//!   scale.
//!
//! Both compile and run their tests on any machine. What is left in `overlay`
//! is the irreducible part — creating a window, pumping messages, handing
//! pixels to the window manager — and that genuinely cannot be checked without
//! Windows.
//!
//! **That split is a mitigation, not a substitute.** Five capabilities have
//! been seen working on one machine (E010, E012, E014); every other one is still
//! `unknown`.

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
    // Observed on one machine, 0.7.0, Windows with two monitors at 96 dpi:
    // strokes, shapes, a highlighter and text drawn over a terminal, in areas
    // that had been fully transparent. See docs/evidence/E010.
    report.record(CapabilityFinding::new(
        Capability::LiveOverlay,
        CapabilityState::available(
            "E010: ink drawn with UpdateLayeredWindow stayed visible above a terminal \
             window, on one machine at 96 dpi",
        ),
        BACKEND,
    ));
    report.record(CapabilityFinding::new(
        Capability::DrawPointerCapture,
        CapabilityState::available(
            "E010: clicks on empty canvas drew strokes rather than reaching the terminal \
             underneath, which is the alpha-1 floor doing its job",
        ),
        BACKEND,
    ));
    report.record(CapabilityFinding::new(
        Capability::VisiblePassthrough,
        CapabilityState::available(
            "E012: with WS_EX_TRANSPARENT set, ink stayed visible and clicks reached the \
             application underneath, as reported by the owner",
        ),
        BACKEND,
    ));
    report.record(CapabilityFinding::new(
        Capability::GlobalShortcut,
        CapabilityState::available(
            "E012: Ctrl+Alt+D, registered with RegisterHotKey, brought the overlay back from \
             pass-through while another application had focus",
        ),
        BACKEND,
    ));
    report.record(CapabilityFinding::new(
        Capability::FullscreenOverlay,
        CapabilityState::available(
            "E014: after an application went fullscreen the overlay could still be toggled and \
             drawn on; which kind of fullscreen was not stated, so exclusive DirectX is untested",
        ),
        BACKEND,
    ));
    let unproven = [
        (
            Capability::LiveZoom,
            "the Magnification API's full-screen transform should zoom without capture \
             (ADR-008); not implemented, T038",
        ),
        (
            Capability::KeyboardRelease,
            "dropping the foreground window should return the keyboard; nobody has \
             watched it",
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
    /// Exactly the capabilities an evidence file supports may be `Available`,
    /// and each must cite it. Everything else stays `Unknown`. Adding to the
    /// list below means adding the evidence first.
    #[test]
    fn nothing_is_claimed_before_it_is_measured() {
        let observed = [
            (Capability::LiveOverlay, "E010"),
            (Capability::DrawPointerCapture, "E010"),
            (Capability::VisiblePassthrough, "E012"),
            (Capability::GlobalShortcut, "E012"),
            (Capability::FullscreenOverlay, "E014"),
        ];
        for finding in capabilities().findings() {
            match &finding.state {
                CapabilityState::Available { evidence } => {
                    let cited = observed
                        .iter()
                        .find(|(capability, _)| *capability == finding.capability)
                        .map(|(_, file)| *file);
                    let Some(file) = cited else {
                        panic!(
                            "{:?} claims availability with no evidence file",
                            finding.capability
                        );
                    };
                    assert!(evidence.starts_with(file), "{evidence}");
                }
                CapabilityState::Unknown { .. } => {}
                other => panic!("{:?} claims {other:?} without evidence", finding.capability),
            }
        }
    }

    #[test]
    fn every_capability_is_accounted_for() {
        // A capability missing from the report reads as "not applicable" when
        // it actually means "nobody thought about it".
        assert_eq!(
            capabilities().findings().len(),
            12,
            "a capability was added or dropped without a reason being written"
        );
    }
}
