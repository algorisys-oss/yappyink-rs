//! T002 contract tests.
//!
//! Scenarios: AC-FR-013 (capabilities and honest fallback), AC-NFR-005 (failure
//! visibility). These run headlessly against the vocabulary; they are not
//! evidence about any desktop. Native behaviour is T005-T007.

use std::path::{Path, PathBuf};

use ink_platform::{
    Capability, CapabilityFinding, CapabilityReport, CapabilityState, EnvSnapshot, PlatformError,
    SessionKind, SocketProbe, detect_session,
};

/// A socket layer that accepts exactly the paths it was given.
struct FakeSockets {
    connectable: Vec<PathBuf>,
}

impl SocketProbe for FakeSockets {
    fn can_connect(&self, path: &Path) -> Result<(), String> {
        if self.connectable.iter().any(|p| p == path) {
            Ok(())
        } else {
            Err("No such file or directory (os error 2)".to_owned())
        }
    }
}

// --- AC-FR-013 -------------------------------------------------------------

#[test]
fn unknown_is_not_available() {
    let unknown = CapabilityState::unknown("no probe implemented yet");
    assert!(!unknown.is_available());
    assert_ne!(
        unknown.label(),
        CapabilityState::available("observed").label()
    );
}

#[test]
fn a_capability_nobody_reported_reads_back_as_unknown() {
    let report = CapabilityReport::new();

    let state = report.state_of(Capability::VisiblePassthrough);

    assert!(matches!(state, CapabilityState::Unknown { .. }));
    assert!(!state.is_available());
}

#[test]
fn unavailable_and_needs_user_action_stay_distinct() {
    let mut report = CapabilityReport::new();
    report.record(CapabilityFinding::new(
        Capability::LiveOverlay,
        CapabilityState::unavailable("zwlr_layer_shell_v1 is not advertised"),
        "wayland",
    ));
    report.record(CapabilityFinding::new(
        Capability::GlobalShortcut,
        CapabilityState::needs_user_action("bind a chord in the desktop's own shortcut settings"),
        "wayland",
    ));

    assert!(!report.state_of(Capability::LiveOverlay).is_available());
    assert!(!report.state_of(Capability::GlobalShortcut).is_available());
    assert_eq!(
        report.state_of(Capability::LiveOverlay).label(),
        "unavailable"
    );
    assert_eq!(
        report.state_of(Capability::GlobalShortcut).label(),
        "needs_user_action"
    );
}

#[test]
fn every_unsuccessful_state_carries_a_reason() {
    for state in [
        CapabilityState::unknown("the portal interface was not queried"),
        CapabilityState::unavailable("protocol absent"),
        CapabilityState::needs_user_action("grant the permission"),
    ] {
        assert!(!state.detail().is_empty(), "{state:?} must explain itself");
    }
}

#[test]
fn a_later_finding_refines_an_earlier_one() {
    let mut report = CapabilityReport::new();
    report.record(CapabilityFinding::new(
        Capability::OutputEnumeration,
        CapabilityState::unknown("session not resolved yet"),
        "none",
    ));
    report.record(CapabilityFinding::new(
        Capability::OutputEnumeration,
        CapabilityState::available("wl_output advertised 2 outputs"),
        "wayland",
    ));

    assert!(
        report
            .state_of(Capability::OutputEnumeration)
            .is_available()
    );
    assert_eq!(
        report.findings().len(),
        1,
        "refining must not leave the stale finding behind"
    );
}

#[test]
fn unreported_capabilities_are_listed_so_a_report_can_say_what_it_skipped() {
    let all = [
        Capability::LiveOverlay,
        Capability::Capture,
        Capability::KeyboardRelease,
    ];
    let mut report = CapabilityReport::new();
    report.record(CapabilityFinding::new(
        Capability::LiveOverlay,
        CapabilityState::unavailable("no overlay surface exists yet"),
        "wayland",
    ));

    assert_eq!(
        report.unreported(&all),
        vec![Capability::Capture, Capability::KeyboardRelease]
    );
}

// --- Session detection: what answers, not what is named ---------------------

