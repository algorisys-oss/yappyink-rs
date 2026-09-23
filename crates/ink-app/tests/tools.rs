//! T015: pen and highlighter.
//!
//! Requirements: FR-007 (pen and highlighter), FR-018 (gesture safety),
//! NFR-003 (bounded storage). Scenario: AC-FR-007.
//!
//! Headless. The rules about what a gesture becomes live here rather than in
//! the adapter, which is why they can be tested at all.

use ink_app::{Action, Controller, Effect, PlatformEvent, Preview, Tool};
use ink_core::{LogicalPoint, Opacity, Rgb, Shape, StrokeKind, limits};

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

/// Drives the controller into Draw, the way a working platform would.
fn drawing() -> Controller {
    let mut controller = Controller::new();
    let effects = controller.act(Action::EnterDraw);
    let transition = effects
        .iter()
        .find_map(|e| match e {
            Effect::ApplyMode { transition, .. } => Some(*transition),
            _ => None,
        })
        .expect("EnterDraw requests a mode");
    controller.handle(PlatformEvent::ModeApplied { transition });
    controller
}

/// Draws a two-sample gesture and returns the commit it produced.
fn stroke(controller: &mut Controller) -> Effect {
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(40.0, 40.0),
    });
    controller
        .handle(PlatformEvent::PointerUp {
            at: point(70.0, 70.0),
        })
        .into_iter()
        .find(|e| matches!(e, Effect::CommitObject { .. }))
        .expect("a completed gesture commits")
}

// --- Tool selection --------------------------------------------------------

#[test]
fn the_pen_is_the_default_tool() {
    let controller = Controller::new();
    assert_eq!(controller.tool(), Tool::Pen);
}

#[test]
fn a_committed_stroke_carries_the_tool_it_was_drawn_with() {
    let mut controller = drawing();

    match stroke(&mut controller) {
        Effect::CommitObject {
            shape: Shape::Stroke { kind, .. },
            ..
        } => {
            assert_eq!(kind, StrokeKind::Pen)
        }
        other => panic!("expected a commit, got {other:?}"),
    }

    controller.act(Action::SelectTool(Tool::Highlighter));
    match stroke(&mut controller) {
        Effect::CommitObject {
            shape: Shape::Stroke { kind, .. },
            ..
        } => {
            assert_eq!(kind, StrokeKind::Highlighter)
        }
        other => panic!("expected a commit, got {other:?}"),
    }
}

#[test]
fn the_highlighter_starts_wider_and_translucent() {
    // FR-007: a highlighter that looked exactly like the pen would be a
    // pointless tool. The defaults are a starting point, not a measurement.
    let mut controller = Controller::new();
    let pen = controller.style();
    controller.act(Action::SelectTool(Tool::Highlighter));
    let highlighter = controller.style();

    assert!(highlighter.width.get() > pen.width.get());
    assert!(highlighter.opacity.get() < pen.opacity.get());
    assert_eq!(pen.opacity, Opacity::OPAQUE, "the pen is opaque");
}

#[test]
fn each_tool_keeps_its_own_settings() {
    // Switching to the highlighter and back must not silently change the pen
    // the user had set up.
    let mut controller = Controller::new();
    controller.act(Action::SetColor(Rgb::new(0, 128, 255)));
    let pen = controller.style();

    controller.act(Action::SelectTool(Tool::Highlighter));
    controller.act(Action::SetColor(Rgb::new(255, 255, 0)));
    controller.act(Action::SelectTool(Tool::Pen));

    assert_eq!(controller.style(), pen, "the pen came back as it was left");
}

// --- Style is captured when the gesture starts -----------------------------

