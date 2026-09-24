//! The chrome, painted and inspected.
//!
//! Requirements: FR-024 (chrome is not document content), and the visibility
//! the overlay depends on to be usable at all.
//!
//! **These tests could not exist a commit ago.** All of this lived inside the
//! Wayland adapter, where reaching it needed a compositor. Every bug in
//! `docs/learning.md` §1 was in this layer and every one was found by a person
//! rather than by a test. Moving it into `ink-ui` is what makes the code below
//! possible, and the point of these particular cases is to cover the shapes of
//! failure that actually happened.

use ink_app::{Action, Controller, Mode, Tool};
use ink_core::{Document, LogicalSize, OutputId};
use ink_render::{Canvas, Painter, Scale};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;

fn blank() -> Vec<u8> {
    vec![0u8; (WIDTH * HEIGHT * 4) as usize]
}

fn painted(canvas: &Canvas) -> usize {
    (0..canvas.height())
        .flat_map(|y| (0..canvas.width()).map(move |x| (x, y)))
        .filter(|(x, y)| canvas.pixel(*x, *y).unwrap()[3] > 0)
        .count()
}

fn document() -> Document {
    Document::new(
        OutputId::new("test"),
        LogicalSize::new(f64::from(WIDTH), f64::from(HEIGHT)).unwrap(),
    )
}

/// The first Wayland build was a fully transparent, undecorated window. It
/// could not be located on screen, so there was nowhere to aim the pointer,
/// and it was handed over as working (`docs/learning.md` §2).
///
/// This is that bug, as a test.
///
/// `Mode::Draw` is passed explicitly rather than taken from a fresh controller,
/// which starts in `Hidden`. Writing it the other way first made this fail, and
/// the code was right: a hidden overlay has no surface, so painting a frame for
/// it would be painting onto nothing (`docs/learning.md` §4).
#[test]
fn the_overlay_can_be_seen() {
    let controller = Controller::new();
    let mut pixels = blank();
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();

    ink_ui::paint_chrome(
        &mut canvas,
        Mode::Draw,
        controller.style(),
        controller.tool(),
    );

    assert!(
        painted(&canvas) > 100,
        "nothing was drawn, so the overlay would be invisible and unusable"
    );
}

/// The middle has to stay clear, or the overlay is a sheet over the screen
/// rather than something you can see through. FR-001.
#[test]
fn the_middle_stays_transparent() {
    let controller = Controller::new();
    let mut pixels = blank();
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();

    ink_ui::paint_chrome(
        &mut canvas,
        Mode::Draw,
        controller.style(),
        controller.tool(),
    );

    let centre = canvas.pixel(WIDTH / 2, HEIGHT / 2).unwrap();
    assert_eq!(
        centre[3], 0,
        "the centre of the overlay is opaque, so the desktop underneath is hidden"
    );
}

/// Changing mode has to change what is on screen, or the user cannot tell
/// whether their next click will draw or go through to the application below.
///
/// This is the shape of the repaint bugs in §1: state changed, screen did not.
/// Here it is checked at the level where the drawing actually happens.
#[test]
fn each_mode_looks_different() {
    let mut seen: Vec<(Mode, Vec<u8>)> = Vec::new();

    for mode in [Mode::Draw, Mode::PassThrough, Mode::Parked] {
        let controller = Controller::new();
        let mut pixels = blank();
        {
            let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();
            ink_ui::paint_chrome(&mut canvas, mode, controller.style(), controller.tool());
        }
        seen.push((mode, pixels));
    }

    for (i, (mode_a, a)) in seen.iter().enumerate() {
        for (mode_b, b) in seen.iter().skip(i + 1) {
            assert_ne!(
                a, b,
                "{mode_a:?} and {mode_b:?} paint identically, so the mode is invisible"
            );
        }
    }
}

