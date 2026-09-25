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
    // no input. The toolbar is now pinned in PassThrough, and it is honest
    // there because the input region is the toolbar's own rectangle: the
    // buttons really are clickable. Hidden has no surface at all.
    assert!(Toolbar::is_visible(Mode::Draw));
    assert!(Toolbar::is_visible(Mode::PassThrough));
    assert!(!Toolbar::is_visible(Mode::Hidden));
}

#[test]
fn the_canvas_is_live_only_in_draw() {
    // The other half of the pinned toolbar: in PassThrough the toolbar is
    // ours and the canvas is not.
    assert!(Toolbar::canvas_is_interactive(Mode::Draw));
    assert!(!Toolbar::canvas_is_interactive(Mode::PassThrough));
    assert!(!Toolbar::canvas_is_interactive(Mode::Hidden));
}

#[test]
fn the_toolbar_still_works_in_passthrough() {
    // The point of pinning it: switching tools and undoing without having to
    // leave PassThrough, which today means reaching for a terminal because the
    // application underneath has the keyboard.
    let mut controller = drawing();
    controller.act(Action::ToggleDraw);
    let transition = match controller.act(Action::ToggleDraw).first() {
        Some(Effect::ApplyMode { transition, .. }) => *transition,
        _ => panic!("expected a mode request"),
    };
    controller.handle(PlatformEvent::ModeApplied { transition });
    // Two toggles from Draw land back in Draw, so go to PassThrough once more.
    let transition = match controller.act(Action::ToggleDraw).first() {
        Some(Effect::ApplyMode { transition, .. }) => *transition,
        _ => panic!("expected a mode request"),
    };
    controller.handle(PlatformEvent::ModeApplied { transition });
    assert_eq!(controller.mode(), Mode::PassThrough);

    let undo = controller
        .toolbar()
        .buttons()
        .iter()
        .position(|b| b.action == Action::Undo)
        .expect("an undo button");
    let at = button_centre(&controller, undo);
    controller.handle(PlatformEvent::PointerDown { at });
    let effects = controller.handle(PlatformEvent::PointerUp { at });

    assert!(effects.iter().any(|e| matches!(e, Effect::Undo)));
}

#[test]
fn the_canvas_draws_nothing_in_passthrough() {
    let mut controller = drawing();
    let transition = match controller.act(Action::ToggleDraw).first() {
        Some(Effect::ApplyMode { transition, .. }) => *transition,
        _ => panic!("expected a mode request"),
    };
    controller.handle(PlatformEvent::ModeApplied { transition });
    assert_eq!(controller.mode(), Mode::PassThrough);

    controller.handle(PlatformEvent::PointerDown { at: on_canvas() });
    let effects = controller.handle(PlatformEvent::PointerUp { at: on_canvas() });

    assert!(!controller.is_gesturing());
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::CommitObject { .. }))
    );
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

// --- Moving and resizing the overlay ---------------------------------------

#[test]
fn pressing_the_grip_asks_the_compositor_to_move_the_window() {
    let mut controller = drawing();
    let grip = controller.toolbar().grip();
    let at = point(
        (grip.min.x + grip.max.x) / 2.0,
        (grip.min.y + grip.max.y) / 2.0,
    );

    let effects = controller.handle(PlatformEvent::PointerDown { at });

    assert!(effects.iter().any(|e| matches!(e, Effect::BeginWindowDrag)));
    assert!(!controller.is_gesturing(), "the grip must not arm a stroke");
}

#[test]
fn the_grip_is_on_the_toolbar_but_is_not_a_button() {
    let controller = Controller::new();
    let grip = controller.toolbar().grip();
    let at = point(
        (grip.min.x + grip.max.x) / 2.0,
        (grip.min.y + grip.max.y) / 2.0,
    );

    assert!(controller.toolbar().contains(at));
    assert!(
        controller.toolbar().hit(at).is_none(),
        "the grip is not a control"
    );
    for button in controller.toolbar().buttons() {
        let overlaps = !(button.bounds.max.x <= grip.min.x
            || grip.max.x <= button.bounds.min.x
            || button.bounds.max.y <= grip.min.y
            || grip.max.y <= button.bounds.min.y);
        assert!(!overlaps, "{:?} overlaps the grip", button.icon);
    }
}

#[test]
fn there_is_no_resize_corner_until_the_surface_size_is_known() {
    // The compositor decides the size. Guessing one would put the grab area
    // somewhere arbitrary.
    let controller = Controller::new();
    assert!(controller.resize_corner().is_none());
}