#[test]
fn changing_the_style_mid_gesture_does_not_rewrite_the_stroke() {
    // The committed object is what the user was drawing when they pressed, not
    // what the settings happened to be when they let go.
    let mut controller = drawing();
    let started_with = controller.style();

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.act(Action::SetColor(Rgb::new(1, 2, 3)));
    controller.act(Action::AdjustWidth(4));
    let committed = controller
        .handle(PlatformEvent::PointerUp {
            at: point(20.0, 20.0),
        })
        .into_iter()
        .find(|e| matches!(e, Effect::CommitObject { .. }))
        .expect("a commit");

    match committed {
        Effect::CommitObject { style, .. } => assert_eq!(style, started_with),
        other => panic!("expected a commit, got {other:?}"),
    }
}

#[test]
fn changing_the_tool_mid_gesture_does_not_change_what_is_being_drawn() {
    let mut controller = drawing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });

    controller.act(Action::SelectTool(Tool::Highlighter));
    let committed = controller
        .handle(PlatformEvent::PointerUp {
            at: point(20.0, 20.0),
        })
        .into_iter()
        .find(|e| matches!(e, Effect::CommitObject { .. }))
        .expect("a commit");

    match committed {
        Effect::CommitObject {
            shape: Shape::Stroke { kind, .. },
            ..
        } => {
            assert_eq!(kind, StrokeKind::Pen, "the stroke stays what it started as");
        }
        other => panic!("expected a commit, got {other:?}"),
    }
}

// --- Style adjustments -----------------------------------------------------

#[test]
fn width_steps_stay_within_usable_bounds() {
    let mut controller = Controller::new();

    for _ in 0..200 {
        controller.act(Action::AdjustWidth(-1));
    }
    let thinnest = controller.style().width.get();
    assert!(thinnest > 0.0, "width can never reach zero or go negative");

    for _ in 0..200 {
        controller.act(Action::AdjustWidth(1));
    }
    let thickest = controller.style().width.get();
    assert!(thickest.is_finite());
    assert!(thickest > thinnest);
}

#[test]
fn opacity_steps_stay_within_zero_and_one() {
    let mut controller = Controller::new();
    controller.act(Action::SelectTool(Tool::Highlighter));

    for _ in 0..100 {
        controller.act(Action::AdjustOpacity(1));
    }
    assert!(controller.style().opacity.get() <= 1.0);

    for _ in 0..100 {
        controller.act(Action::AdjustOpacity(-1));
    }
    let lowest = controller.style().opacity.get();
    assert!(
        lowest > 0.0,
        "a fully invisible tool is a trap, not a setting"
    );
}

#[test]
fn cycling_colours_returns_to_where_it_started() {
    let mut controller = Controller::new();
    let first = controller.style().color;

    let mut seen = vec![first];
    for _ in 0..20 {
        controller.act(Action::CycleColor);
        let colour = controller.style().color;
        if colour == first {
            break;
        }
        seen.push(colour);
    }

    assert!(seen.len() > 1, "there is more than one colour");
    assert_eq!(controller.style().color, first, "the palette is a cycle");
}

// --- NFR-003: bounded sampling ---------------------------------------------

#[test]
fn samples_too_close_together_are_dropped() {
    // A pointer reporting at 1000 Hz over a short drag would otherwise store
    // thousands of samples a person cannot see.
    let mut controller = drawing();

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    for _ in 0..100 {
        controller.handle(PlatformEvent::PointerMoved {
            at: point(10.01, 10.01),
        });
    }

    let points = controller.gesture_points().expect("a gesture in flight");
    assert_eq!(
        points.len(),
        1,
        "near-identical samples add nothing, got {}",
        points.len()
    );
}

#[test]
fn a_gesture_stops_growing_at_the_sample_limit() {
    let mut controller = drawing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(0.0, 0.0),
    });

    // Every sample is far enough apart to be kept, so only the limit stops it.
    for step in 1..(limits::MAX_STROKE_POINTS + 500) {
        controller.handle(PlatformEvent::PointerMoved {
            at: point(step as f64 * 2.0, 0.0),
        });
    }

    let points = controller.gesture_points().expect("a gesture in flight");
    assert_eq!(points.len(), limits::MAX_STROKE_POINTS);
}

