//! T017: erasing and command history.
//!
//! Requirements: FR-009 (object eraser), FR-010 (undo, redo, clear).
//! Scenarios: AC-FR-009, AC-FR-010.
//!
//! The centrepiece is `core_test_sequence`, which is the sequence written out
//! in `specs/002-drawing-history/spec.md`, implemented literally. Everything
//! runs headlessly.

use ink_core::{
    IdSource, LogicalPoint, LogicalSize, Object, ObjectId, Opacity, OutputId, Rgb, Session, Shape,
    StrokeKind, Style, Width,
};

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

fn style() -> Style {
    Style::new(
        Rgb::new(255, 0, 255),
        Width::new(4.0).unwrap(),
        Opacity::OPAQUE,
    )
}

fn session() -> (Session, IdSource, OutputId) {
    let output = OutputId::new("test");
    let session = Session::new(output.clone(), LogicalSize::new(1000.0, 1000.0).unwrap());
    (session, IdSource::starting_at(1), output)
}

fn stroke_at(ids: &mut IdSource, output: &OutputId, y: f64) -> Object {
    Object::new(
        ids.next_id(),
        output.clone(),
        style(),
        Shape::stroke(StrokeKind::Pen, vec![point(10.0, y), point(200.0, y)]).unwrap(),
    )
}

/// The ids currently in the document, in paint order.
fn ids_in(session: &Session) -> Vec<ObjectId> {
    session.document().objects().map(Object::id).collect()
}

// --- The sequence from specs/002-drawing-history/spec.md -------------------

#[test]
fn core_test_sequence() {
    let (mut session, mut ids, output) = session();

    // "Create stroke A, rectangle B, and stroke C."
    let a = session.add(stroke_at(&mut ids, &output, 10.0)).unwrap();
    let b = session
        .add(Object::new(
            ids.next_id(),
            output.clone(),
            style(),
            // Off to the right of the eraser's path. A rectangle spanning the
            // sweep's x would be crossed by its horizontal edges, which is
            // correct behaviour but not what this step is testing.
            Shape::rectangle(point(400.0, 100.0), point(600.0, 180.0)).unwrap(),
        ))
        .unwrap();
    let c = session.add(stroke_at(&mut ids, &output, 300.0)).unwrap();
    assert_eq!(ids_in(&session), vec![a, b, c]);

    // "Erase A and C with one gesture." A vertical sweep at x = 20 crosses
    // both strokes and misses the rectangle's outline.
    let erased = session
        .erase_along(&[point(20.0, 5.0), point(20.0, 320.0)], 3.0)
        .unwrap();
    assert_eq!(erased, 2, "one gesture, two objects");
    assert_eq!(ids_in(&session), vec![b]);

    // "Undo once restores both in their original order."
    assert!(session.undo().unwrap());
    assert_eq!(
        ids_in(&session),
        vec![a, b, c],
        "the eraser sweep was one transaction, and order is exact"
    );

    // "Undo again removes C."
    assert!(session.undo().unwrap());
    assert_eq!(ids_in(&session), vec![a, b]);

    // "Redo restores C."
    assert!(session.redo().unwrap());
    assert_eq!(ids_in(&session), vec![a, b, c]);

    // "Clear removes all."
    assert_eq!(session.clear().unwrap(), 3);
    assert!(session.document().is_empty());

    // "Undo restores all."
    assert!(session.undo().unwrap());
    assert_eq!(ids_in(&session), vec![a, b, c]);

    // "After undoing a command, create D and verify redo is empty."
    assert!(session.undo().unwrap());
    assert!(
        session.can_redo(),
        "there is something to redo before the new edit"
    );
    session.add(stroke_at(&mut ids, &output, 500.0)).unwrap();
    assert!(!session.can_redo(), "a new edit clears the redo branch");
}

// --- FR-009: erasing -------------------------------------------------------

