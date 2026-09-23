//! T011/T014: rendering conventions.
//!
//! Requirements: FR-001 (transparency), FR-007 (opacity), FR-012 (scale at the
//! boundary), NFR-004 (headless).
//!
//! These assert pixels, with no GPU and no window. The alpha convention they
//! pin down is the one T014's real renderer must keep.

use ink_core::{
    Document, IdSource, LogicalPoint, LogicalSize, Object, Opacity, OutputId, Rgb, Shape,
    StrokeKind, Style, Width,
};
use ink_render::{Canvas, Scale, paint};

const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).unwrap()
}

fn document_with(shape: Shape, style: Style) -> Document {
    let output = OutputId::new("test");
    let mut document = Document::new(output.clone(), LogicalSize::new(100.0, 100.0).unwrap());
    let mut ids = IdSource::starting_at(1);
    document
        .add(Object::new(ids.next_id(), output, style, shape))
        .unwrap();
    document
}

fn opaque_magenta() -> Style {
    Style::new(
        Rgb::new(255, 0, 255),
        Width::new(4.0).unwrap(),
        Opacity::OPAQUE,
    )
}

#[test]
fn a_canvas_must_match_its_buffer_exactly() {
    let mut pixels = vec![0u8; 10 * 10 * 4];
    assert!(Canvas::new(&mut pixels, 10, 10).is_some());
    assert!(
        Canvas::new(&mut pixels, 10, 11).is_none(),
        "a short buffer must be refused"
    );
    assert!(
        Canvas::new(&mut pixels, 9, 10).is_none(),
        "an oversized buffer must be refused too"
    );
}

#[test]
fn an_empty_document_leaves_every_pixel_transparent() {
    // FR-001: the background of the overlay is genuinely transparent. A black
    // pixel here would be the fullscreen-unredirect bug from E002 reappearing
    // in our own code.
    let document = Document::new(
        OutputId::new("test"),
        LogicalSize::new(100.0, 100.0).unwrap(),
    );
    let mut pixels = vec![0xFFu8; 20 * 20 * 4];
    let mut canvas = Canvas::new(&mut pixels, 20, 20).unwrap();
    canvas.clear();

    paint(&document, &mut canvas, Scale::ONE);

    for y in 0..20 {
        for x in 0..20 {
            assert_eq!(
                canvas.pixel(x, y),
                Some(TRANSPARENT),
                "pixel ({x}, {y}) is not transparent"
            );
        }
    }
}

#[test]
fn a_dot_paints_where_it_was_put_and_nowhere_else() {
    let document = document_with(
        Shape::stroke(StrokeKind::Pen, vec![point(10.0, 10.0)]).unwrap(),
        opaque_magenta(),
    );
    let mut pixels = vec![0u8; 40 * 40 * 4];
    let mut canvas = Canvas::new(&mut pixels, 40, 40).unwrap();

    paint(&document, &mut canvas, Scale::ONE);

    // Memory order is B, G, R, A. Opaque magenta is full blue, no green, full
    // red, full alpha.
    assert_eq!(canvas.pixel(10, 10), Some([255, 0, 255, 255]));
    assert_eq!(
        canvas.pixel(35, 35),
        Some(TRANSPARENT),
        "far from the dot stays clear"
    );
}

#[test]
fn a_transparent_pixel_is_four_zero_bytes_not_black() {
    // Premultiplied alpha: the reason a transparent area cannot show a dark
    // fringe is that its colour channels are zero as well as its alpha.
    let document = document_with(
        Shape::stroke(StrokeKind::Pen, vec![point(5.0, 5.0)]).unwrap(),
        opaque_magenta(),
    );
    let mut pixels = vec![0u8; 40 * 40 * 4];
    let mut canvas = Canvas::new(&mut pixels, 40, 40).unwrap();

    paint(&document, &mut canvas, Scale::ONE);

    let far = canvas.pixel(30, 30).unwrap();
    assert_eq!(far, TRANSPARENT);
    assert_eq!(far[3], 0, "alpha is zero");
    assert_eq!(&far[0..3], &[0, 0, 0], "and so are the colour channels");
}

