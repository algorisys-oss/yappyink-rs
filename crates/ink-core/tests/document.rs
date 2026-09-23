//! T009: the platform-free document model.
//!
//! Requirements: FR-011 (output-local documents), FR-012 (coordinate
//! correctness), NFR-004 (headless deterministic core).
//! Scenario: AC-FR-011, AC-FR-012.
//!
//! Everything here runs with no display, no GPU, and no clock. Object ids come
//! from an injected source.

use ink_core::{
    Document, DocumentError, IdSource, LogicalPoint, LogicalSize, Object, ObjectId, Opacity,
    OutputId, Rgb, Shape, StrokeKind, Style, Width, limits,
};

fn style() -> Style {
    Style::new(
        Rgb::new(255, 0, 255),
        Width::new(4.0).unwrap(),
        Opacity::OPAQUE,
    )
}

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

fn output_size() -> LogicalSize {
    LogicalSize::new(1920.0, 1080.0).expect("valid test output size")
}

fn pen_stroke(ids: &mut IdSource, output: &OutputId) -> Object {
    Object::new(
        ids.next_id(),
        output.clone(),
        style(),
        Shape::stroke(StrokeKind::Pen, vec![point(10.0, 10.0), point(20.0, 40.0)]).unwrap(),
    )
}

// --- Objects ---------------------------------------------------------------

#[test]
fn every_shape_can_be_built_from_valid_input() {
    let a = point(10.0, 10.0);
    let b = point(100.0, 80.0);

    assert!(Shape::stroke(StrokeKind::Pen, vec![a, b]).is_ok());
    assert!(
        Shape::stroke(StrokeKind::Highlighter, vec![a]).is_ok(),
        "a dot is a valid gesture"
    );
    assert!(Shape::line(a, b).is_ok());
    assert!(Shape::arrow(a, b).is_ok());
    assert!(Shape::rectangle(a, b).is_ok());
    assert!(Shape::ellipse(a, b).is_ok());
}

#[test]
fn a_stroke_needs_at_least_one_point() {
    assert!(matches!(
        Shape::stroke(StrokeKind::Pen, Vec::new()),
        Err(DocumentError::EmptyStroke)
    ));
}

#[test]
fn a_stroke_is_bounded_by_the_sample_limit() {
    let too_many = vec![point(1.0, 1.0); limits::MAX_STROKE_POINTS + 1];

    assert!(matches!(
        Shape::stroke(StrokeKind::Pen, too_many),
        Err(DocumentError::StrokeTooLong { .. })
    ));
}

#[test]
fn a_degenerate_two_point_shape_is_rejected_rather_than_stored_invisibly() {
    let same = point(50.0, 50.0);

    // FR-008: a completed drag that goes nowhere must not create an invisible
    // object the user cannot see, select, or erase.
    assert!(matches!(
        Shape::line(same, same),
        Err(DocumentError::DegenerateShape { .. })
    ));
    assert!(matches!(
        Shape::rectangle(same, same),
        Err(DocumentError::DegenerateShape { .. })
    ));
    assert!(matches!(
        Shape::ellipse(same, same),
        Err(DocumentError::DegenerateShape { .. })
    ));
}

#[test]
fn shape_bounds_are_normalised_whichever_way_the_drag_went() {
    let top_left = point(10.0, 20.0);
    let bottom_right = point(110.0, 220.0);

    let forwards = Shape::rectangle(top_left, bottom_right).unwrap();
    let backwards = Shape::rectangle(bottom_right, top_left).unwrap();

    assert_eq!(forwards.bounds(), backwards.bounds());
    let bounds = forwards.bounds();
    assert_eq!((bounds.min.x, bounds.min.y), (10.0, 20.0));
    assert_eq!((bounds.max.x, bounds.max.y), (110.0, 220.0));
}

// --- Style validation ------------------------------------------------------

#[test]
fn width_must_be_finite_and_positive() {
    assert!(Width::new(4.0).is_some());
    assert!(Width::new(0.0).is_none());
    assert!(Width::new(-1.0).is_none());
    assert!(Width::new(f64::NAN).is_none());
    assert!(Width::new(f64::INFINITY).is_none());
}

#[test]
fn width_is_in_logical_units_not_pixels() {
    // FR-012: width 4.0 means four logical units. Scaling happens at the
    // backend boundary, so nothing here may bake in a device pixel ratio.
    assert_eq!(Width::new(4.0).unwrap().get(), 4.0);
}

#[test]
fn opacity_must_lie_between_zero_and_one() {
    assert!(Opacity::new(0.0).is_some());
    assert!(Opacity::new(0.35).is_some());
    assert!(Opacity::new(1.0).is_some());
    assert!(Opacity::new(1.0001).is_none());
    assert!(Opacity::new(-0.0001).is_none());
    assert!(Opacity::new(f64::NAN).is_none());
}

// --- Documents are per output ----------------------------------------------

