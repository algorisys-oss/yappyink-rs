//! T010: the interaction reducer.
//!
//! Requirements: FR-002 (Draw), FR-003 (PassThrough), FR-004 (Hidden without
//! clearing), FR-018 (gesture and transition safety), FR-019 (fault recovery).
//! Scenarios: AC-FR-004, AC-FR-018, AC-FR-019.
//!
//! Every test runs headlessly with no clock and no platform. The rules encoded
//! here were verified by hand against Mutter on 2026-09-23; see
//! `docs/evidence/E003-gnome-interaction-probe.md`.

use ink_app::{Action, Controller, Effect, Mode, PlatformEvent, TransitionId};
use ink_core::LogicalPoint;
use ink_platform::PlatformError;

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

/// Drives the controller to a confirmed mode, the way a working platform would.
fn settle(controller: &mut Controller, action: Action) {
    let effects = controller.act(action);
    let transition = applied_transition(&effects).expect("the action should request a mode");
    controller.handle(PlatformEvent::ModeApplied { transition });
}

fn applied_transition(effects: &[Effect]) -> Option<TransitionId> {
    effects.iter().find_map(|effect| match effect {
        Effect::ApplyMode { transition, .. } => Some(*transition),
        _ => None,
    })
}

fn requested_mode(effects: &[Effect]) -> Option<Mode> {
    effects.iter().find_map(|effect| match effect {
        Effect::ApplyMode { mode, .. } => Some(*mode),
        _ => None,
    })
}

// --- Startup and the desired/effective split -------------------------------

#[test]
fn startup_is_hidden_with_no_blocking_surface() {
    // FR-004: startup begins Hidden.
    let controller = Controller::new();

    assert_eq!(controller.mode(), Mode::Hidden);
    assert_eq!(controller.desired_mode(), Mode::Hidden);
    assert!(!controller.is_transitioning());
}

#[test]
fn a_requested_mode_is_not_effective_until_the_platform_confirms() {
    // FR-019: do not show PassThrough as effective until its native transition
    // completes.
    let mut controller = Controller::new();

    let effects = controller.act(Action::EnterDraw);

    assert_eq!(requested_mode(&effects), Some(Mode::Draw));
    assert_eq!(
        controller.desired_mode(),
        Mode::Draw,
        "the intent is recorded"
    );
    assert_eq!(
        controller.mode(),
        Mode::Hidden,
        "but nothing is effective yet"
    );
    assert!(controller.is_transitioning());

    let transition = applied_transition(&effects).unwrap();
    controller.handle(PlatformEvent::ModeApplied { transition });

    assert_eq!(controller.mode(), Mode::Draw);
    assert!(!controller.is_transitioning());
}

// --- Transitions -----------------------------------------------------------

#[test]
fn toggle_draw_moves_between_draw_and_passthrough() {
    let mut controller = Controller::new();

    settle(&mut controller, Action::ToggleDraw);
    assert_eq!(controller.mode(), Mode::Draw, "Hidden -> Draw");

    settle(&mut controller, Action::ToggleDraw);
    assert_eq!(controller.mode(), Mode::PassThrough, "Draw -> PassThrough");

    settle(&mut controller, Action::ToggleDraw);
    assert_eq!(controller.mode(), Mode::Draw, "PassThrough -> Draw");
}

#[test]
fn toggle_visibility_hides_from_any_visible_mode_and_returns_to_passthrough() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::ToggleDraw);

    settle(&mut controller, Action::ToggleVisibility);
    assert_eq!(controller.mode(), Mode::Hidden, "Draw -> Hidden");

    // Hidden -> PassThrough: showing old ink must never silently start taking
    // drawing input.
    settle(&mut controller, Action::ToggleVisibility);
    assert_eq!(controller.mode(), Mode::PassThrough);
}

#[test]
fn a_request_for_the_mode_already_in_effect_does_nothing() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);

    let effects = controller.act(Action::EnterDraw);

    assert!(
        effects.is_empty(),
        "a no-op must not produce a platform request"
    );
    assert!(!controller.is_transitioning());
}

// --- Gestures --------------------------------------------------------------

#[test]
fn a_completed_gesture_commits_exactly_one_stroke() {
    // FR-007: one gesture is one object, however many samples it took.
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(11.0, 12.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(14.0, 18.0),
    });
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(20.0, 30.0),
    });

    let committed: Vec<&Effect> = effects
        .iter()
        .filter(|e| matches!(e, Effect::CommitStroke { .. }))
        .collect();
    assert_eq!(committed.len(), 1);
    match committed[0] {
        Effect::CommitStroke { points } => assert_eq!(points.len(), 4),
        other => panic!("expected a commit, got {other:?}"),
    }
    assert!(
        controller.gesture_points().is_none(),
        "the transient gesture is finished"
    );
}

#[test]
fn a_press_without_movement_still_commits_a_dot() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(10.0, 10.0),
    });

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CommitStroke { .. }))
    );
}

