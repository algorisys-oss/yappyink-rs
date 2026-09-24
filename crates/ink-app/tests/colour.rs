//! The colour picker.
//!
//! Requirement: FR-007 (configurable colour), FR-006 (a control that looks
//! clickable is clickable), NFR-006 (a selected state not carried by colour
//! alone).

use ink_app::{Action, Controller, Effect, Icon, PALETTE, PlatformEvent, Toolbar};
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

fn colour_button(controller: &Controller) -> LogicalPoint {
    let bounds = controller
        .toolbar()
        .buttons()
        .iter()
        .find(|b| b.icon == Icon::Color)
        .expect("a colour button")
        .bounds;
    point(
        (bounds.min.x + bounds.max.x) / 2.0,
        (bounds.min.y + bounds.max.y) / 2.0,
    )
}

fn click(controller: &mut Controller, at: LogicalPoint) {
    controller.handle(PlatformEvent::PointerDown { at });
    controller.handle(PlatformEvent::PointerUp { at });
}

#[test]
fn the_picker_starts_closed_and_opens_on_the_button() {
    let mut controller = drawing();
    assert!(!controller.picker_open());
    assert!(controller.swatches().is_empty());

    let button = colour_button(&controller);
    click(&mut controller, button);

    assert!(controller.picker_open());
    assert_eq!(controller.swatches().len(), PALETTE.len());
}

#[test]
fn choosing_a_swatch_sets_the_colour_and_closes_the_row() {
    // A picker that stays open covers the canvas and has to be dismissed
    // separately, which is one interaction too many for choosing a colour.
    let mut controller = drawing();
    let button = colour_button(&controller);
    click(&mut controller, button);

    let (index, colour, rect) = controller.swatches()[3];
    click(
        &mut controller,
        point(
            (rect.min.x + rect.max.x) / 2.0,
            (rect.min.y + rect.max.y) / 2.0,
        ),
    );

    assert_eq!(
        controller.style().color,
        colour,
        "swatch {index} was not applied"
    );
    assert!(!controller.picker_open());
}

#[test]
fn the_button_toggles_rather_than_only_opening() {
    let mut controller = drawing();
    let at = colour_button(&controller);

    click(&mut controller, at);
    click(&mut controller, at);

    assert!(!controller.picker_open());
}

#[test]
fn a_swatch_press_never_draws() {
    // The swatch row hangs over the canvas, so a press on it must be checked
    // before any tool sees it, exactly as the toolbar's is (FR-006).
    let mut controller = drawing();
    let button = colour_button(&controller);
    click(&mut controller, button);
    let (_, _, rect) = controller.swatches()[0];

    controller.handle(PlatformEvent::PointerDown {
        at: point(
            (rect.min.x + rect.max.x) / 2.0,
            (rect.min.y + rect.max.y) / 2.0,
        ),
    });

    assert!(!controller.is_gesturing());
}

#[test]
fn sliding_off_a_swatch_before_releasing_cancels_it() {
    let mut controller = drawing();
    let button = colour_button(&controller);
    click(&mut controller, button);
    let before = controller.style().color;
    let (_, _, rect) = controller.swatches()[2];

    controller.handle(PlatformEvent::PointerDown {
        at: point(
            (rect.min.x + rect.max.x) / 2.0,
            (rect.min.y + rect.max.y) / 2.0,
        ),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(700.0, 700.0),
    });

    assert_eq!(controller.style().color, before);
}

#[test]
fn the_interactive_area_grows_to_cover_an_open_picker() {
    // The input region is set from this. If it did not grow, the swatches
    // would be visible and dead, which is the thing FR-006 forbids.
    let mut controller = drawing();
    let closed = controller.interactive_bounds();

    let button = colour_button(&controller);
    click(&mut controller, button);
    let open = controller.interactive_bounds();

    assert!(
        open.max.y > closed.max.y,
        "the region did not grow downwards"
    );
    for (_, _, rect) in controller.swatches() {
        assert!(
            rect.max.y <= open.max.y,
            "a swatch is outside the interactive area"
        );
        assert!(
            rect.max.x <= open.max.x,
            "a swatch is outside the interactive area"
        );
    }
}

#[test]
fn every_palette_entry_is_reachable_and_distinct() {
    let mut controller = drawing();
    let button = colour_button(&controller);
    click(&mut controller, button);

    let swatches = controller.swatches();
    assert_eq!(swatches.len(), PALETTE.len());
    for (index, (position, colour, _)) in swatches.iter().enumerate() {
        assert_eq!(*position, index);
        assert_eq!(*colour, PALETTE[index]);
    }

    let mut seen: Vec<_> = swatches.iter().map(|(_, c, _)| *c).collect();
    seen.sort_by_key(|c| (c.r, c.g, c.b));
    seen.dedup();
    assert_eq!(
        seen.len(),
        PALETTE.len(),
        "two swatches are the same colour"
    );
}

#[test]
fn the_picker_is_offered_only_where_it_can_be_clicked() {
    // Same rule as the toolbar it hangs from.
    let mut controller = drawing();
    let button = colour_button(&controller);
    click(&mut controller, button);
    assert!(controller.picker_open());

    assert!(Toolbar::is_visible(controller.mode()));
    assert!(!controller.swatches().is_empty());
}