#[test]
fn opacity_is_premultiplied_into_the_colour() {
    // FR-007: a half-opacity highlighter must arrive at the compositor already
    // multiplied, or it will be drawn twice as bright.
    let style = Style::new(
        Rgb::new(255, 255, 255),
        Width::new(4.0).unwrap(),
        Opacity::new(0.5).unwrap(),
    );
    let document = document_with(
        Shape::stroke(StrokeKind::Highlighter, vec![point(10.0, 10.0)]).unwrap(),
        style,
    );
    let mut pixels = vec![0u8; 40 * 40 * 4];
    let mut canvas = Canvas::new(&mut pixels, 40, 40).unwrap();

    paint(&document, &mut canvas, Scale::ONE);

    let [b, g, r, a] = canvas.pixel(10, 10).unwrap();
    assert_eq!(a, 128, "half opacity");
    assert_eq!(
        [b, g, r],
        [128, 128, 128],
        "channels are scaled by alpha, not left at 255"
    );
}

#[test]
fn scale_is_applied_at_the_boundary_and_nowhere_else() {
    // FR-012: the document holds logical units. At scale 2 the same document
    // paints at twice the pixel coordinates.
    let shape = Shape::stroke(StrokeKind::Pen, vec![point(10.0, 10.0)]).unwrap();
    let document = document_with(shape, opaque_magenta());

    let mut one = vec![0u8; 60 * 60 * 4];
    let mut canvas_one = Canvas::new(&mut one, 60, 60).unwrap();
    paint(&document, &mut canvas_one, Scale::ONE);
    assert_eq!(canvas_one.pixel(10, 10), Some([255, 0, 255, 255]));
    assert_eq!(canvas_one.pixel(20, 20), Some(TRANSPARENT));

    let mut two = vec![0u8; 60 * 60 * 4];
    let mut canvas_two = Canvas::new(&mut two, 60, 60).unwrap();
    paint(&document, &mut canvas_two, Scale::new(2.0).unwrap());
    assert_eq!(canvas_two.pixel(20, 20), Some([255, 0, 255, 255]));
}

#[test]
fn a_scale_must_be_finite_and_positive() {
    assert!(Scale::new(1.5).is_some());
    assert!(Scale::new(0.0).is_none());
    assert!(Scale::new(-1.0).is_none());
    assert!(Scale::new(f64::NAN).is_none());
}

#[test]
fn painting_off_canvas_does_not_panic_or_wrap() {
    // Objects are never clamped to the output, so the renderer is where
    // out-of-bounds geometry has to be survived.
    let document = document_with(
        Shape::stroke(
            StrokeKind::Pen,
            vec![point(-500.0, -500.0), point(900.0, 900.0)],
        )
        .unwrap(),
        opaque_magenta(),
    );
    let mut pixels = vec![0u8; 20 * 20 * 4];
    let mut canvas = Canvas::new(&mut pixels, 20, 20).unwrap();

    paint(&document, &mut canvas, Scale::ONE);

    // The diagonal crosses the canvas, so something was drawn, and no pixel
    // outside it was touched because there is nowhere outside it to touch.
    assert_eq!(canvas.pixel(10, 10), Some([255, 0, 255, 255]));
}

#[test]
fn later_objects_paint_over_earlier_ones() {
    let output = OutputId::new("test");
    let mut document = Document::new(output.clone(), LogicalSize::new(100.0, 100.0).unwrap());
    let mut ids = IdSource::starting_at(1);
    let at = Shape::stroke(StrokeKind::Pen, vec![point(10.0, 10.0)]).unwrap();

    document
        .add(Object::new(
            ids.next_id(),
            output.clone(),
            opaque_magenta(),
            at.clone(),
        ))
        .unwrap();
    let green = Style::new(
        Rgb::new(0, 255, 0),
        Width::new(4.0).unwrap(),
        Opacity::OPAQUE,
    );
    document
        .add(Object::new(ids.next_id(), output, green, at))
        .unwrap();

    let mut pixels = vec![0u8; 40 * 40 * 4];
    let mut canvas = Canvas::new(&mut pixels, 40, 40).unwrap();
    paint(&document, &mut canvas, Scale::ONE);

    assert_eq!(
        canvas.pixel(10, 10),
        Some([0, 255, 0, 255]),
        "the later object wins"
    );
}