#[test]
fn a_capped_gesture_still_commits_one_valid_object() {
    // The cap must not turn into a rejected commit at release time: the user
    // drew something and it has to become ink.
    let mut controller = drawing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(0.0, 0.0),
    });
    for step in 1..(limits::MAX_STROKE_POINTS + 10) {
        controller.handle(PlatformEvent::PointerMoved {
            at: point(step as f64 * 2.0, 0.0),
        });
    }

    let committed = controller
        .handle(PlatformEvent::PointerUp {
            at: point(1.0, 1.0),
        })
        .into_iter()
        .find(|e| matches!(e, Effect::CommitObject { .. }))
        .expect("a commit");

    match committed {
        Effect::CommitObject {
            shape: Shape::Stroke { points, .. },
            ..
        } => {
            assert!(
                points.len() <= limits::MAX_STROKE_POINTS,
                "got {}",
                points.len()
            );
            assert!(!points.is_empty());
        }
        other => panic!("expected a commit, got {other:?}"),
    }
}

#[test]
fn a_dot_survives_the_thinning_rule() {
    // FR-007: a press and release in one place is a valid gesture. The
    // distance filter must not eat it.
    let mut controller = drawing();

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    let committed = controller
        .handle(PlatformEvent::PointerUp {
            at: point(10.0, 10.0),
        })
        .into_iter()
        .find(|e| matches!(e, Effect::CommitObject { .. }))
        .expect("a dot commits");

    match committed {
        Effect::CommitObject {
            shape: Shape::Stroke { points, .. },
            ..
        } => {
            assert_eq!(points.len(), 1)
        }
        other => panic!("expected a commit, got {other:?}"),
    }
}

#[test]
fn a_cancelled_gesture_still_produces_no_commit_whatever_the_tool() {
    let mut controller = drawing();
    controller.act(Action::SelectTool(Tool::Highlighter));
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(40.0, 40.0),
    });

    let effects = controller.act(Action::Escape);

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::GestureCancelled { .. }))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitObject { .. }))
    );
}

// --- T016: shapes ----------------------------------------------------------

/// Drags from one point to another and returns what it produced.
fn drag(controller: &mut Controller, from: (f64, f64), to: (f64, f64)) -> Vec<Effect> {
    controller.handle(PlatformEvent::PointerDown {
        at: point(from.0, from.1),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(to.0, to.1),
    })
}

fn committed_shape(effects: &[Effect]) -> Option<&Shape> {
    effects.iter().find_map(|e| match e {
        Effect::CommitObject { shape, .. } => Some(shape),
        _ => None,
    })
}

#[test]
fn each_shape_tool_commits_its_own_geometry() {
    for (tool, matches_shape) in [
        (
            Tool::Line,
            (|s: &Shape| matches!(s, Shape::Line { .. })) as fn(&Shape) -> bool,
        ),
        (Tool::Arrow, |s| matches!(s, Shape::Arrow { .. })),
        (Tool::Rectangle, |s| matches!(s, Shape::Rectangle { .. })),
        (Tool::Ellipse, |s| matches!(s, Shape::Ellipse { .. })),
    ] {
        let mut controller = drawing();
        controller.act(Action::SelectTool(tool));

        let effects = drag(&mut controller, (10.0, 10.0), (80.0, 60.0));

        let shape =
            committed_shape(&effects).unwrap_or_else(|| panic!("{tool:?} committed nothing"));
        assert!(matches_shape(shape), "{tool:?} produced {shape:?}");
    }
}

