//! T020 and T021: saving and loading.
//!
//! Requirements: FR-015 (explicit local files), FR-016 (privacy defaults),
//! FR-017 (safe file handling), NFR-003 (bounds).
//! Scenarios: AC-FR-015, AC-FR-017.
//!
//! The interesting tests are the failures. A save that works is table stakes;
//! a save that cannot destroy the last good file, and a load that cannot
//! damage the open document, are the requirements.

use std::path::{Path, PathBuf};

use ink_core::{
    Document, IdSource, LogicalPoint, LogicalSize, Object, Opacity, OutputId, Rgb, Shape,
    StrokeKind, Style, Width,
};
use ink_storage::{SCHEMA_VERSION, StorageError, from_bytes, load, save};

/// A directory that removes itself. Enough for these tests, and it keeps the
/// crate free of a temporary-file dependency.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let unique = format!(
            "yappyink-test-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("a temporary directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

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

/// A document with one of every shape, so a round trip covers the whole format.
fn sample() -> Document {
    let output = OutputId::new("eDP-1");
    let mut document = Document::new(output.clone(), LogicalSize::new(1920.0, 1080.0).unwrap());
    let mut ids = IdSource::starting_at(1);
    let shapes = [
        Shape::stroke(StrokeKind::Pen, vec![point(10.0, 10.0), point(60.0, 40.0)]).unwrap(),
        Shape::stroke(StrokeKind::Highlighter, vec![point(20.0, 20.0)]).unwrap(),
        Shape::line(point(0.0, 0.0), point(100.0, 100.0)).unwrap(),
        Shape::arrow(point(10.0, 90.0), point(90.0, 10.0)).unwrap(),
        Shape::rectangle(point(200.0, 200.0), point(300.0, 260.0)).unwrap(),
        Shape::ellipse(point(400.0, 200.0), point(520.0, 280.0)).unwrap(),
    ];
    for shape in shapes {
        document
            .add(Object::new(ids.next_id(), output.clone(), style(), shape))
            .unwrap();
    }
    document
}

// --- The round trip --------------------------------------------------------

#[test]
fn every_shape_survives_a_save_and_load_unchanged() {
    let dir = TempDir::new("roundtrip");
    let path = dir.join("session.json");
    let original = sample();

    save(&original, &path).expect("the save");
    let loaded = load(&path).expect("the load");

    assert_eq!(loaded.document.len(), original.len());
    for (before, after) in original.objects().zip(loaded.document.objects()) {
        assert_eq!(before, after, "an object changed crossing the file");
    }
    assert_eq!(loaded.saved_output.as_str(), "eDP-1");
    assert_eq!(loaded.saved_size, original.output_size());
}

#[test]
fn the_next_id_clears_everything_in_the_file() {
    // Without this, ids issued after a load collide with loaded ones and the
    // eraser takes the wrong object.
    let dir = TempDir::new("ids");
    let path = dir.join("session.json");
    let original = sample();
    let highest = original.objects().map(|o| o.id().get()).max().unwrap();

    save(&original, &path).unwrap();
    let loaded = load(&path).unwrap();

    assert!(loaded.next_object_id > highest);
}

#[test]
fn saving_creates_the_directory_it_needs() {
    let dir = TempDir::new("mkdir");
    let path = dir.join("nested/deeper/session.json");

    save(&sample(), &path).expect("the save");

    assert!(path.exists());
}

#[test]
fn a_saved_file_contains_no_pixels_and_is_readable_text() {
    // FR-014 and ADR-003: the live drawing path never reads the screen, and a
    // saved document is vector objects. A user should be able to look at their
    // own file and see that.
    let dir = TempDir::new("plain");
    let path = dir.join("session.json");

    save(&sample(), &path).unwrap();
    let text = std::fs::read_to_string(&path).expect("it is text");

    assert!(text.contains("\"schema_version\""));
    assert!(text.contains("\"stroke\""));
    assert!(text.contains("\"eDP-1\""));
}