#[test]
fn pressing_the_bottom_right_corner_asks_the_compositor_to_resize() {
    let mut controller = drawing();
    controller.set_surface_size(ink_core::LogicalSize::new(800.0, 600.0).unwrap());
    let corner = controller.resize_corner().expect("a corner");
    let at = point(
        (corner.min.x + corner.max.x) / 2.0,
        (corner.min.y + corner.max.y) / 2.0,
    );

    let effects = controller.handle(PlatformEvent::PointerDown { at });

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::BeginWindowResize))
    );
    assert!(
        !controller.is_gesturing(),
        "the corner must not arm a stroke"
    );
}

/// Parked shrinks the surface to the toolbar, so a corner would sit on the
/// buttons and a press meant for one would start a resize instead.
#[test]
fn there_is_no_resize_corner_while_parked() {
    let mut controller = drawing();
    controller.set_surface_size(ink_core::LogicalSize::new(800.0, 600.0).unwrap());
    assert!(controller.resize_corner().is_some());

    let effects = controller.act(Action::TogglePark);
    let transition = effects
        .iter()
        .find_map(|e| match e {
            Effect::ApplyMode { transition, .. } => Some(*transition),
            _ => None,
        })
        .expect("TogglePark requests a mode");
    controller.handle(PlatformEvent::ModeApplied { transition });

    assert!(controller.resize_corner().is_none());
}

#[test]
fn the_canvas_next_to_the_resize_corner_still_draws() {
    // The grab area has to be small enough that it does not eat the drawing
    // surface around it.
    let mut controller = drawing();
    controller.set_surface_size(ink_core::LogicalSize::new(800.0, 600.0).unwrap());
    let corner = controller.resize_corner().expect("a corner");

    let effects = controller.handle(PlatformEvent::PointerDown {
        at: point(corner.min.x - 5.0, corner.min.y - 5.0),
    });

    assert!(effects.is_empty());
    assert!(
        controller.is_gesturing(),
        "just outside the corner is canvas"
    );
}

#[test]
fn neither_handle_is_offered_where_the_surface_takes_no_input() {
    // Same rule as the toolbar: a handle that cannot be grabbed should not be
    // acting as though it can.
    let mut controller = drawing();
    controller.set_surface_size(ink_core::LogicalSize::new(800.0, 600.0).unwrap());
    let corner = controller.resize_corner().expect("a corner");
    let grip = controller.toolbar().grip();
    controller.act(Action::ToggleDraw);
    let transition = match controller.act(Action::EnterDraw).first() {
        Some(Effect::ApplyMode { transition, .. }) => *transition,
        other => panic!("expected a mode request, got {other:?}"),
    };
    controller.handle(PlatformEvent::ModeApplied { transition });

    // Back in Draw both are live again, which is what the earlier tests cover.
    // The point here is that the geometry does not move between modes.
    assert_eq!(controller.resize_corner(), Some(corner));
    assert_eq!(controller.toolbar().grip().min, grip.min);
}

// --- Tooltips --------------------------------------------------------------

#[test]
fn resting_on_a_button_offers_its_tooltip() {
    let mut controller = drawing();
    let at = button_centre(&controller, 0);

    controller.handle(PlatformEvent::PointerMoved { at });

    let hovered = controller.hovered_button().expect("a hovered button");
    assert_eq!(hovered.icon, controller.toolbar().buttons()[0].icon);
    assert!(!hovered.label().is_empty());
}

#[test]
fn moving_onto_the_canvas_drops_the_tooltip() {
    let mut controller = drawing();
    controller.handle(PlatformEvent::PointerMoved {
        at: button_centre(&controller, 0),
    });
    assert!(controller.hovered_button().is_some());

    controller.handle(PlatformEvent::PointerMoved { at: on_canvas() });

    assert!(controller.hovered_button().is_none());
}

#[test]
fn no_tooltip_appears_while_a_stroke_is_being_drawn() {
    // A drag that passes under the toolbar should not raise a tooltip over
    // the ink being drawn.
    let mut controller = drawing();
    controller.handle(PlatformEvent::PointerDown { at: on_canvas() });
    controller.handle(PlatformEvent::PointerMoved {
        at: button_centre(&controller, 0),
    });

    assert!(controller.is_gesturing());
    assert!(controller.hovered_button().is_none());
}

#[test]
fn every_button_has_a_label_naming_its_key() {
    let controller = Controller::new();

    for button in controller.toolbar().buttons() {
        let label = button.label();
        assert!(!label.is_empty(), "{:?} has no label", button.icon);
        assert!(
            label.contains('(') && label.contains(')'),
            "{label:?} does not name a key"
        );
        // The label font has no lowercase, so a lowercase label would render
        // as capitals anyway and the two would silently disagree.
        assert_eq!(label, label.to_uppercase(), "{label:?} is not uppercase");
    }
}

