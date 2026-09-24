//! The `doctor` report (FR-020, FR-013, NFR-005).
//!
//! Diagnostics carry backend names, protocol names, and error classes. They
//! must not carry desktop pixels, annotation text, or raw key streams
//! (constitution §6); at this stage none of those exist, and nothing here may
//! start collecting them.
//!
//! The report always ends with what was *not* probed, so a reader is never left
//! to assume that a silent capability is a working one.

use std::fmt::Write as _;

use ink_platform::{
    Capability, CapabilityReport, EnvSnapshot, PlatformSocketProbe, SessionKind, detect_session,
};

const ALL_CAPABILITIES: [Capability; 11] = [
    Capability::LiveOverlay,
    Capability::DrawPointerCapture,
    Capability::VisiblePassthrough,
    Capability::KeyboardRelease,
    Capability::GlobalShortcut,
    Capability::OutputEnumeration,
    Capability::SimultaneousOutputs,
    Capability::FullscreenOverlay,
    Capability::WorkspaceOverlay,
    Capability::Capture,
    Capability::CaptureExclusion,
];

/// Runs every probe available on this build and renders the report.
pub fn report() -> String {
    let env = EnvSnapshot::from_process_env();
    let session = detect_session(&env, &PlatformSocketProbe::default());

    let mut out = String::new();
    let _ = writeln!(out, "yappyink doctor {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "build target: {}", std::env::consts::OS);
    let _ = writeln!(out);

    let _ = writeln!(out, "session");
    let _ = writeln!(out, "  reachable backend: {}", session.backend_name());
    match &session {
        SessionKind::Wayland { socket } => {
            let _ = writeln!(out, "  wayland socket:    {} (connected)", socket.display());
        }
        SessionKind::X11 { display, socket } => {
            let _ = writeln!(
                out,
                "  x11 display:       {display} via {}",
                socket.display()
            );
        }
        SessionKind::None { reasons } => {
            for reason in reasons {
                let _ = writeln!(out, "  no session:        {reason}");
            }
        }
    }
    let _ = writeln!(
        out,
        "  claims to be:      XDG_SESSION_TYPE={}, XDG_CURRENT_DESKTOP={}",
        env.claimed_session_type().unwrap_or("<unset>"),
        env.reported_desktop().unwrap_or("<unset>"),
    );
    let _ = writeln!(
        out,
        "  note:              these two strings are context only. Nothing above or below was",
    );
    let _ = writeln!(
        out,
        "                     decided from them; the backend is whatever answered a connection.",
    );
    let _ = writeln!(out);

    let mut capabilities = CapabilityReport::new();
    let mut not_probed: Vec<String> = Vec::new();

    match &session {
        SessionKind::Wayland { .. } => {
            let _ = writeln!(out, "wayland protocol");
            wayland_section(&mut out, &mut capabilities, &mut not_probed);
            not_probed.push(
                "X11 and XWayland: not probed, because the native wayland socket answered first"
                    .to_owned(),
            );
        }
        SessionKind::X11 { .. } => {
            not_probed.push(
                "everything about X11: the socket answers, but no X11 adapter exists yet (T005). \
                 Compositor presence, EWMH stacking hints, and input shapes were not queried"
                    .to_owned(),
            );
        }
        SessionKind::None { .. } => {
            not_probed
                .push("every capability: no display server answered, so no adapter ran".to_owned());
        }
    }

    not_probed.push(
        "the GlobalShortcuts portal: no D-Bus client dependency is chosen yet, so the interface \
         was never queried (T012, FR-005)"
            .to_owned(),
    );
    not_probed.push(
        "Windows and macOS: this binary has never been built or run on them (T003, T004)"
            .to_owned(),
    );
    not_probed.push(
        "capture and screen recording: deliberately never probed, because drawing must not \
         depend on them (FR-014, FR-026)"
            .to_owned(),
    );

    let _ = writeln!(out);
    let _ = writeln!(out, "capabilities");
    for capability in ALL_CAPABILITIES {
        let state = capabilities.state_of(capability);
        let _ = writeln!(out, "  {:<22} {}", capability.as_str(), state.label());
        let _ = writeln!(out, "  {:<22}   because: {}", "", state.detail());
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "not probed");
    for item in &not_probed {
        let _ = writeln!(out, "  - {item}");
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "No overlay surface, input region, shortcut, or pass-through behaviour has been"
    );
    let _ = writeln!(
        out,
        "demonstrated by this report. Every capability above is a runtime observation, which"
    );
    let _ = writeln!(
        out,
        "is not release certification: see platform-matrix.md for what evidence requires."
    );

    out
}

/// The Wayland part of the report.
///
/// Only compiled on Linux. On a build for another OS the arm cannot be reached
/// anyway, and this keeps the adapter dependency off those targets.
#[cfg(target_os = "linux")]
fn wayland_section(
    out: &mut String,
    capabilities: &mut CapabilityReport,
    not_probed: &mut Vec<String>,
) {
    // Imported here rather than at the top of the file: only this arm records
    // findings, and a top-level import is dead weight on every other target.
    use ink_platform::{CapabilityFinding, CapabilityState};

    match ink_platform_wayland::probe() {
        Ok(probe) => {
            let _ = writeln!(*out, "  advertised globals: {}", probe.globals.len());
            for global in &probe.globals {
                let _ = writeln!(*out, "    {} v{}", global.interface, global.version);
            }
            let _ = writeln!(
                out,
                "  zwlr_layer_shell_v1: {}",
                if probe.advertises_layer_shell() {
                    "advertised (still unproven: no surface was created)"
                } else {
                    "not advertised"
                }
            );
            let _ = writeln!(*out);
            let _ = writeln!(*out, "outputs ({})", probe.outputs.len());
            for (index, output) in probe.outputs.iter().enumerate() {
                let name = output.name.as_deref().unwrap_or("<no name event>");
                let _ = writeln!(*out, "  [{index}] {name}");
                if let Some(description) = &output.description {
                    let _ = writeln!(*out, "      description: {description}");
                }
                match output.resolution {
                    Some((w, h)) => {
                        let refresh = output
                            .refresh_mhz
                            .map(|r| format!("{:.3} Hz", f64::from(r) / 1000.0))
                            .unwrap_or_else(|| "<no refresh>".to_owned());
                        let _ = writeln!(*out, "      current mode: {w}x{h} px @ {refresh}");
                    }
                    None => {
                        let _ = writeln!(*out, "      current mode: <not reported>");
                    }
                }
                let _ = writeln!(
                    out,
                    "      integer scale: {}   transform: {}",
                    output
                        .scale
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "<not reported>".to_owned()),
                    output.transform.as_deref().unwrap_or("<not reported>"),
                );
            }
            let _ = writeln!(
                out,
                "  note: integer scale only. Fractional scaling is negotiated per surface,"
            );
            let _ = writeln!(
                out,
                "        so the effective scale is not established here (FR-012, T018)."
            );
            *capabilities = ink_platform_wayland::capabilities(&probe);
        }
        Err(error) => {
            let _ = writeln!(*out, "  probe failed [{}]: {error}", error.class());
            capabilities.record(CapabilityFinding::new(
                Capability::OutputEnumeration,
                CapabilityState::unknown(format!("the wayland probe failed: {error}")),
                "wayland",
            ));
            not_probed.push(
                "every wayland capability: the compositor connection failed, so nothing \
                 was observed"
                    .to_owned(),
            );
        }
    }
}

