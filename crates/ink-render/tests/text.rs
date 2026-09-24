//! Text rendering.
//!
//! Requirement: FR-023 (text), FR-012 (scale at the boundary).
//!
//! These need a font on the machine. Where none is found they report that and
//! pass, rather than failing for a reason that is not about this code: a
//! headless build machine without fonts is not a bug in the renderer.

use ink_core::{
    Document, IdSource, LogicalPoint, LogicalSize, Object, Opacity, OutputId, Rgb, Shape, Style,
    Width,
};
use ink_render::text::TextFont;
use ink_render::{Canvas, Painter, Scale};

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).unwrap()
}

fn style() -> Style {
    Style::new(
        Rgb::new(255, 255, 255),
        Width::new(2.0).unwrap(),
        Opacity::OPAQUE,
    )
}

fn document_with(shape: Shape) -> Document {
    let output = OutputId::new("test");
    let mut document = Document::new(output.clone(), LogicalSize::new(400.0, 200.0).unwrap());
    let mut ids = IdSource::starting_at(1);
    document
        .add(Object::new(ids.next_id(), output, style(), shape))
        .unwrap();
    document
}

/// Counts pixels with any coverage.
fn painted(canvas: &Canvas) -> usize {
    (0..canvas.height())
        .flat_map(|y| (0..canvas.width()).map(move |x| (x, y)))
        .filter(|(x, y)| canvas.pixel(*x, *y).unwrap()[3] > 0)
        .count()
}

fn painter() -> Option<Painter> {
    match TextFont::discover() {
        Ok(font) => Some(Painter::new().with_font(font)),
        Err(reason) => {
            eprintln!("skipping: {reason}");
            None
        }
    }
}

#[test]
fn text_is_drawn() {
    let Some(mut painter) = painter() else { return };
    let document = document_with(Shape::text(point(10.0, 10.0), "Hello", 24.0).unwrap());
    let mut pixels = vec![0u8; 400 * 200 * 4];
    let mut canvas = Canvas::new(&mut pixels, 400, 200).unwrap();

    painter.paint(&document, &mut canvas, Scale::ONE);

    assert!(painted(&canvas) > 50, "almost nothing was drawn");
}

#[test]
fn longer_text_covers_more() {
    let Some(mut painter) = painter() else { return };
    let mut short_pixels = vec![0u8; 400 * 200 * 4];
    let mut short = Canvas::new(&mut short_pixels, 400, 200).unwrap();
    painter.paint(
        &document_with(Shape::text(point(10.0, 10.0), "I", 24.0).unwrap()),
        &mut short,
        Scale::ONE,
    );

    let mut long_pixels = vec![0u8; 400 * 200 * 4];
    let mut long = Canvas::new(&mut long_pixels, 400, 200).unwrap();
    painter.paint(
        &document_with(Shape::text(point(10.0, 10.0), "IIIIIIII", 24.0).unwrap()),
        &mut long,
        Scale::ONE,
    );

    assert!(painted(&long) > painted(&short));
}

#[test]
fn a_bigger_size_covers_more() {
    let Some(mut painter) = painter() else { return };
    let mut small_pixels = vec![0u8; 400 * 200 * 4];
    let mut small = Canvas::new(&mut small_pixels, 400, 200).unwrap();
    painter.paint(
        &document_with(Shape::text(point(10.0, 10.0), "Ag", 12.0).unwrap()),
        &mut small,
        Scale::ONE,
    );

    let mut big_pixels = vec![0u8; 400 * 200 * 4];
    let mut big = Canvas::new(&mut big_pixels, 400, 200).unwrap();
    painter.paint(
        &document_with(Shape::text(point(10.0, 10.0), "Ag", 36.0).unwrap()),
        &mut big,
        Scale::ONE,
    );

    assert!(
        painted(&big) > painted(&small) * 2,
        "size had little effect"
    );
}

#[test]
fn a_second_line_is_drawn_below_the_first() {
    let Some(mut painter) = painter() else { return };
    let document = document_with(Shape::text(point(10.0, 10.0), "A\nA", 24.0).unwrap());
    let mut pixels = vec![0u8; 400 * 200 * 4];
    let mut canvas = Canvas::new(&mut pixels, 400, 200).unwrap();

    painter.paint(&document, &mut canvas, Scale::ONE);

    let rows: Vec<u32> = (0..200)
        .filter(|y| (0..400).any(|x| canvas.pixel(x, *y).unwrap()[3] > 0))
        .collect();
    let first = *rows.first().expect("something was drawn");
    let last = *rows.last().unwrap();
    assert!(
        last - first > 24,
        "the two lines are not separated: {first} to {last}"
    );
}

#[test]
fn text_without_a_font_draws_nothing_rather_than_boxes() {
    // A painter with no face is what a machine with no fonts gets. Drawing
    // placeholder boxes would look like a rendering bug; drawing nothing, with
    // the application reporting why, is honest.
    let mut painter = Painter::new();
    assert!(!painter.has_font());
    let document = document_with(Shape::text(point(10.0, 10.0), "Hello", 24.0).unwrap());
    let mut pixels = vec![0u8; 400 * 200 * 4];
    let mut canvas = Canvas::new(&mut pixels, 400, 200).unwrap();

    painter.paint(&document, &mut canvas, Scale::ONE);

    assert_eq!(painted(&canvas), 0);
}

#[test]
fn scale_applies_to_text_like_everything_else() {
    // FR-012: the document is in logical units and the scale is applied at the
    // boundary, so the same text at scale 2 covers about four times the area.
    let Some(mut painter) = painter() else { return };
    let document = document_with(Shape::text(point(5.0, 5.0), "Ag", 20.0).unwrap());

    let mut one_pixels = vec![0u8; 400 * 200 * 4];
    let mut one = Canvas::new(&mut one_pixels, 400, 200).unwrap();
    painter.paint(&document, &mut one, Scale::ONE);

    let mut two_pixels = vec![0u8; 400 * 200 * 4];
    let mut two = Canvas::new(&mut two_pixels, 400, 200).unwrap();
    painter.paint(&document, &mut two, Scale::new(2.0).unwrap());

    let ratio = painted(&two) as f64 / painted(&one).max(1) as f64;
    assert!((2.5..=6.0).contains(&ratio), "scale ratio was {ratio}");
}
