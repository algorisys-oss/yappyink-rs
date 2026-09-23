//! Selection, move, resize and delete.
//!
//! Requirement: FR-023, the non-text half. Scenario: AC-FR-023 in part.
//! Task: the split recorded against T028.
//!
//! Headless. The controller does not own the document, so the scene is pushed
//! in as ids and bounds; these tests supply it the same way the adapter does.

use ink_app::{Action, Controller, Effect, PlatformEvent, SelectionDrag, Tool};
use ink_core::{LogicalPoint, LogicalRect, ObjectId};

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> LogicalRect {
    LogicalRect {
        min: point(x0, y0),
        max: point(x1, y1),
    }
}

/// A controller in Draw with the Select tool and two objects on the canvas,
/// both well clear of the toolbar.
fn selecting() -> (Controller, ObjectId, ObjectId) {
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
    controller.act(Action::SelectTool(Tool::Select));

    let first = ObjectId::from_raw(1);
    let second = ObjectId::from_raw(2);
    controller.set_scene(vec![
        (first, rect(100.0, 200.0, 200.0, 300.0)),
        (second, rect(400.0, 200.0, 500.0, 300.0)),
    ]);
    (controller, first, second)
}

fn press_drag_release(
    controller: &mut Controller,
    from: LogicalPoint,
    to: LogicalPoint,
) -> Vec<Effect> {
    controller.handle(PlatformEvent::PointerDown { at: from });
    controller.handle(PlatformEvent::PointerMoved { at: to });
    controller.handle(PlatformEvent::PointerUp { at: to })
}

// --- Picking ---------------------------------------------------------------

#[test]
fn pressing_an_object_selects_it() {
    let (mut controller, first, _) = selecting();

    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });

    assert_eq!(controller.selection(), &[first]);
}

#[test]
fn pressing_empty_space_drops_the_selection() {
    let (mut controller, _, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });
    assert!(!controller.selection().is_empty());

    controller.handle(PlatformEvent::PointerDown {
        at: point(800.0, 800.0),
    });

    assert!(controller.selection().is_empty());
    assert!(!controller.is_gesturing(), "empty space starts no drag");
}

#[test]
fn the_topmost_object_wins_where_they_overlap() {
    let mut controller = Controller::new();
    let effects = controller.act(Action::EnterDraw);
    let transition = effects
        .iter()
        .find_map(|e| match e {
            Effect::ApplyMode { transition, .. } => Some(*transition),
            _ => None,
        })
        .unwrap();
    controller.handle(PlatformEvent::ModeApplied { transition });
    controller.act(Action::SelectTool(Tool::Select));

    let under = ObjectId::from_raw(1);
    let over = ObjectId::from_raw(2);
    controller.set_scene(vec![
        (under, rect(100.0, 200.0, 300.0, 400.0)),
        (over, rect(150.0, 250.0, 250.0, 350.0)),
    ]);

    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });

    assert_eq!(
        controller.selection(),
        &[over],
        "later objects paint on top"
    );
}

#[test]
fn only_the_select_tool_picks_things() {
    for tool in [Tool::Pen, Tool::Rectangle, Tool::Eraser] {
        let (mut controller, _, _) = selecting();
        controller.act(Action::SelectTool(tool));

        controller.handle(PlatformEvent::PointerDown {
            at: point(150.0, 250.0),
        });

        assert!(
            controller.selection().is_empty(),
            "{tool:?} selected something"
        );
    }
}

// --- Moving ----------------------------------------------------------------

#[test]
fn dragging_a_selected_object_moves_it() {
    let (mut controller, first, _) = selecting();

    let effects = press_drag_release(&mut controller, point(150.0, 250.0), point(170.0, 290.0));

    match effects
        .iter()
        .find(|e| matches!(e, Effect::MoveSelection { .. }))
    {
        Some(Effect::MoveSelection { ids, dx, dy }) => {
            assert_eq!(ids, &[first]);
            assert_eq!((*dx, *dy), (20.0, 40.0));
        }
        other => panic!("expected a move, got {other:?}"),
    }
}

#[test]
fn a_click_that_does_not_move_is_a_selection_not_an_edit() {
    // It must not become an undo entry the user has to step over.
    let (mut controller, _, _) = selecting();

    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    let effects = controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });

    assert!(effects.is_empty(), "a bare click produced {effects:?}");
}

#[test]
fn a_move_in_flight_previews_as_an_offset() {
    let (mut controller, _, _) = selecting();

    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(160.0, 270.0),
    });

    assert_eq!(
        controller.selection_drag(),
        Some(SelectionDrag::Move { dx: 10.0, dy: 20.0 })
    );
}

