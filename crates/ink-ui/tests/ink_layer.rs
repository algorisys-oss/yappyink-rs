//! The cached document raster.
//!
//! Requirements: NFR-001 (it exists to keep a frame inside budget), FR-007 (it
//! must not change what the ink looks like).
//!
//! A cache has two ways to be wrong: show stale ink, or not save anything.
//! Both are tested, and so is the claim that it draws exactly what painting
//! the document directly draws.

use ink_app::{Action, Controller, Effect, PlatformEvent, Tool};
use ink_core::{LogicalPoint, LogicalSize, Object, ObjectId, OutputId, Session, Shape, StrokeKind};
use ink_render::{Canvas, Painter, Scale};
use ink_ui::InkLayer;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).unwrap()
}

fn applied(controller: &mut Controller, effects: Vec<Effect>) {
    let transition = effects
        .iter()
        .find_map(|e| match e {
            Effect::ApplyMode { transition, .. } => Some(*transition),
            _ => None,
        })
        .expect("a mode was requested");
    controller.handle(PlatformEvent::ModeApplied { transition });
}

fn drawing() -> Controller {
    let mut controller = Controller::new();
    let effects = controller.act(Action::EnterDraw);
    applied(&mut controller, effects);
    controller.act(Action::SelectTool(Tool::Pen));
    controller
}

fn session_with(controller: &Controller, strokes: u64) -> Session {
    let output = OutputId::new("layer");
    let mut session = Session::new(
        output.clone(),
        LogicalSize::new(f64::from(WIDTH), f64::from(HEIGHT)).unwrap(),
    );
    for i in 0..strokes {
        let y = 20.0 + i as f64 * 15.0;
        let kind = if i % 2 == 0 {
            StrokeKind::Pen
        } else {
            StrokeKind::Highlighter
        };
        let shape = Shape::stroke(kind, vec![point(10.0, y), point(300.0, y + 10.0)]).unwrap();
        session
            .add(Object::new(
                ObjectId::from_raw(i + 1),
                output.clone(),
                controller.style(),
                shape,
            ))
            .unwrap();
    }
    session
}

fn blank() -> Vec<u8> {
    vec![0u8; (WIDTH * HEIGHT * 4) as usize]
}

fn through_layer(layer: &mut InkLayer, controller: &Controller, session: &Session) -> Vec<u8> {
    let mut pixels = blank();
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();
    ink_ui::clear(&mut canvas, true);
    layer.paint(
        &mut canvas,
        controller,
        session.document(),
        &mut Painter::new(),
        Scale::ONE,
    );
    pixels
}

fn direct(controller: &Controller, session: &Session) -> Vec<u8> {
    let mut pixels = blank();
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();
    ink_ui::clear(&mut canvas, true);
    ink_ui::paint_document(
        &mut canvas,
        controller,
        session.document(),
        &mut Painter::new(),
        Scale::ONE,
    );
    pixels
}

/// The cache must not change what the ink looks like, highlighters included,
/// over the capture floor. Compositing a layer is the same source-over as
/// painting directly, so at most rounding may differ.
#[test]
fn the_layer_draws_what_painting_directly_draws() {
    let controller = drawing();
    let session = session_with(&controller, 8);
    let cached = through_layer(&mut InkLayer::new(), &controller, &session);
    let fresh = direct(&controller, &session);
    let worst = cached
        .iter()
        .zip(&fresh)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        worst <= 1,
        "the cached ink differs by up to {worst} per channel"
    );
    assert!(
        fresh.chunks_exact(4).any(|p| p[3] > 1),
        "the scene drew nothing"
    );
}

/// The point of the cache: a stroke in flight changes every frame, and the
/// committed ink under it must not be re-rasterised for it.
#[test]
fn moving_the_pointer_does_not_rebuild_the_layer() {
    let mut controller = drawing();
    let session = session_with(&controller, 8);
    let mut layer = InkLayer::new();
    controller.handle(PlatformEvent::PointerDown {
        at: point(50.0, 50.0),
    });
    for step in 0..20 {
        controller.handle(PlatformEvent::PointerMoved {
            at: point(50.0 + f64::from(step) * 5.0, 60.0),
        });
        through_layer(&mut layer, &controller, &session);
    }
    assert_eq!(layer.rebuilds(), 1);
}

/// The failure a cache is most likely to have: ink that changed and a screen
/// that did not. Every kind of change is a new revision, so each rebuilds.
#[test]
fn every_change_to_the_ink_rebuilds_the_layer() {
    let controller = drawing();
    let mut session = session_with(&controller, 4);
    let mut layer = InkLayer::new();
    through_layer(&mut layer, &controller, &session);

    let shape = Shape::stroke(
        StrokeKind::Pen,
        vec![point(5.0, 200.0), point(200.0, 220.0)],
    )
    .unwrap();
    let output = session.document().output().clone();
    session
        .add(Object::new(
            ObjectId::from_raw(99),
            output,
            controller.style(),
            shape,
        ))
        .unwrap();
    let after_add = through_layer(&mut layer, &controller, &session);
    assert_eq!(layer.rebuilds(), 2, "adding a stroke did not rebuild");
    assert_eq!(after_add, direct(&controller, &session));

    session.undo().unwrap();
    through_layer(&mut layer, &controller, &session);
    assert_eq!(layer.rebuilds(), 3, "undo did not rebuild");

    session.clear().unwrap();
    let cleared = through_layer(&mut layer, &controller, &session);
    assert_eq!(layer.rebuilds(), 4, "clearing did not rebuild");
    assert!(
        cleared.chunks_exact(4).all(|p| p[3] <= 1),
        "cleared ink is still on screen"
    );
}

/// Loading a file replaces the document with one of the same size and perhaps
/// the same object count. A per-document counter would restart and match the
/// old key; the revision is process-wide so it cannot.
#[test]
fn adopting_another_document_rebuilds_the_layer() {
    let controller = drawing();
    let mut session = session_with(&controller, 3);
    let mut layer = InkLayer::new();
    through_layer(&mut layer, &controller, &session);

    let replacement = session_with(&controller, 3).document().clone();
    session.adopt(replacement);
    through_layer(&mut layer, &controller, &session);
    assert_eq!(layer.rebuilds(), 2);
}

/// Parked keeps the document but does not show it, cached or not.
#[test]
fn nothing_is_drawn_while_parked() {
    let mut controller = drawing();
    let session = session_with(&controller, 4);
    let effects = controller.act(Action::TogglePark);
    applied(&mut controller, effects);
    let pixels = through_layer(&mut InkLayer::new(), &controller, &session);
    assert!(pixels.chunks_exact(4).all(|p| p[3] <= 1));
}