/// On Windows the adapter is linked and reports what it would offer.
///
/// Every state it returns is `Unknown`, because nobody has run it. That is the
/// distinction the constitution insists on: a documented behaviour is not a
/// measured one, and reporting it as available would be inventing evidence.
#[cfg(windows)]
fn wayland_section(
    out: &mut String,
    capabilities: &mut CapabilityReport,
    not_probed: &mut Vec<String>,
) {
    let _ = writeln!(
        *out,
        "  the windows-layered backend is compiled in, and has never been run"
    );
    for finding in ink_platform_windows::capabilities().findings() {
        capabilities.record(finding.clone());
    }
    not_probed.push(
        "every windows capability: the backend compiles but nobody has watched it".to_owned(),
    );
}

/// On macOS the adapter is linked and reports what it would offer.
///
/// Every state is `Unknown`, and unlike Windows there is not even a plan for
/// measuring them: nobody on this project has a Mac.
#[cfg(target_os = "macos")]
fn wayland_section(
    out: &mut String,
    capabilities: &mut CapabilityReport,
    not_probed: &mut Vec<String>,
) {
    let _ = writeln!(
        *out,
        "  the macos-appkit backend is compiled in, and has never been run"
    );
    for finding in ink_platform_macos::capabilities().findings() {
        capabilities.record(finding.clone());
    }
    not_probed
        .push("every macos capability: the backend compiles but nobody has watched it".to_owned());
}

/// On any other build no adapter is linked, so nothing can be said.
#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn wayland_section(
    out: &mut String,
    _capabilities: &mut CapabilityReport,
    not_probed: &mut Vec<String>,
) {
    let _ = writeln!(*out, "  no wayland adapter is compiled into this build");
    not_probed.push("every wayland capability: this binary was not built for Linux".to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs against whatever machine the test runs on, including a headless CI
    /// box with no display. The report must still be produced and must still be
    /// explicit about what it skipped.
    #[test]
    fn a_report_is_always_produced_and_always_lists_what_it_skipped() {
        let text = report();

        assert!(text.contains("session"));
        assert!(text.contains("not probed"));
        for capability in ALL_CAPABILITIES {
            assert!(
                text.contains(capability.as_str()),
                "{capability} is missing from the report"
            );
        }
    }

    #[test]
    fn every_capability_line_carries_its_reason() {
        let text = report();

        assert_eq!(
            text.matches("because:").count(),
            ALL_CAPABILITIES.len(),
            "a capability without a reason is exactly the silent claim FR-013 forbids",
        );
    }
}