// --- Resizing --------------------------------------------------------------

#[test]
fn a_selection_has_a_handle_at_each_corner() {
    let (mut controller, _, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });

    let handles = controller.selection_handles();

    assert_eq!(handles.len(), 4);
    let bounds = controller
        .selection_bounds()
        .expect("a selection has bounds");
    assert_eq!(bounds, rect(100.0, 200.0, 200.0, 300.0));
}

#[test]
fn dragging_a_corner_handle_scales_about_the_opposite_corner() {
    let (mut controller, first, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });

    // Bottom-right handle, dragged to double the size.
    let effects = press_drag_release(&mut controller, point(200.0, 300.0), point(300.0, 400.0));

    match effects
        .iter()
        .find(|e| matches!(e, Effect::ScaleSelection { .. }))
    {
        Some(Effect::ScaleSelection {
            ids,
            anchor,
            sx,
            sy,
        }) => {
            assert_eq!(ids, &[first]);
            assert_eq!(*anchor, point(100.0, 200.0), "the far corner stays put");
            assert!((*sx - 2.0).abs() < 1e-9, "sx was {sx}");
            assert!((*sy - 2.0).abs() < 1e-9, "sy was {sy}");
        }
        other => panic!("expected a scale, got {other:?}"),
    }
}

#[test]
fn a_handle_wins_over_the_object_underneath_it() {
    // A selection's handles sit on the object, so if the object won the
    // selection could never be shrunk.
    let (mut controller, _, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });

    // The top-left handle is inside the object's own bounds.
    controller.handle(PlatformEvent::PointerDown {
        at: point(100.0, 200.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(120.0, 220.0),
    });

    assert!(
        matches!(
            controller.selection_drag(),
            Some(SelectionDrag::Scale { .. })
        ),
        "the press went to the object instead of the handle"
    );
}

#[test]
fn dragging_a_corner_through_its_anchor_stops_rather_than_inverting() {
    let (mut controller, _, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });

    // Drag the bottom-right corner far past the top-left anchor.
    let effects = press_drag_release(&mut controller, point(200.0, 300.0), point(0.0, 0.0));

    match effects
        .iter()
        .find(|e| matches!(e, Effect::ScaleSelection { .. }))
    {
        Some(Effect::ScaleSelection { sx, sy, .. }) => {
            assert!(*sx > 0.0, "sx must not go negative, was {sx}");
            assert!(*sy > 0.0, "sy must not go negative, was {sy}");
        }
        other => panic!("expected a scale, got {other:?}"),
    }
}

// --- Deleting and clearing -------------------------------------------------

#[test]
fn deleting_a_selection_asks_for_exactly_those_objects() {
    let (mut controller, first, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });

    let effects = controller.act(Action::DeleteSelection);

    match effects
        .iter()
        .find(|e| matches!(e, Effect::DeleteSelection { .. }))
    {
        Some(Effect::DeleteSelection { ids }) => assert_eq!(ids, &[first]),
        other => panic!("expected a delete, got {other:?}"),
    }
    assert!(
        controller.selection().is_empty(),
        "the selection went with them"
    );
}

#[test]
fn deleting_nothing_does_nothing() {
    let (mut controller, _, _) = selecting();

    let effects = controller.act(Action::DeleteSelection);

    assert!(effects.is_empty());
}

#[test]
fn an_object_that_leaves_the_document_leaves_the_selection() {
    // Undoing a creation must not leave handles floating around something
    // that is no longer there.
    let (mut controller, first, second) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });
    assert_eq!(controller.selection(), &[first]);

    controller.set_scene(vec![(second, rect(400.0, 200.0, 500.0, 300.0))]);

    assert!(controller.selection().is_empty());
    assert!(controller.selection_bounds().is_none());
}

#[test]
fn escape_drops_the_selection_before_it_leaves_draw_mode() {
    let (mut controller, _, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(150.0, 250.0),
    });

    let effects = controller.act(Action::Escape);

    assert!(controller.selection().is_empty());
    assert!(
        effects.is_empty(),
        "Escape should not also change mode: {effects:?}"
    );
    assert_eq!(controller.mode(), ink_app::Mode::Draw);
}

#[test]
fn cancelling_a_move_keeps_the_selection() {
    // Putting the objects back is not the same as deselecting them.
    let (mut controller, first, _) = selecting();
    controller.handle(PlatformEvent::PointerDown {
        at: point(150.0, 250.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(180.0, 280.0),
    });

    controller.act(Action::Escape);

    assert_eq!(controller.selection(), &[first]);
    assert!(controller.selection_drag().is_none(), "the drag is over");
}