#[test]
fn saving_twice_replaces_rather_than_appends() {
    let dir = TempDir::new("replace");
    let path = dir.join("session.json");

    save(&sample(), &path).unwrap();
    let first = std::fs::metadata(&path).unwrap().len();
    save(
        &Document::new(
            OutputId::new("eDP-1"),
            LogicalSize::new(800.0, 600.0).unwrap(),
        ),
        &path,
    )
    .unwrap();
    let second = std::fs::metadata(&path).unwrap().len();

    assert!(second < first, "the second save did not replace the first");
    assert_eq!(load(&path).unwrap().document.len(), 0);
}

#[test]
fn a_save_leaves_no_temporary_file_behind() {
    let dir = TempDir::new("debris");
    let path = dir.join("session.json");

    save(&sample(), &path).unwrap();

    let leftovers: Vec<_> = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "left behind {leftovers:?}");
}

// --- FR-017: a bad file never damages what is open -------------------------

fn refuse(json: &str) -> StorageError {
    from_bytes(json.as_bytes(), Path::new("test.json"))
        .err()
        .unwrap_or_else(|| panic!("this should have been refused: {json}"))
}

#[test]
fn a_newer_schema_is_refused_with_advice_rather_than_guessed_at() {
    let error = refuse(&format!(
        r#"{{"schema_version":{},"app":"x","output":"a","output_width":100.0,
            "output_height":100.0,"objects":[]}}"#,
        SCHEMA_VERSION + 1
    ));

    assert_eq!(error.class(), "unsupported_version");
    assert!(
        error.to_string().contains("Upgrade"),
        "the message should say what to do: {error}"
    );
}

#[test]
fn an_older_schema_is_still_readable() {
    // Refusing to read our own past output would be a bug, not caution.
    let document = from_bytes(
        br#"{"schema_version":1,"app":"x","output":"a","output_width":100.0,
             "output_height":100.0,"objects":[]}"#,
        Path::new("test.json"),
    );

    assert!(document.is_ok(), "{:?}", document.err());
}

#[test]
fn malformed_json_is_refused() {
    assert_eq!(refuse("not json at all").class(), "malformed");
    assert_eq!(refuse("{").class(), "malformed");
    assert_eq!(refuse("{}").class(), "malformed");
}

#[test]
fn an_unknown_shape_is_refused_rather_than_skipped() {
    // A file with something this build does not understand must not load with
    // pieces quietly missing.
    let error = refuse(
        r#"{"schema_version":1,"app":"x","output":"a","output_width":100.0,
            "output_height":100.0,"objects":[
              {"id":1,"color":[1,2,3],"width":4.0,"opacity":1.0,
               "shape":{"hexagon":{"a":[0.0,0.0],"b":[1.0,1.0]}}}]}"#,
    );

    assert_eq!(error.class(), "malformed");
}

#[test]
fn nonfinite_coordinates_are_refused() {
    // JSON has no NaN or infinity literal, so the realistic hostile input is a
    // huge exponent. It is refused as malformed rather than invalid, because
    // the JSON parser rejects an out-of-range number before our own validation
    // sees it. Which layer catches it does not matter; that it is caught does.
    let error = refuse(
        r#"{"schema_version":1,"app":"x","output":"a","output_width":100.0,
            "output_height":100.0,"objects":[
              {"id":1,"color":[1,2,3],"width":4.0,"opacity":1.0,
               "shape":{"line":{"from":[1e400,0.0],"to":[10.0,10.0]}}}]}"#,
    );

    assert_eq!(error.class(), "malformed");

    // The same applies to a width, so no path reaches a renderer with a
    // non-finite number in it.
    let error = refuse(
        r#"{"schema_version":1,"app":"x","output":"a","output_width":1e400,
            "output_height":100.0,"objects":[]}"#,
    );
    assert_eq!(error.class(), "malformed");
}