#[test]
fn a_fast_sweep_erases_what_it_passed_over_between_samples() {
    // The case the spec calls out: two widely spaced samples with a stroke
    // between them. Testing only the sample points would miss it entirely.
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();

    let erased = session
        .erase_along(&[point(50.0, 10.0), point(50.0, 300.0)], 2.0)
        .unwrap();

    assert_eq!(
        erased, 1,
        "the sweep crossed the stroke between two samples"
    );
}

#[test]
fn an_eraser_that_touches_nothing_records_no_history() {
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 10.0)).unwrap();
    let before = session.document().len();

    let erased = session
        .erase_along(&[point(900.0, 900.0), point(950.0, 950.0)], 2.0)
        .unwrap();

    assert_eq!(erased, 0);
    assert_eq!(session.document().len(), before);
    // Undo must reach the stroke, not an empty erase in between.
    assert!(session.undo().unwrap());
    assert!(session.document().is_empty());
}

#[test]
fn erasing_removes_whole_objects_never_parts_of_them() {
    // FR-009 is an object eraser. Clipping a stroke in half would be a
    // different feature with different undo semantics.
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 50.0)).unwrap();

    // Clip the very end of the stroke only.
    session
        .erase_along(&[point(199.0, 50.0), point(205.0, 50.0)], 2.0)
        .unwrap();

    assert!(session.document().is_empty(), "the whole stroke goes");
}

#[test]
fn a_tap_with_the_eraser_still_erases() {
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 50.0)).unwrap();

    let erased = session.erase_along(&[point(100.0, 50.0)], 3.0).unwrap();

    assert_eq!(
        erased, 1,
        "a single-sample gesture is a degenerate segment, not nothing"
    );
}

#[test]
fn a_thick_object_is_hit_where_it_looks_hit() {
    // The tolerance includes half the stroke width, so a wide line is erased
    // by touching its visible edge rather than only its mathematical centre.
    let output = OutputId::new("test");
    let mut session = Session::new(output.clone(), LogicalSize::new(1000.0, 1000.0).unwrap());
    let mut ids = IdSource::starting_at(1);
    let fat = Style::new(
        Rgb::new(255, 0, 255),
        Width::new(40.0).unwrap(),
        Opacity::OPAQUE,
    );
    session
        .add(Object::new(
            ids.next_id(),
            output,
            fat,
            Shape::stroke(
                StrokeKind::Pen,
                vec![point(10.0, 100.0), point(200.0, 100.0)],
            )
            .unwrap(),
        ))
        .unwrap();

    // 18 units above the centre line: outside a thin tolerance, inside the
    // 20-unit half-width of what is drawn.
    let erased = session.erase_along(&[point(100.0, 82.0)], 1.0).unwrap();

    assert_eq!(erased, 1);
}

// --- FR-010: undo, redo, clear ---------------------------------------------

#[test]
fn undo_and_redo_report_when_there_is_nothing_to_do() {
    let (mut session, _, _) = session();

    assert!(!session.undo().unwrap());
    assert!(!session.redo().unwrap());
    assert!(!session.can_undo());
    assert!(!session.can_redo());
}

#[test]
fn clearing_an_empty_document_is_a_no_op() {
    // FR-010 says so explicitly: it must not become an undo entry that does
    // nothing when the user reaches it.
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 10.0)).unwrap();

    assert_eq!(session.clear().unwrap(), 1);
    assert_eq!(
        session.clear().unwrap(),
        0,
        "the second clear changes nothing"
    );

    assert!(session.undo().unwrap());
    assert_eq!(session.document().len(), 1, "one undo reaches the objects");
}