#[test]
fn pointer_input_outside_draw_never_starts_a_stroke() {
    // FR-003: in PassThrough the pointer belongs to the application underneath.
    // If an event reaches us anyway, it must not become ink.
    let mut controller = Controller::new();
    settle(&mut controller, Action::ToggleDraw);
    settle(&mut controller, Action::ToggleDraw);
    assert_eq!(controller.mode(), Mode::PassThrough);

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(20.0, 20.0),
    });

    assert!(controller.gesture_points().is_none());
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitStroke { .. }))
    );
}

#[test]
fn a_cancelled_gesture_commits_nothing() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });

    let effects = controller.handle(PlatformEvent::PointerCancelled);

    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitStroke { .. }))
    );
    assert!(controller.gesture_points().is_none());
}

// --- FR-018: gesture and transition safety ---------------------------------

#[test]
fn a_mode_change_mid_gesture_discards_the_stroke() {
    // Verified by hand on Mutter, E003 finding 5. Here it is a deterministic
    // test instead of an observation.
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(40.0, 40.0),
    });

    let effects = controller.act(Action::ToggleDraw);

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::GestureCancelled { points: 2 })),
        "the half-drawn stroke must be reported as cancelled: {effects:?}"
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitStroke { .. }))
    );
    assert!(controller.gesture_points().is_none());
}

#[test]
fn a_button_still_held_when_draw_resumes_is_ignored_until_released() {
    // "On returning to Draw, ignore preexisting held buttons until release,
    // then accept only a new pointer-down" (ux-state-machine.md).
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });

    // The mode flips while the button is down, then comes back.
    settle(&mut controller, Action::ToggleDraw);
    settle(&mut controller, Action::ToggleDraw);
    assert_eq!(controller.mode(), Mode::Draw);

    // Movement from the still-held button must not draw.
    controller.handle(PlatformEvent::PointerMoved {
        at: point(50.0, 50.0),
    });
    assert!(
        controller.gesture_points().is_none(),
        "the stale drag must not resume"
    );

    // Releasing it commits nothing.
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(60.0, 60.0),
    });
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitStroke { .. }))
    );

    // Only a fresh press draws again.
    controller.handle(PlatformEvent::PointerDown {
        at: point(70.0, 70.0),
    });
    assert_eq!(
        controller.gesture_points().map(<[LogicalPoint]>::len),
        Some(1)
    );
}

#[test]
fn no_new_gesture_starts_while_a_transition_is_in_flight() {
    // architecture.md: while transitioning, stop accepting new gestures.
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.act(Action::ToggleDraw); // requested, unconfirmed

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });

    assert!(controller.is_transitioning());
    assert!(controller.gesture_points().is_none());
}

// --- Escape ----------------------------------------------------------------

#[test]
fn escape_cancels_a_gesture_before_it_changes_any_mode() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });

    let effects = controller.act(Action::Escape);

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::GestureCancelled { .. }))
    );
    assert!(
        requested_mode(&effects).is_none(),
        "the first Escape cancels the gesture and nothing else"
    );
    assert_eq!(controller.mode(), Mode::Draw);
}

#[test]
fn escape_with_no_gesture_requests_passthrough() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);

    let effects = controller.act(Action::Escape);

    assert_eq!(requested_mode(&effects), Some(Mode::PassThrough));
}

#[test]
fn escape_outside_draw_belongs_to_the_application_underneath() {
    // ux-state-machine.md: Escape is not globally captured in PassThrough.
    let mut controller = Controller::new();
    settle(&mut controller, Action::ToggleDraw);
    settle(&mut controller, Action::ToggleDraw);

    let effects = controller.act(Action::Escape);

    assert!(
        effects.is_empty(),
        "we must not consume Escape in PassThrough"
    );
    assert_eq!(controller.mode(), Mode::PassThrough);
}

// --- EmergencyHide ---------------------------------------------------------

#[test]
fn emergency_hide_withdraws_at_once_without_waiting_for_confirmation() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });

    let effects = controller.act(Action::EmergencyHide);

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::WithdrawImmediately))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::GestureCancelled { .. }))
    );
    assert_eq!(controller.mode(), Mode::Hidden, "withdrawal is not awaited");
    assert!(!controller.is_transitioning());
}

#[test]
fn emergency_hide_works_while_a_transition_is_in_flight() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.act(Action::ToggleDraw);
    assert!(controller.is_transitioning());

    let effects = controller.act(Action::EmergencyHide);

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::WithdrawImmediately))
    );
    assert_eq!(controller.mode(), Mode::Hidden);
}

#[test]
fn a_confirmation_arriving_after_emergency_hide_is_ignored() {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    let pending = applied_transition(&controller.act(Action::ToggleDraw)).unwrap();

    controller.act(Action::EmergencyHide);
    controller.handle(PlatformEvent::ModeApplied {
        transition: pending,
    });

    assert_eq!(
        controller.mode(),
        Mode::Hidden,
        "a late confirmation must not un-hide us"
    );
}

// --- FR-019: stale callbacks and faults ------------------------------------