// --- FR-007: highlighter opacity applies to the whole stroke ---------------

fn highlighter_style() -> Style {
    Style::new(
        Rgb::new(255, 255, 0),
        Width::new(12.0).unwrap(),
        Opacity::new(0.5).unwrap(),
    )
}

#[test]
fn a_highlighter_stroke_has_one_alpha_along_its_whole_length() {
    // The samples of a stroke overlap heavily, so blending them one at a time
    // makes the stroke darker than the opacity the user chose. FR-007 requires
    // the opacity to apply to the completed stroke as a whole.
    let document = document_with(
        Shape::stroke(
            StrokeKind::Highlighter,
            vec![point(10.0, 30.0), point(50.0, 30.0)],
        )
        .unwrap(),
        highlighter_style(),
    );
    let mut pixels = vec![0u8; 80 * 60 * 4];
    let mut canvas = Canvas::new(&mut pixels, 80, 60).unwrap();

    paint(&document, &mut canvas, Scale::ONE);

    // Every painted pixel along the stroke must carry exactly the requested
    // alpha, not an accumulation of it.
    for x in 15..45 {
        let pixel = canvas.pixel(x, 30).expect("inside the canvas");
        assert_eq!(
            pixel[3], 128,
            "at x={x} the alpha is {} rather than 128; the stroke is compositing with itself",
            pixel[3]
        );
    }
}

#[test]
fn a_highlighter_stroke_crossing_itself_does_not_darken_at_the_crossing() {
    // A stroke drawn back over its own path is still one stroke.
    let document = document_with(
        Shape::stroke(
            StrokeKind::Highlighter,
            vec![
                point(30.0, 10.0),
                point(30.0, 50.0),
                point(10.0, 30.0),
                point(50.0, 30.0),
            ],
        )
        .unwrap(),
        highlighter_style(),
    );
    let mut pixels = vec![0u8; 80 * 60 * 4];
    let mut canvas = Canvas::new(&mut pixels, 80, 60).unwrap();

    paint(&document, &mut canvas, Scale::ONE);

    let crossing = canvas.pixel(30, 30).expect("the crossing point");
    let elsewhere = canvas.pixel(30, 20).expect("a point on one arm only");
    assert_eq!(
        crossing, elsewhere,
        "the crossing is a different colour from the rest of the same stroke"
    );
}

#[test]
fn two_separate_highlighter_strokes_do_accumulate() {
    // The distinction FR-007 draws: within one stroke, no build-up; between
    // strokes, build-up is what the user asked for by drawing twice.
    let output = OutputId::new("test");
    let mut document = Document::new(output.clone(), LogicalSize::new(100.0, 100.0).unwrap());
    let mut ids = IdSource::starting_at(1);
    let shape = Shape::stroke(StrokeKind::Highlighter, vec![point(20.0, 30.0)]).unwrap();

    document
        .add(Object::new(
            ids.next_id(),
            output.clone(),
            highlighter_style(),
            shape.clone(),
        ))
        .unwrap();
    document
        .add(Object::new(
            ids.next_id(),
            output,
            highlighter_style(),
            shape,
        ))
        .unwrap();

    let mut pixels = vec![0u8; 80 * 60 * 4];
    let mut canvas = Canvas::new(&mut pixels, 80, 60).unwrap();
    paint(&document, &mut canvas, Scale::ONE);

    let alpha = canvas.pixel(20, 30).unwrap()[3];
    assert!(
        alpha > 128,
        "two passes should be denser than one, got {alpha}"
    );
}