#[test]
fn an_impossible_style_is_refused() {
    for (what, width, opacity) in [
        ("a zero width", "0.0", "1.0"),
        ("a negative width", "-4.0", "1.0"),
        ("an opacity above one", "4.0", "1.5"),
        ("a negative opacity", "4.0", "-0.1"),
    ] {
        let error = refuse(&format!(
            r#"{{"schema_version":1,"app":"x","output":"a","output_width":100.0,
                "output_height":100.0,"objects":[
                  {{"id":1,"color":[1,2,3],"width":{width},"opacity":{opacity},
                   "shape":{{"line":{{"from":[0.0,0.0],"to":[10.0,10.0]}}}}}}]}}"#
        ));
        assert_eq!(error.class(), "invalid", "{what} was accepted");
    }
}

#[test]
fn a_degenerate_shape_in_a_file_is_refused() {
    // FR-008 refuses to create an object nobody can see. A file must not be
    // able to smuggle one in.
    let error = refuse(
        r#"{"schema_version":1,"app":"x","output":"a","output_width":100.0,
            "output_height":100.0,"objects":[
              {"id":1,"color":[1,2,3],"width":4.0,"opacity":1.0,
               "shape":{"rectangle":{"a":[5.0,5.0],"b":[5.0,5.0]}}}]}"#,
    );

    assert_eq!(error.class(), "invalid");
}

#[test]
fn an_empty_stroke_is_refused() {
    let error = refuse(
        r#"{"schema_version":1,"app":"x","output":"a","output_width":100.0,
            "output_height":100.0,"objects":[
              {"id":1,"color":[1,2,3],"width":4.0,"opacity":1.0,
               "shape":{"stroke":{"highlighter":false,"points":[]}}}]}"#,
    );

    assert_eq!(error.class(), "invalid");
}

#[test]
fn an_unusable_output_size_is_refused() {
    for (what, width, height) in [
        ("zero width", "0.0", "100.0"),
        ("negative height", "100.0", "-5.0"),
    ] {
        let error = refuse(&format!(
            r#"{{"schema_version":1,"app":"x","output":"a","output_width":{width},
                "output_height":{height},"objects":[]}}"#
        ));
        assert_eq!(error.class(), "invalid", "{what} was accepted");
    }
}

#[test]
fn an_empty_output_binding_is_refused() {
    let error = refuse(
        r#"{"schema_version":1,"app":"x","output":"","output_width":100.0,
            "output_height":100.0,"objects":[]}"#,
    );

    assert_eq!(error.class(), "invalid");
}

#[test]
fn a_missing_file_reports_io_rather_than_corruption() {
    let dir = TempDir::new("missing");

    let error = load(&dir.join("nothing-here.json")).unwrap_err();

    assert_eq!(error.class(), "io");
}

#[test]
fn an_oversized_file_is_refused_without_being_parsed() {
    // NFR-003. The size is checked against the file's metadata, so a hostile
    // file cannot make the parser allocate its way through memory first.
    let dir = TempDir::new("huge");
    let path = dir.join("huge.json");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(ink_storage::MAX_DOCUMENT_BYTES + 1).unwrap();
    drop(file);

    let error = load(&path).unwrap_err();

    assert_eq!(error.class(), "too_large");
}

#[test]
fn a_refused_load_leaves_the_previous_file_alone() {
    // The whole point of FR-017: a bad file is refused, and everything that
    // was there before is still there.
    let dir = TempDir::new("intact");
    let good = dir.join("good.json");
    save(&sample(), &good).unwrap();
    let before = std::fs::read(&good).unwrap();

    let bad = dir.join("bad.json");
    std::fs::write(&bad, b"{ not a document").unwrap();
    assert!(load(&bad).is_err());

    assert_eq!(std::fs::read(&good).unwrap(), before);
    assert_eq!(load(&good).unwrap().document.len(), sample().len());
}
