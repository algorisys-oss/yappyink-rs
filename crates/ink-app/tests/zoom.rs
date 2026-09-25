//! Live zoom in the controller (FR-029, specs/004-live-zoom).
//!
//! The controller only decides the level; the adapter drives the platform's
//! magnifier. These pin the level sequence, that zoom is independent of mode,
//! and that a platform without a magnifier neither shows a button nor pretends.

use ink_app::keymap::{self, Key};
use ink_app::{Action, Controller, Effect, Icon, PlatformEvent, ZOOM_STEPS};

fn zoom_effect(effects: &[Effect]) -> Option<Option<f64>> {
    effects.iter().find_map(|e| match e {
        Effect::Zoom { factor } => Some(*factor),
        _ => None,
    })
}

#[test]
fn without_a_magnifier_there_is_no_button_and_the_key_says_so() {
    let mut controller = Controller::new();
    assert!(
        !controller
            .toolbar()
            .buttons()
            .iter()
            .any(|b| b.icon == Icon::Zoom),
        "a zoom button on a platform that cannot zoom does nothing (FR-006)"
    );
    let effects = controller.act(Action::CycleZoom);
    assert!(matches!(effects.as_slice(), [Effect::ZoomUnavailable]));
    assert_eq!(controller.zoom(), None);
}

#[test]
fn offering_zoom_adds_the_button() {
    let mut controller = Controller::new();
    let before = controller.toolbar().buttons().len();
    controller.offer_zoom();
    let buttons = controller.toolbar().buttons();
    assert_eq!(buttons.len(), before + 1);
    let zoom = buttons
        .iter()
        .find(|b| b.icon == Icon::Zoom)
        .expect("a zoom button");
    assert_eq!(zoom.action, Action::CycleZoom);
}

/// Adding a button must not make any two overlap, or a click could reach the
/// wrong one.
#[test]
fn the_zoom_button_fits_without_overlapping() {
    let mut controller = Controller::new();
    controller.offer_zoom();
    let buttons = controller.toolbar().buttons();
    for (i, a) in buttons.iter().enumerate() {
        for b in &buttons[i + 1..] {
            let apart = a.bounds.max.x <= b.bounds.min.x || b.bounds.max.x <= a.bounds.min.x;
            assert!(apart, "{:?} overlaps {:?}", a.icon, b.icon);
        }
        let toolbar = controller.toolbar().bounds();
        assert!(
            a.bounds.max.x <= toolbar.max.x,
            "{:?} is outside the toolbar",
            a.icon
        );
    }
}

#[test]
fn the_levels_step_through_and_back_to_off() {
    let mut controller = Controller::new();
    controller.offer_zoom();
    let mut seen = Vec::new();
    for _ in 0..=ZOOM_STEPS.len() {
        let effects = controller.act(Action::CycleZoom);
        let factor = zoom_effect(&effects).expect("each press asks for a level");
        assert_eq!(factor, controller.zoom());
        seen.push(factor);
    }
    assert_eq!(seen, vec![Some(2.0), Some(3.0), Some(4.0), None]);
}

#[test]
fn zoom_off_goes_straight_back() {
    let mut controller = Controller::new();
    controller.offer_zoom();
    controller.act(Action::CycleZoom);
    controller.act(Action::CycleZoom);
    let effects = controller.act(Action::ZoomOff);
    assert_eq!(zoom_effect(&effects), Some(None));
    assert_eq!(controller.zoom(), None);
    // And the next press starts again at the first step.
    controller.act(Action::CycleZoom);
    assert_eq!(controller.zoom(), Some(ZOOM_STEPS[0]));
}

/// The spec: zoom works in any mode. A fresh controller is Hidden, and a
/// presenter zooming while passing through must not be refused either.
#[test]
fn zoom_does_not_depend_on_the_mode() {
    let mut controller = Controller::new();
    controller.offer_zoom();
    assert_eq!(
        zoom_effect(&controller.act(Action::CycleZoom)),
        Some(Some(2.0))
    );

    let effects = controller.act(Action::ToggleDraw);
    if let Some(transition) = effects.iter().find_map(|e| match e {
        Effect::ApplyMode { transition, .. } => Some(*transition),
        _ => None,
    }) {
        controller.handle(PlatformEvent::ModeApplied { transition });
    }
    assert_eq!(
        zoom_effect(&controller.act(Action::CycleZoom)),
        Some(Some(3.0))
    );
}

#[test]
fn z_is_the_zoom_key() {
    assert_eq!(keymap::command(Key::Char('z')), Some(Action::CycleZoom));
}

/// One key back to normal, whatever the level: stepping round with `z` takes
/// up to three presses.
#[test]
fn zero_resets_zoom_in_one_press() {
    assert_eq!(keymap::command(Key::Char('0')), Some(Action::ZoomOff));
}
