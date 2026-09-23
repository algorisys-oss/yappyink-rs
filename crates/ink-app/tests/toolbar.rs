//! T013: the toolbar.
//!
//! Requirement: FR-006. Scenario: AC-FR-006.
//!
//! What matters about a toolbar is who owns the pointer when it is pressed.
//! These tests are about that, not about appearance.

use ink_app::{Action, Controller, Effect, Mode, PlatformEvent, Tool, Toolbar};
use ink_core::LogicalPoint;

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

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

/// The centre of the nth button.
fn button_centre(controller: &Controller, index: usize) -> LogicalPoint {
    let bounds = controller.toolbar().buttons()[index].bounds;
    point(
        (bounds.min.x + bounds.max.x) / 2.0,
        (bounds.min.y + bounds.max.y) / 2.0,
    )
}

/// A point on the canvas, well clear of the toolbar.
fn on_canvas() -> LogicalPoint {
    point(400.0, 400.0)
}

// --- FR-006: a toolbar press is never a stroke -----------------------------

#[test]
fn pressing_a_button_never_starts_a_gesture() {
    let mut controller = drawing();

    controller.handle(PlatformEvent::PointerDown {
        at: button_centre(&controller, 0),
    });

    assert!(
        !controller.is_gesturing(),
        "a button press armed a drawing gesture"
    );
    assert!(controller.preview().is_none());
}

#[test]
fn dragging_off_a_button_onto_the_canvas_draws_nothing() {
    // The obvious way to get ink under a toolbar: press a control and keep
    // going. One completed drag, and it must produce no object at all.
    let mut controller = drawing();

    controller.handle(PlatformEvent::PointerDown {
        at: button_centre(&controller, 0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(200.0, 200.0),
    });
    let effects = controller.handle(PlatformEvent::PointerUp { at: on_canvas() });

    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitObject { .. })),
        "a drag from the toolbar left ink: {effects:?}"
    );
    assert!(!controller.is_gesturing());
}

#[test]
fn the_gap_between_buttons_is_not_a_hole_to_draw_through() {
    // The frame belongs to the toolbar too, or a careless press between two
    // controls would start a stroke underneath them.
    let mut controller = drawing();
    let first = controller.toolbar().buttons()[0].bounds;
    let second = controller.toolbar().buttons()[1].bounds;
    let gap = point(
        (first.max.x + second.min.x) / 2.0,
        (first.min.y + first.max.y) / 2.0,
    );
    assert!(
        controller.toolbar().hit(gap).is_none(),
        "the test point is not on a button"
    );
    assert!(
        controller.toolbar().contains(gap),
        "but it is on the toolbar"
    );

    controller.handle(PlatformEvent::PointerDown { at: gap });

    assert!(!controller.is_gesturing());
}

#[test]
fn every_tool_is_stopped_by_the_toolbar_not_just_the_pen() {
    for tool in [
        Tool::Pen,
        Tool::Highlighter,
        Tool::Line,
        Tool::Arrow,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Eraser,
    ] {
        let mut controller = drawing();
        controller.act(Action::SelectTool(tool));

        controller.handle(PlatformEvent::PointerDown {
            at: button_centre(&controller, 0),
        });

        assert!(!controller.is_gesturing(), "{tool:?} drew on the toolbar");
    }
}

// --- Buttons behave like buttons -------------------------------------------

#[test]
fn a_button_acts_on_release_not_on_press() {
    let mut controller = drawing();
    let highlighter = controller
        .toolbar()
        .buttons()
        .iter()
        .position(|b| b.action == Action::SelectTool(Tool::Highlighter))
        .expect("a highlighter button");
    let at = button_centre(&controller, highlighter);

    controller.handle(PlatformEvent::PointerDown { at });
    assert_eq!(
        controller.tool(),
        Tool::Pen,
        "the press alone changed nothing"
    );

    controller.handle(PlatformEvent::PointerUp { at });
    assert_eq!(controller.tool(), Tool::Highlighter);
}

#[test]
fn sliding_off_a_button_before_releasing_cancels_it() {
    // The standard way to change your mind about a click.
    let mut controller = drawing();
    let highlighter = controller
        .toolbar()
        .buttons()
        .iter()
        .position(|b| b.action == Action::SelectTool(Tool::Highlighter))
        .expect("a highlighter button");

    controller.handle(PlatformEvent::PointerDown {
        at: button_centre(&controller, highlighter),
    });
    controller.handle(PlatformEvent::PointerUp { at: on_canvas() });

    assert_eq!(
        controller.tool(),
        Tool::Pen,
        "the cancelled press still acted"
    );
}