#[test]
fn a_session_claiming_wayland_without_a_reachable_socket_is_not_wayland() {
    let env = EnvSnapshot::from_pairs([
        ("XDG_SESSION_TYPE", "wayland"),
        ("XDG_CURRENT_DESKTOP", "ubuntu:GNOME"),
        ("WAYLAND_DISPLAY", "wayland-0"),
        ("XDG_RUNTIME_DIR", "/run/user/1001"),
    ]);
    let sockets = FakeSockets {
        connectable: vec![],
    };

    match detect_session(&env, &sockets) {
        SessionKind::None { reasons } => {
            assert!(reasons.iter().any(|r| r.contains("wayland")), "{reasons:?}");
        }
        other => panic!("an unreachable socket must not be reported as a session: {other:?}"),
    }
}

#[test]
fn wayland_wins_over_xwayland_when_both_answer() {
    let env = EnvSnapshot::from_pairs([
        ("WAYLAND_DISPLAY", "wayland-0"),
        ("XDG_RUNTIME_DIR", "/run/user/1001"),
        ("DISPLAY", ":0"),
    ]);
    let sockets = FakeSockets {
        connectable: vec![
            PathBuf::from("/run/user/1001/wayland-0"),
            PathBuf::from("/tmp/.X11-unix/X0"),
        ],
    };

    assert!(matches!(
        detect_session(&env, &sockets),
        SessionKind::Wayland { .. }
    ));
}

#[test]
fn x11_is_reported_when_only_the_x_socket_answers() {
    let env = EnvSnapshot::from_pairs([("DISPLAY", ":0")]);
    let sockets = FakeSockets {
        connectable: vec![PathBuf::from("/tmp/.X11-unix/X0")],
    };

    match detect_session(&env, &sockets) {
        SessionKind::X11 { display, .. } => assert_eq!(display, ":0"),
        other => panic!("expected X11, got {other:?}"),
    }
}

#[test]
fn an_empty_environment_reports_why_each_route_failed() {
    let env = EnvSnapshot::default();
    let sockets = FakeSockets {
        connectable: vec![],
    };

    match detect_session(&env, &sockets) {
        SessionKind::None { reasons } => assert_eq!(reasons.len(), 2, "{reasons:?}"),
        other => panic!("expected no session, got {other:?}"),
    }
}

// --- AC-NFR-005 ------------------------------------------------------------

#[test]
fn error_classes_stay_distinguishable() {
    let errors = [
        PlatformError::unsupported("layer surface", "protocol not advertised"),
        PlatformError::PermissionDenied {
            operation: "global shortcut".to_owned(),
            reason: "the portal request was refused".to_owned(),
        },
        PlatformError::ShortcutConflict {
            binding: "Ctrl+Alt+D".to_owned(),
            holder: "the desktop environment".to_owned(),
        },
        PlatformError::disconnected("the compositor closed the connection"),
        PlatformError::SurfaceLost {
            reason: "the output was removed".to_owned(),
        },
        PlatformError::invalid_data("wl_output scale", "a non-positive factor was sent"),
    ];

    let classes: Vec<&str> = errors.iter().map(PlatformError::class).collect();
    let mut unique = classes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        classes.len(),
        "error classes collapsed: {classes:?}"
    );

    for error in &errors {
        assert!(
            !error.to_string().is_empty(),
            "{error:?} must render for a user"
        );
    }
}

#[test]
fn only_connection_faults_are_retryable_and_only_refusals_ask_the_user() {
    assert!(PlatformError::disconnected("gone").is_transient());
    assert!(
        PlatformError::SurfaceLost {
            reason: "gone".to_owned()
        }
        .is_transient()
    );
    assert!(!PlatformError::unsupported("layer surface", "absent").is_transient());

    assert!(
        PlatformError::ShortcutConflict {
            binding: "Ctrl+Alt+D".to_owned(),
            holder: "gnome-shell".to_owned(),
        }
        .needs_user_action()
    );
    assert!(
        !PlatformError::unsupported("layer surface", "absent").needs_user_action(),
        "an absent protocol is not something the user can grant"
    );
}
