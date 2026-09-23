//! Capability vocabulary (FR-013).
//!
//! Constitution §4: `Unknown`, `Available`, `NeedsUserAction`, and `Unavailable`
//! are distinct, each unsuccessful state carries a reason, and runtime detection
//! is not release certification.

use std::fmt;

/// The capabilities a backend can be asked about.
///
/// The list is `platform-matrix.md` "Capability vocabulary" verbatim. A backend
/// that has no opinion about one reports `Unknown`, never a default of
/// available.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Ink is drawn above other applications while they keep updating.
    LiveOverlay,
    /// Draw mode receives pointer input over the whole output, including
    /// visually transparent pixels.
    DrawPointerCapture,
    /// PassThrough leaves ink visible while input belongs to applications
    /// underneath, without forwarding or synthesising anything.
    VisiblePassthrough,
    /// The overlay can give up keyboard ownership. Separate from pointer
    /// transparency.
    KeyboardRelease,
    /// A global activation chord can be registered or routed.
    GlobalShortcut,
    /// Outputs and their scale/geometry can be enumerated.
    OutputEnumeration,
    /// Several outputs can host overlays at the same time (V1, FR-021).
    SimultaneousOutputs,
    /// Overlay survives above a fullscreen application.
    FullscreenOverlay,
    /// Overlay behaves across workspaces/Spaces.
    WorkspaceOverlay,
    /// Screen capture (later feature, FR-026). Never required for drawing.
    Capture,
    /// Capture can exclude our own surfaces, avoiding self-capture (FR-027).
    CaptureExclusion,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveOverlay => "live_overlay",
            Self::DrawPointerCapture => "draw_pointer_capture",
            Self::VisiblePassthrough => "visible_passthrough",
            Self::KeyboardRelease => "keyboard_release",
            Self::GlobalShortcut => "global_shortcut",
            Self::OutputEnumeration => "output_enumeration",
            Self::SimultaneousOutputs => "simultaneous_outputs",
            Self::FullscreenOverlay => "fullscreen_overlay",
            Self::WorkspaceOverlay => "workspace_overlay",
            Self::Capture => "capture",
            Self::CaptureExclusion => "capture_exclusion",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What is known about one capability right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityState {
    /// Nothing has been established. Carries what stopped the probe, so a
    /// diagnostics reader can tell "we did not look" from "we looked and it is
    /// not there".
    Unknown { not_probed_because: String },
    /// Observed to work, with the observation that supports it.
    Available { evidence: String },
    /// Reachable only after the user acts: granting a portal permission,
    /// binding a compositor shortcut, installing a companion.
    NeedsUserAction { action: String },
    /// Observed to be absent or refused, with the reason.
    Unavailable { reason: String },
}

impl CapabilityState {
    pub fn unknown(not_probed_because: impl Into<String>) -> Self {
        Self::Unknown {
            not_probed_because: not_probed_because.into(),
        }
    }

    pub fn available(evidence: impl Into<String>) -> Self {
        Self::Available {
            evidence: evidence.into(),
        }
    }

    pub fn needs_user_action(action: impl Into<String>) -> Self {
        Self::NeedsUserAction {
            action: action.into(),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    /// True only for [`CapabilityState::Available`].
    ///
    /// `Unknown` is deliberately false here: an unprobed capability must never
    /// be treated as usable (constitution §4). This is also why there is no
    /// `Default` impl for this type.
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }

    /// The short word used in diagnostics output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unknown { .. } => "unknown",
            Self::Available { .. } => "available",
            Self::NeedsUserAction { .. } => "needs_user_action",
            Self::Unavailable { .. } => "unavailable",
        }
    }

    /// The explanatory text, whichever variant carries it.
    pub fn detail(&self) -> &str {
        match self {
            Self::Unknown { not_probed_because } => not_probed_because,
            Self::Available { evidence } => evidence,
            Self::NeedsUserAction { action } => action,
            Self::Unavailable { reason } => reason,
        }
    }
}

/// One capability plus the observation behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityFinding {
    pub capability: Capability,
    pub state: CapabilityState,
    /// Which adapter produced this, e.g. `wayland`, `x11`, `none`.
    pub backend: String,
}

impl CapabilityFinding {
    pub fn new(capability: Capability, state: CapabilityState, backend: impl Into<String>) -> Self {
        Self {
            capability,
            state,
            backend: backend.into(),
        }
    }
}

/// Every capability finding from one probe run.
///
/// A capability that was never asked about is reported as `Unknown` by
/// [`CapabilityReport::state_of`] rather than being silently missing, so a
/// reader cannot mistake absence for a negative result.
#[derive(Clone, Debug, Default)]
pub struct CapabilityReport {
    findings: Vec<CapabilityFinding>,
}

impl CapabilityReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a finding. A later finding for the same capability replaces the
    /// earlier one, so an adapter can refine what the session probe assumed.
    pub fn record(&mut self, finding: CapabilityFinding) {
        match self
            .findings
            .iter_mut()
            .find(|f| f.capability == finding.capability)
        {
            Some(existing) => *existing = finding,
            None => self.findings.push(finding),
        }
    }

    pub fn state_of(&self, capability: Capability) -> CapabilityState {
        self.findings
            .iter()
            .find(|f| f.capability == capability)
            .map(|f| f.state.clone())
            .unwrap_or_else(|| CapabilityState::unknown("no adapter reported on this capability"))
    }

    pub fn findings(&self) -> &[CapabilityFinding] {
        &self.findings
    }

    /// Capabilities that no adapter has reported on.
    pub fn unreported(&self, all: &[Capability]) -> Vec<Capability> {
        all.iter()
            .copied()
            .filter(|c| !self.findings.iter().any(|f| f.capability == *c))
            .collect()
    }
}