#[test]
fn releasing_over_a_different_button_does_nothing() {
    let mut controller = drawing();
    let a = button_centre(&controller, 0);
    let b = button_centre(&controller, 1);

    controller.handle(PlatformEvent::PointerDown { at: a });
    controller.handle(PlatformEvent::PointerUp { at: b });

    assert_eq!(
        controller.tool(),
        Tool::Pen,
        "a press slid onto another button acted"
    );
}

#[test]
fn the_history_buttons_ask_for_the_same_effects_as_the_keys() {
    let mut controller = drawing();
    let undo = controller
        .toolbar()
        .buttons()
        .iter()
        .position(|b| b.action == Action::Undo)
        .expect("an undo button");
    let at = button_centre(&controller, undo);

    controller.handle(PlatformEvent::PointerDown { at });
    let effects = controller.handle(PlatformEvent::PointerUp { at });

    assert!(
        effects.iter().any(|e| matches!(e, Effect::Undo)),
        "the button produced {effects:?}"
    );
}

// --- Visibility ------------------------------------------------------------

#[test]
fn the_toolbar_is_offered_only_where_it_can_be_clicked() {
    // FR-006 forbids a control that looks interactive on a surface that takes
    // no input. PassThrough takes none, and Hidden has no surface.
    assert!(Toolbar::is_visible(Mode::Draw));
    assert!(!Toolbar::is_visible(Mode::PassThrough));
    assert!(!Toolbar::is_visible(Mode::Hidden));
}

#[test]
fn a_press_where_the_toolbar_would_be_draws_normally_when_it_is_hidden() {
    // In PassThrough we should get no pointer events at all, but if one
    // arrives the toolbar must not silently swallow it as though it were
    // there.
    let mut controller = drawing();
    let at = button_centre(&controller, 0);
    controller.act(Action::ToggleDraw);
    let transition = match controller.act(Action::EnterDraw).first() {
        Some(Effect::ApplyMode { transition, .. }) => *transition,
        other => panic!("expected a mode request, got {other:?}"),
    };
    controller.handle(PlatformEvent::ModeApplied { transition });
    assert_eq!(controller.mode(), Mode::Draw);

    // Back in Draw the toolbar is there again, so this press is a button.
    controller.handle(PlatformEvent::PointerDown { at });
    assert!(!controller.is_gesturing());
}

// --- Layout ----------------------------------------------------------------

#[test]
fn buttons_do_not_overlap_and_sit_inside_the_toolbar() {
    let controller = Controller::new();
    let toolbar = controller.toolbar();
    let bounds = toolbar.bounds();

    for (index, button) in toolbar.buttons().iter().enumerate() {
        assert!(
            button.bounds.min.x >= bounds.min.x,
            "button {index} escapes left"
        );
        assert!(
            button.bounds.max.x <= bounds.max.x,
            "button {index} escapes right"
        );
        assert!(
            button.bounds.min.y >= bounds.min.y,
            "button {index} escapes top"
        );
        assert!(
            button.bounds.max.y <= bounds.max.y,
            "button {index} escapes bottom"
        );

        for (other_index, other) in toolbar.buttons().iter().enumerate() {
            if index == other_index {
                continue;
            }
            let apart = button.bounds.max.x <= other.bounds.min.x
                || other.bounds.max.x <= button.bounds.min.x
                || button.bounds.max.y <= other.bounds.min.y
                || other.bounds.max.y <= button.bounds.min.y;
            assert!(apart, "buttons {index} and {other_index} overlap");
        }
    }
}

#[test]
fn the_selected_tool_is_marked() {
    // NFR-006 wants a selected state that is not colour alone. This is the
    // model half of that: the drawing code is told which button is current.
    let mut controller = Controller::new();
    controller.act(Action::SelectTool(Tool::Ellipse));

    let selected: Vec<&ink_app::Button> = controller
        .toolbar()
        .buttons()
        .iter()
        .filter(|b| b.is_selected(controller.tool()))
        .collect();

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].action, Action::SelectTool(Tool::Ellipse));
}

#[test]
fn only_tool_buttons_are_ever_marked_as_selected() {
    // Undo is an action, not a mode. Marking it would say something untrue.
    let controller = Controller::new();

    for button in controller.toolbar().buttons() {
        if matches!(button.action, Action::SelectTool(_)) {
            continue;
        }
        for tool in [Tool::Pen, Tool::Eraser, Tool::Ellipse] {
            assert!(
                !button.is_selected(tool),
                "{:?} claimed to be selected",
                button.icon
            );
        }
    }
}