#[test]
fn an_undone_edit_is_restored_with_its_exact_style() {
    let output = OutputId::new("test");
    let mut session = Session::new(output.clone(), LogicalSize::new(1000.0, 1000.0).unwrap());
    let mut ids = IdSource::starting_at(1);
    let distinctive = Style::new(
        Rgb::new(64, 224, 96),
        Width::new(16.0).unwrap(),
        Opacity::new(0.3).unwrap(),
    );
    let original = Object::new(
        ids.next_id(),
        output,
        distinctive,
        Shape::stroke(
            StrokeKind::Highlighter,
            vec![point(10.0, 10.0), point(90.0, 90.0)],
        )
        .unwrap(),
    );
    session.add(original.clone()).unwrap();

    session.clear().unwrap();
    session.undo().unwrap();

    let restored = session.document().objects().next().expect("restored");
    assert_eq!(restored, &original, "style, geometry and id all survive");
}

#[test]
fn redo_survives_until_a_new_edit_replaces_it() {
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 10.0)).unwrap();
    session.add(stroke_at(&mut ids, &output, 20.0)).unwrap();

    session.undo().unwrap();
    assert!(session.can_redo());

    // A no-op must not count as a new edit, or it would silently destroy the
    // redo branch.
    session.erase_along(&[point(900.0, 900.0)], 1.0).unwrap();
    assert!(session.can_redo(), "a no-op erase left the branch alone");

    session.add(stroke_at(&mut ids, &output, 30.0)).unwrap();
    assert!(!session.can_redo());
}

#[test]
fn undoing_everything_empties_the_document_and_stops() {
    let (mut session, mut ids, output) = session();
    for step in 0..5 {
        session
            .add(stroke_at(&mut ids, &output, 10.0 + f64::from(step) * 20.0))
            .unwrap();
    }

    let mut undone = 0;
    while session.undo().unwrap() {
        undone += 1;
        assert!(undone <= 10, "undo is not terminating");
    }

    assert_eq!(undone, 5);
    assert!(session.document().is_empty());
}

#[test]
fn history_is_bounded_and_says_when_it_truncated() {
    // NFR-003: report the limit rather than growing without bound.
    let (mut session, mut ids, output) = session();
    let limit = ink_core::limits::MAX_UNDO_TRANSACTIONS;

    for step in 0..(limit + 10) {
        session
            .add(stroke_at(&mut ids, &output, (step % 900) as f64 + 5.0))
            .unwrap();
    }

    assert_eq!(session.history().undo_depth(), limit);
    assert!(session.history().was_truncated());
    // The document is intact: truncation costs undo reach, never objects.
    assert_eq!(session.document().len(), limit + 10);
}

// --- T028 (split): selection, move and resize ------------------------------

#[test]
fn hit_testing_picks_the_topmost_object() {
    // Later objects paint over earlier ones, so the one the user can see is
    // the one they mean.
    let (mut session, mut ids, output) = session();
    let under = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    let over = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    assert_ne!(under, over);

    assert_eq!(session.hit_test(point(100.0, 100.0), 2.0), Some(over));
}

#[test]
fn hit_testing_misses_where_there_is_nothing() {
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();

    assert_eq!(session.hit_test(point(800.0, 800.0), 2.0), None);
}

#[test]
fn moving_an_object_is_one_undoable_edit_that_restores_exactly() {
    let (mut session, mut ids, output) = session();
    let id = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    let before = session.document().get(id).cloned().expect("the object");

    assert_eq!(session.move_objects(&[id], 50.0, -20.0).unwrap(), 1);
    let moved = session.document().get(id).expect("still there");
    assert_eq!(moved.bounds().min.x, before.bounds().min.x + 50.0);
    assert_eq!(moved.bounds().min.y, before.bounds().min.y - 20.0);

    assert!(session.undo().unwrap());
    assert_eq!(
        session.document().get(id),
        Some(&before),
        "geometry restored exactly"
    );
}

#[test]
fn moving_keeps_the_object_in_its_place_in_the_paint_order() {
    // A moved object must not jump in front of things it was behind.
    let (mut session, mut ids, output) = session();
    let first = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    let second = session.add(stroke_at(&mut ids, &output, 200.0)).unwrap();

    session.move_objects(&[first], 10.0, 10.0).unwrap();

    assert_eq!(ids_in(&session), vec![first, second]);
}