#[test]
fn a_stale_confirmation_cannot_overwrite_the_current_state() {
    // architecture.md: a delayed callback from an older transition must not
    // overwrite current state.
    let mut controller = Controller::new();
    let first = applied_transition(&controller.act(Action::EnterDraw)).unwrap();

    // The user changes their mind before the first transition confirms.
    let second = applied_transition(&controller.act(Action::EmergencyHide));
    assert!(
        second.is_none(),
        "EmergencyHide withdraws rather than requesting a mode"
    );

    controller.handle(PlatformEvent::ModeApplied { transition: first });

    assert_eq!(controller.mode(), Mode::Hidden);
}

#[test]
fn a_stale_failure_cannot_fault_the_current_state() {
    let mut controller = Controller::new();
    let stale = applied_transition(&controller.act(Action::EnterDraw)).unwrap();
    controller.act(Action::EmergencyHide);
    settle(&mut controller, Action::EnterDraw);

    let effects = controller.handle(PlatformEvent::ModeFailed {
        transition: stale,
        error: PlatformError::unsupported("layer surface", "not advertised"),
    });

    assert!(
        effects.is_empty(),
        "an old failure is not news: {effects:?}"
    );
    assert!(!controller.is_faulted());
    assert_eq!(controller.mode(), Mode::Draw);
}

#[test]
fn a_failed_transition_withdraws_before_it_reports() {
    // FR-019: withdraw interactive overlays *before* reporting the fault, so a
    // failure never leaves an invisible input blocker on screen.
    let mut controller = Controller::new();
    let transition = applied_transition(&controller.act(Action::EnterDraw)).unwrap();

    let effects = controller.handle(PlatformEvent::ModeFailed {
        transition,
        error: PlatformError::unsupported("layer surface", "not advertised"),
    });

    let withdraw = effects
        .iter()
        .position(|e| matches!(e, Effect::WithdrawImmediately));
    let fault = effects
        .iter()
        .position(|e| matches!(e, Effect::Faulted { .. }));
    assert!(
        withdraw.is_some() && fault.is_some(),
        "expected both: {effects:?}"
    );
    assert!(
        withdraw < fault,
        "withdrawal must be ordered before the report"
    );

    assert_eq!(
        controller.mode(),
        Mode::Hidden,
        "a failed transition never pretends to succeed"
    );
    assert!(controller.is_faulted());
}

#[test]
fn a_fault_is_cleared_by_the_next_successful_transition() {
    let mut controller = Controller::new();
    let transition = applied_transition(&controller.act(Action::EnterDraw)).unwrap();
    controller.handle(PlatformEvent::ModeFailed {
        transition,
        error: PlatformError::disconnected("the compositor went away"),
    });
    assert!(controller.is_faulted());

    settle(&mut controller, Action::EnterDraw);

    assert!(!controller.is_faulted());
    assert_eq!(controller.mode(), Mode::Draw);
}

#[test]
fn losing_the_output_cancels_the_gesture_and_withdraws() {
    // FR-019: on output loss, cancel the transient gesture, keep committed
    // work, and withdraw input-intercepting surfaces.
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });

    let effects = controller.handle(PlatformEvent::OutputLost);

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::GestureCancelled { .. }))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::WithdrawImmediately))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitStroke { .. }))
    );
    assert_eq!(controller.mode(), Mode::Hidden);
}

#[test]
fn a_late_confirmation_loses_to_a_newer_request() {
    // The case the id exists for: the user changes their mind while a
    // transition is in flight, and the platform then confirms the one they
    // abandoned. Without the id check the abandoned mode would win.
    let mut controller = Controller::new();
    let first = applied_transition(&controller.act(Action::EnterDraw)).unwrap();
    let second = applied_transition(&controller.act(Action::ToggleVisibility)).unwrap();
    assert_ne!(first, second, "each request needs its own transition");

    controller.handle(PlatformEvent::ModeApplied { transition: first });

    assert_eq!(
        controller.mode(),
        Mode::Hidden,
        "the abandoned Draw must not take effect"
    );
    assert_eq!(controller.desired_mode(), Mode::Hidden);
    assert!(
        controller.is_transitioning(),
        "the newer transition is still outstanding"
    );

    controller.handle(PlatformEvent::ModeApplied { transition: second });
    assert!(!controller.is_transitioning());
}

#[test]
fn a_late_failure_loses_to_a_newer_request() {
    let mut controller = Controller::new();
    let first = applied_transition(&controller.act(Action::EnterDraw)).unwrap();
    let second = applied_transition(&controller.act(Action::ToggleVisibility)).unwrap();

    let effects = controller.handle(PlatformEvent::ModeFailed {
        transition: first,
        error: PlatformError::disconnected("the compositor went away"),
    });

    assert!(
        effects.is_empty(),
        "an abandoned transition's failure is not news: {effects:?}"
    );
    assert!(!controller.is_faulted());

    controller.handle(PlatformEvent::ModeApplied { transition: second });
    assert_eq!(
        controller.mode(),
        Mode::Hidden,
        "ToggleVisibility from Draw goes to Hidden, and the newer request is what lands"
    );
}