/// Selecting a tool has to be visible on the toolbar.
///
/// The owner reported this one: "tool should be selected when I click". The
/// repaint snapshot did not include the tool, so the highlight did not move
/// until the next stroke (`docs/learning.md` §1).
#[test]
fn the_selected_tool_is_visible_on_the_toolbar() {
    let mut pen = blank();
    let mut eraser = blank();

    for (tool, buffer) in [(Tool::Pen, &mut pen), (Tool::Eraser, &mut eraser)] {
        let mut controller = Controller::new();
        controller.act(Action::SelectTool(tool));
        let mut canvas = Canvas::new(buffer, WIDTH, HEIGHT).unwrap();
        ink_ui::paint_toolbar(
            &mut canvas,
            controller.toolbar(),
            controller.tool(),
            controller.style().color,
            Scale::ONE,
        );
    }

    assert_ne!(
        pen, eraser,
        "the toolbar looks the same whichever tool is selected"
    );
}

/// The colour button shows the colour you are about to draw with, so cycling
/// the colour has to change it.
#[test]
fn the_colour_button_follows_the_colour() {
    let mut before = blank();
    let mut after = blank();

    let mut controller = Controller::new();
    let first = controller.style().color;
    {
        let mut canvas = Canvas::new(&mut before, WIDTH, HEIGHT).unwrap();
        ink_ui::paint_toolbar(
            &mut canvas,
            controller.toolbar(),
            controller.tool(),
            first,
            Scale::ONE,
        );
    }

    controller.act(Action::CycleColor);
    let second = controller.style().color;
    assert_ne!(first, second, "cycling did not change the colour at all");
    {
        let mut canvas = Canvas::new(&mut after, WIDTH, HEIGHT).unwrap();
        ink_ui::paint_toolbar(
            &mut canvas,
            controller.toolbar(),
            controller.tool(),
            second,
            Scale::ONE,
        );
    }

    assert_ne!(
        before, after,
        "the toolbar does not show the current colour"
    );
}

/// Chrome is painted over the document and never into it (FR-024).
///
/// An ink-only export has to be able to exclude all of this, which is only
/// true if none of it can reach the document. The signature already enforces
/// that — `paint_selection` takes `&Document` — so this pins the guarantee
/// against someone later deciding a `&mut` would be convenient.
#[test]
fn chrome_never_modifies_the_document() {
    let mut controller = Controller::new();
    controller.act(Action::SelectTool(Tool::Select));
    let document = document();
    let before = document.len();

    let mut pixels = blank();
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();
    let mut painter = Painter::new();
    ink_ui::paint_selection(
        &mut canvas,
        &controller,
        &document,
        &mut painter,
        Scale::ONE,
    );

    assert_eq!(document.len(), before);
}

/// A surface can legitimately be tiny — parked, or mid-resize — and the chrome
/// must clip rather than panic.
///
/// Worth its own test because every one of these functions computes pixel
/// coordinates from logical ones and a subtraction that goes negative is the
/// obvious way to get an out-of-bounds write.
#[test]
fn a_surface_smaller_than_its_chrome_does_not_panic() {
    for (width, height) in [(1, 1), (4, 4), (20, 8), (120, 30)] {
        let controller = Controller::new();
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let mut canvas = Canvas::new(&mut pixels, width, height).unwrap();

        ink_ui::paint_chrome(
            &mut canvas,
            Mode::Draw,
            controller.style(),
            controller.tool(),
        );
        ink_ui::paint_toolbar(
            &mut canvas,
            controller.toolbar(),
            controller.tool(),
            controller.style().color,
            Scale::ONE,
        );
        ink_ui::paint_swatches(&mut canvas, &controller, Scale::ONE);
    }
}

/// The same at a scaled DPI, which multiplies every coordinate and is where an
/// overflow would show up first.
#[test]
fn a_scaled_surface_does_not_panic() {
    for factor in [0.5, 1.5, 2.0, 3.0] {
        let scale = Scale::new(factor).unwrap();
        let controller = Controller::new();
        let mut pixels = blank();
        let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();

        ink_ui::paint_toolbar(
            &mut canvas,
            controller.toolbar(),
            controller.tool(),
            controller.style().color,
            scale,
        );
        ink_ui::paint_swatches(&mut canvas, &controller, scale);
    }
}