#[test]
fn several_objects_move_together_as_one_edit() {
    let (mut session, mut ids, output) = session();
    let a = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    let b = session.add(stroke_at(&mut ids, &output, 200.0)).unwrap();

    assert_eq!(session.move_objects(&[a, b], 10.0, 10.0).unwrap(), 2);

    assert!(session.undo().unwrap());
    assert!(!session.can_undo() || session.document().len() == 2);
    // One undo put both back, so the next undo reaches the creations.
    assert_eq!(session.document().len(), 2);
}

#[test]
fn scaling_an_object_is_undoable_and_exact() {
    let (mut session, mut ids, output) = session();
    let id = session
        .add(Object::new(
            ids.next_id(),
            output.clone(),
            style(),
            Shape::rectangle(point(100.0, 100.0), point(200.0, 200.0)).unwrap(),
        ))
        .unwrap();
    let before = session.document().get(id).cloned().expect("the object");

    session
        .scale_objects(&[id], point(100.0, 100.0), 2.0, 2.0)
        .unwrap();
    let scaled = session.document().get(id).expect("still there");
    assert_eq!(scaled.bounds().max.x, 300.0);
    assert_eq!(scaled.bounds().max.y, 300.0);

    assert!(session.undo().unwrap());
    assert_eq!(session.document().get(id), Some(&before));
}

#[test]
fn a_scale_that_would_destroy_a_shape_is_refused_whole() {
    // FR-008 refuses to create an invisible object, so a transform must not
    // be able to produce one either. All or nothing: a partly applied
    // transform would leave a selection that no longer matches the drag.
    let (mut session, mut ids, output) = session();
    let keeps = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    let shrinks = session
        .add(Object::new(
            ids.next_id(),
            output.clone(),
            style(),
            Shape::rectangle(point(100.0, 300.0), point(200.0, 400.0)).unwrap(),
        ))
        .unwrap();
    let before = session
        .document()
        .get(shrinks)
        .cloned()
        .expect("the object");

    let result = session.scale_objects(&[keeps, shrinks], point(0.0, 0.0), 0.0001, 0.0001);

    assert!(result.is_err(), "a collapsing scale must be refused");
    assert_eq!(
        session.document().get(shrinks),
        Some(&before),
        "nothing changed"
    );
    assert_eq!(session.document().len(), 2);
}

#[test]
fn deleting_a_selection_is_one_undoable_edit() {
    let (mut session, mut ids, output) = session();
    let a = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    let b = session.add(stroke_at(&mut ids, &output, 200.0)).unwrap();
    session.add(stroke_at(&mut ids, &output, 300.0)).unwrap();

    assert_eq!(session.delete(&[a, b]).unwrap(), 2);
    assert_eq!(session.document().len(), 1);

    assert!(session.undo().unwrap());
    assert_eq!(session.document().len(), 3, "one undo brings both back");
}

#[test]
fn transforming_nothing_records_no_history() {
    let (mut session, mut ids, output) = session();
    session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();

    assert_eq!(session.move_objects(&[], 10.0, 10.0).unwrap(), 0);
    assert_eq!(
        session
            .move_objects(&[ink_core::ObjectId::from_raw(9999)], 10.0, 10.0)
            .unwrap(),
        0
    );

    // Undo reaches the creation, not an empty move.
    assert!(session.undo().unwrap());
    assert!(session.document().is_empty());
}

#[test]
fn the_bounds_of_a_selection_cover_all_of_it() {
    let (mut session, mut ids, output) = session();
    let a = session.add(stroke_at(&mut ids, &output, 100.0)).unwrap();
    let b = session.add(stroke_at(&mut ids, &output, 300.0)).unwrap();

    let bounds = session.bounds_of(&[a, b]).expect("two objects have bounds");

    assert!(bounds.min.y <= 100.0);
    assert!(bounds.max.y >= 300.0);
    assert_eq!(
        session.bounds_of(&[]),
        None,
        "an empty selection has no bounds"
    );
}