#[test]
fn the_window_menu_button_asks_for_the_compositors_menu() {
    // Mutter gives a client no way to set Always on Top. Asking for the menu
    // where the user can is the only honest route from inside the app.
    let mut controller = drawing();
    let index = controller
        .toolbar()
        .buttons()
        .iter()
        .position(|b| b.action == Action::ShowWindowMenu)
        .expect("a window-menu button");
    let at = button_centre(&controller, index);

    controller.handle(PlatformEvent::PointerDown { at });
    let effects = controller.handle(PlatformEvent::PointerUp { at });

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::ShowWindowMenu { .. })),
        "the button produced {effects:?}"
    );
}

#[test]
fn the_window_menu_opens_under_the_button_that_asked_for_it() {
    let mut controller = drawing();
    let button = *controller
        .toolbar()
        .buttons()
        .iter()
        .find(|b| b.action == Action::ShowWindowMenu)
        .expect("a window-menu button");

    let effects = controller.act(Action::ShowWindowMenu);

    match effects.first() {
        Some(Effect::ShowWindowMenu { at }) => {
            assert_eq!(at.x, button.bounds.min.x);
            assert_eq!(at.y, button.bounds.max.y, "below the button, not over it");
        }
        other => panic!("expected a menu request, got {other:?}"),
    }
}

// --- Parked: shrink to the toolbar -----------------------------------------

fn settle(controller: &mut Controller, action: Action) {
    let effects = controller.act(action);
    if let Some(Effect::ApplyMode { transition, .. }) = effects
        .iter()
        .find(|e| matches!(e, Effect::ApplyMode { .. }))
    {
        let transition = *transition;
        controller.handle(PlatformEvent::ModeApplied { transition });
    }
}

#[test]
fn parking_keeps_the_toolbar_and_drops_the_ink() {
    let mut controller = drawing();

    settle(&mut controller, Action::TogglePark);

    assert_eq!(controller.mode(), Mode::Parked);
    assert!(
        Toolbar::is_visible(Mode::Parked),
        "the controls stay reachable"
    );
    assert!(
        !Toolbar::ink_is_visible(Mode::Parked),
        "there is nowhere to draw it"
    );
    assert!(!Toolbar::canvas_is_interactive(Mode::Parked));
}

#[test]
fn coming_back_from_parked_goes_to_draw() {
    // The reason to come back is to draw. PassThrough is one more press away.
    let mut controller = drawing();
    settle(&mut controller, Action::TogglePark);

    settle(&mut controller, Action::TogglePark);

    assert_eq!(controller.mode(), Mode::Draw);
}

#[test]
fn the_toolbar_still_works_while_parked() {
    let mut controller = drawing();
    settle(&mut controller, Action::TogglePark);

    let undo = controller
        .toolbar()
        .buttons()
        .iter()
        .position(|b| b.action == Action::Undo)
        .expect("an undo button");
    let at = button_centre(&controller, undo);
    controller.handle(PlatformEvent::PointerDown { at });
    let effects = controller.handle(PlatformEvent::PointerUp { at });

    assert!(effects.iter().any(|e| matches!(e, Effect::Undo)));
}

#[test]
fn parking_does_not_touch_the_document() {
    // Same promise as Hidden: the annotations are still there when you come
    // back, they are simply not on screen.
    let mut controller = drawing();
    controller.handle(PlatformEvent::PointerDown { at: on_canvas() });
    let committed = controller.handle(PlatformEvent::PointerUp { at: on_canvas() });
    assert!(
        committed
            .iter()
            .any(|e| matches!(e, Effect::CommitObject { .. }))
    );

    let effects = controller.act(Action::TogglePark);

    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::Clear | Effect::DeleteSelection { .. })),
        "parking removed something: {effects:?}"
    );
}

#[test]
fn quitting_saves_first() {
    // The quit button sits beside hide and pass-through. A misclick should not
    // cost a session's work.
    let mut controller = drawing();

    let effects = controller.act(Action::Quit);

    let save = effects.iter().position(|e| matches!(e, Effect::Save));
    let quit = effects.iter().position(|e| matches!(e, Effect::Quit));
    assert!(
        save.is_some() && quit.is_some(),
        "expected both: {effects:?}"
    );
    assert!(save < quit, "the save must be ordered before the exit");
}