#[test]
fn an_object_must_belong_to_the_document_it_is_added_to() {
    let mut ids = IdSource::starting_at(1);
    let here = OutputId::new("eDP-1");
    let elsewhere = OutputId::new("HDMI-1");
    let mut document = Document::new(here, output_size());

    let foreign = pen_stroke(&mut ids, &elsewhere);

    assert!(matches!(
        document.add(foreign),
        Err(DocumentError::OutputMismatch { .. })
    ));
    assert!(document.is_empty());
}

#[test]
fn documents_for_different_outputs_are_independent() {
    let mut ids = IdSource::starting_at(1);
    let laptop = OutputId::new("eDP-1");
    let monitor = OutputId::new("HDMI-1");
    let mut on_laptop = Document::new(laptop.clone(), output_size());
    let mut on_monitor = Document::new(monitor.clone(), output_size());

    on_laptop.add(pen_stroke(&mut ids, &laptop)).unwrap();
    on_laptop.add(pen_stroke(&mut ids, &laptop)).unwrap();
    on_monitor.add(pen_stroke(&mut ids, &monitor)).unwrap();

    assert_eq!(on_laptop.len(), 2);
    assert_eq!(on_monitor.len(), 1);

    // FR-011: switching outputs must preserve each document, so clearing one
    // cannot touch the other.
    on_laptop.clear();
    assert!(on_laptop.is_empty());
    assert_eq!(
        on_monitor.len(),
        1,
        "clearing one output must not affect another"
    );
}

#[test]
fn objects_keep_the_order_they_were_added_in() {
    let mut ids = IdSource::starting_at(1);
    let output = OutputId::new("eDP-1");
    let mut document = Document::new(output.clone(), output_size());

    let first = document.add(pen_stroke(&mut ids, &output)).unwrap();
    let second = document.add(pen_stroke(&mut ids, &output)).unwrap();

    // Paint order is document order: a later stroke draws over an earlier one.
    let ids_in_order: Vec<ObjectId> = document.objects().map(Object::id).collect();
    assert_eq!(ids_in_order, vec![first, second]);
}

#[test]
fn removing_objects_takes_only_the_ones_named() {
    let mut ids = IdSource::starting_at(1);
    let output = OutputId::new("eDP-1");
    let mut document = Document::new(output.clone(), output_size());

    let keep = document.add(pen_stroke(&mut ids, &output)).unwrap();
    let remove = document.add(pen_stroke(&mut ids, &output)).unwrap();

    let removed = document.remove(&[remove]);

    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].id(), remove);
    assert_eq!(document.len(), 1);
    assert_eq!(document.objects().next().unwrap().id(), keep);
}

#[test]
fn removing_an_unknown_id_is_not_an_error_and_changes_nothing() {
    let mut ids = IdSource::starting_at(1);
    let output = OutputId::new("eDP-1");
    let mut document = Document::new(output.clone(), output_size());
    document.add(pen_stroke(&mut ids, &output)).unwrap();

    let removed = document.remove(&[ObjectId::from_raw(9999)]);

    assert!(
        removed.is_empty(),
        "a no-op removal must report that it removed nothing"
    );
    assert_eq!(document.len(), 1);
}

#[test]
fn a_document_is_bounded_by_the_object_limit() {
    let mut ids = IdSource::starting_at(1);
    let output = OutputId::new("eDP-1");
    let mut document = Document::new(output.clone(), output_size());

    for _ in 0..limits::MAX_OBJECTS_PER_OUTPUT {
        document.add(pen_stroke(&mut ids, &output)).unwrap();
    }

    // NFR-003: report the limit rather than growing without bound.
    assert!(matches!(
        document.add(pen_stroke(&mut ids, &output)),
        Err(DocumentError::ObjectLimitReached { .. })
    ));
    assert_eq!(document.len(), limits::MAX_OBJECTS_PER_OUTPUT);
}

// --- Coordinates -----------------------------------------------------------

#[test]
fn objects_outside_the_output_are_kept_not_clamped() {
    let mut ids = IdSource::starting_at(1);
    let output = OutputId::new("eDP-1");
    let mut document = Document::new(output.clone(), output_size());

    // A stroke can legitimately run past the edge: the pointer was there. What
    // is visible is a rendering question, not a document one.
    let object = Object::new(
        ids.next_id(),
        output.clone(),
        style(),
        Shape::stroke(
            StrokeKind::Pen,
            vec![point(-50.0, 10.0), point(5000.0, 10.0)],
        )
        .unwrap(),
    );

    assert!(document.add(object).is_ok());
    assert_eq!(document.len(), 1);
}

#[test]
fn the_document_records_the_output_size_it_was_bound_to() {
    // FR-011: the size is stored so a document can be remapped later when the
    // output it was drawn on is missing or has changed.
    let document = Document::new(OutputId::new("eDP-1"), output_size());

    assert_eq!(document.output_size(), output_size());
    assert_eq!(document.output().as_str(), "eDP-1");
}