#[test]
fn one_completed_drag_creates_exactly_one_object() {
    // FR-008. The samples in between are not objects, however many there were.
    let mut controller = drawing();
    controller.act(Action::SelectTool(Tool::Rectangle));

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    for step in 1..50 {
        controller.handle(PlatformEvent::PointerMoved {
            at: point(10.0 + step as f64, 10.0),
        });
    }
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(80.0, 60.0),
    });

    let commits = effects
        .iter()
        .filter(|e| matches!(e, Effect::CommitObject { .. }))
        .count();
    assert_eq!(commits, 1);
}

#[test]
fn a_shape_is_defined_by_where_the_drag_ended_not_by_the_samples() {
    let mut controller = drawing();
    controller.act(Action::SelectTool(Tool::Line));

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(500.0, 500.0),
    });
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(80.0, 60.0),
    });

    match committed_shape(&effects) {
        Some(Shape::Line { from, to }) => {
            assert_eq!((from.x, from.y), (10.0, 10.0));
            assert_eq!(
                (to.x, to.y),
                (80.0, 60.0),
                "the wandering middle is not kept"
            );
        }
        other => panic!("expected a line, got {other:?}"),
    }
}

#[test]
fn a_drag_that_went_nowhere_is_discarded_with_a_reason() {
    // FR-008: a degenerate drag must not become an invisible object. The user
    // completed this gesture, so it is discarded rather than cancelled, and
    // the reason travels with it.
    for tool in [Tool::Line, Tool::Arrow, Tool::Rectangle, Tool::Ellipse] {
        let mut controller = drawing();
        controller.act(Action::SelectTool(tool));

        let effects = drag(&mut controller, (40.0, 40.0), (40.0, 40.0));

        assert!(
            committed_shape(&effects).is_none(),
            "{tool:?} created an invisible object"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::GestureDiscarded { .. })),
            "{tool:?} discarded the gesture silently: {effects:?}"
        );
    }
}

#[test]
fn a_shape_in_flight_previews_and_a_degenerate_one_does_not() {
    let mut controller = drawing();
    controller.act(Action::SelectTool(Tool::Ellipse));

    controller.handle(PlatformEvent::PointerDown {
        at: point(40.0, 40.0),
    });
    assert!(
        controller.preview().is_none(),
        "a drag that has not moved shows nothing"
    );
    assert!(controller.is_gesturing(), "but a gesture is in flight");

    controller.handle(PlatformEvent::PointerMoved {
        at: point(90.0, 80.0),
    });
    assert!(matches!(controller.preview(), Some(Preview::Shape { .. })));
}

#[test]
fn cancelling_a_shape_mid_drag_commits_nothing() {
    let mut controller = drawing();
    controller.act(Action::SelectTool(Tool::Arrow));
    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(80.0, 60.0),
    });

    let effects = controller.act(Action::Escape);

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::GestureCancelled { .. }))
    );
    assert!(committed_shape(&effects).is_none());
    assert!(!controller.is_gesturing());
}

#[test]
fn the_four_shape_tools_share_one_style() {
    // T016 asks for style reuse: they are one pen held differently, so
    // switching from a rectangle to an arrow must not change the colour.
    let mut controller = Controller::new();
    controller.act(Action::SelectTool(Tool::Rectangle));
    controller.act(Action::CycleColor);
    let chosen = controller.style();

    controller.act(Action::SelectTool(Tool::Arrow));

    assert_eq!(controller.style(), chosen);
}

#[test]
fn a_shape_keeps_the_style_it_started_with() {
    let mut controller = drawing();
    controller.act(Action::SelectTool(Tool::Rectangle));
    let started_with = controller.style();

    controller.handle(PlatformEvent::PointerDown {
        at: point(10.0, 10.0),
    });
    controller.act(Action::CycleColor);
    controller.act(Action::AdjustWidth(3));
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(80.0, 60.0),
    });

    match effects
        .iter()
        .find(|e| matches!(e, Effect::CommitObject { .. }))
    {
        Some(Effect::CommitObject { style, .. }) => assert_eq!(*style, started_with),
        other => panic!("expected a commit, got {other:?}"),
    }
}
