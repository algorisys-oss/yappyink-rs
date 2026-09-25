//! The gesture in flight, painted and inspected.
//!
//! Requirements: FR-007 (a stroke is visible while it is drawn), FR-023 (text
//! is visible while it is typed, composition included).
//!
//! `paint_preview` was lifted out of the Wayland adapter because the Windows
//! and macOS adapters each had their own copy, and both copies skipped text.
//! Typed text was invisible on those platforms until Return committed it. These
//! tests pin the shared version so a fourth backend cannot quietly do the same.

use ink_app::{Action, Controller, Effect, PlatformEvent, Tool};
use ink_core::{LogicalPoint, LogicalSize, Object, ObjectId, OutputId, Session, Shape, StrokeKind};
use ink_render::text::TextFont;
use ink_render::{Canvas, Painter, Scale};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

fn drawing(tool: Tool) -> Controller {
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
    controller.act(Action::SelectTool(tool));
    controller
}

/// Paints the preview alone onto a blank canvas and counts what it touched.
fn painted_by_preview(controller: &Controller, painter: &mut Painter) -> usize {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();
    ink_ui::paint_preview(
        &mut canvas,
        controller,
        &OutputId::new("test"),
        painter,
        Scale::ONE,
    );
    pixels.chunks_exact(4).filter(|pixel| pixel[3] > 0).count()
}

/// A painter that can draw text, or `None` on a machine with no usable font.
///
/// Text needs a real face, and a test machine may not have one. Returning
/// early is not a pass in disguise: it says so on stderr, and CI's Linux image
/// has DejaVu, so the assertion does run there.
fn painter_with_font() -> Option<Painter> {
    match TextFont::discover() {
        Ok(font) => Some(Painter::new().with_font(font)),
        Err(reason) => {
            eprintln!("skipped: no usable font on this machine ({reason})");
            None
        }
    }
}

#[test]
fn nothing_in_flight_paints_nothing() {
    let controller = drawing(Tool::Pen);
    assert_eq!(painted_by_preview(&controller, &mut Painter::new()), 0);
}

#[test]
fn a_stroke_is_visible_while_it_is_drawn() {
    let mut controller = drawing(Tool::Pen);
    controller.handle(PlatformEvent::PointerDown {
        at: point(100.0, 150.0),
    });
    controller.handle(PlatformEvent::PointerMoved {
        at: point(250.0, 160.0),
    });

    assert!(
        painted_by_preview(&controller, &mut Painter::new()) > 100,
        "a stroke being dragged drew nothing, so it would only appear on release"
    );
}

/// The bug this function was lifted to fix: an open editor with text in it
/// drew nothing on Windows or macOS.
#[test]
fn text_is_visible_while_it_is_typed() {
    let Some(mut painter) = painter_with_font() else {
        return;
    };
    let mut controller = drawing(Tool::Text);
    controller.handle(PlatformEvent::PointerDown {
        at: point(50.0, 100.0),
    });
    controller.handle(PlatformEvent::PointerUp {
        at: point(50.0, 100.0),
    });
    assert!(
        controller.is_editing_text(),
        "the click did not open an editor"
    );

    let empty = painted_by_preview(&controller, &mut painter);
    assert!(
        empty > 0,
        "an empty editor drew nothing; the caret is what shows it opened"
    );

    for character in "Hello".chars() {
        controller.act(Action::TypeText(character));
    }
    assert!(
        painted_by_preview(&controller, &mut painter) > empty,
        "typing did not add anything to the screen"
    );
}

/// The same characters, once as composed text and once as typed text. The only
/// difference on screen should be the underline marking what is provisional.
#[test]
fn a_composition_is_underlined() {
    let Some(mut painter) = painter_with_font() else {
        return;
    };
    let open = || {
        let mut controller = drawing(Tool::Text);
        controller.handle(PlatformEvent::PointerDown {
            at: point(50.0, 100.0),
        });
        controller.handle(PlatformEvent::PointerUp {
            at: point(50.0, 100.0),
        });
        controller.act(Action::TypeText('a'));
        controller
    };

    let mut typed = open();
    typed.act(Action::TypeText('b'));

    let mut composing = open();
    composing.handle(PlatformEvent::Preedit("b".to_owned()));

    assert!(
        painted_by_preview(&composing, &mut painter) > painted_by_preview(&typed, &mut painter),
        "a composition looked exactly like committed text"
    );
}

fn session_with_one_stroke(controller: &Controller) -> Session {
    let output = OutputId::new("test");
    let mut session = Session::new(
        output.clone(),
        LogicalSize::new(f64::from(WIDTH), f64::from(HEIGHT)).unwrap(),
    );
    let shape = Shape::stroke(
        StrokeKind::Pen,
        vec![point(100.0, 150.0), point(300.0, 150.0)],
    )
    .unwrap();
    session
        .add(Object::new(
            ObjectId::from_raw(1),
            output,
            controller.style(),
            shape,
        ))
        .unwrap();
    session
}

fn painted_by_document(controller: &Controller, session: &Session) -> usize {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();
    ink_ui::paint_document(
        &mut canvas,
        controller,
        session.document(),
        &mut Painter::new(),
        Scale::ONE,
    );
    pixels.chunks_exact(4).filter(|pixel| pixel[3] > 0).count()
}

/// Parked keeps the document and does not show it. The Windows and macOS
/// adapters painted it anyway before this function was shared.
#[test]
fn ink_is_drawn_in_draw_mode_and_not_while_parked() {
    let mut controller = drawing(Tool::Pen);
    let session = session_with_one_stroke(&controller);
    assert!(painted_by_document(&controller, &session) > 100);

    let effects = controller.act(Action::TogglePark);
    let transition = effects
        .iter()
        .find_map(|e| match e {
            Effect::ApplyMode { transition, .. } => Some(*transition),
            _ => None,
        })
        .expect("TogglePark requests a mode");
    controller.handle(PlatformEvent::ModeApplied { transition });
    assert_eq!(controller.mode(), ink_app::Mode::Parked);
    assert_eq!(painted_by_document(&controller, &session), 0);
}
