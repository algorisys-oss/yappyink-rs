//! Reading and writing the local document file.
//!
//! Three rules shape everything here, all from FR-015 and FR-017:
//!
//! **A load never damages what is already open.** The incoming file is parsed
//! and fully validated into domain objects before anything is returned, so a
//! caller cannot end up half-way through adopting a bad document. Every failure
//! path leaves the caller's current document untouched, because the caller
//! still holds it.
//!
//! **A save never destroys the last good file.** Writing goes to a temporary
//! file in the destination directory, is flushed to disk, and only then
//! replaces the target by rename. An interrupted save leaves the previous file
//! exactly as it was.
//!
//! **Nothing here reads the screen.** A document is vector objects and styles.
//! There are no pixels in it, and saving annotations does not require any
//! capture permission (FR-014, ADR-003).

#![forbid(unsafe_code)]

pub mod error;
pub mod wire;

use std::io::Write;
use std::path::{Path, PathBuf};

use ink_core::{
    Document, LogicalPoint, LogicalSize, Object, ObjectId, Opacity, OutputId, Rgb, Shape,
    StrokeKind, Style, Width, limits,
};

pub use error::StorageError;
pub use wire::SCHEMA_VERSION;

/// The largest file this will read, from `product-spec.md`.
///
/// Checked against the file's size before any of it is parsed, so a hostile
/// file cannot make the parser allocate its way through memory first.
pub const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

/// A document read from disk, with what the caller needs to adopt it safely.
#[derive(Debug)]
pub struct LoadedDocument {
    pub document: Document,
    /// The output the file was saved against. A hint: the monitor may be gone
    /// or renamed, and FR-015 forbids silently stretching or moving the
    /// annotations to fit a different one.
    pub saved_output: OutputId,
    pub saved_size: LogicalSize,
    /// One past the highest object id in the file.
    ///
    /// The caller seeds its id source with this. Without it, ids issued after
    /// a load would collide with loaded ones, and an eraser would take the
    /// wrong object.
    pub next_object_id: u64,
}

/// Where a session is kept when no path is given.
///
/// `XDG_DATA_HOME`, or `~/.local/share`, which is where a user's own data
/// belongs. Returns an error rather than guessing when neither is available.
pub fn default_session_path() -> Result<PathBuf, StorageError> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| {
            StorageError::invalid(
                "the session path",
                "neither XDG_DATA_HOME nor HOME is set, so there is nowhere to put it",
            )
        })?;
    Ok(base.join("yappyink").join("session.json"))
}

/// Writes the document, replacing any previous file only once the new one is
/// safely on disk.
pub fn save(document: &Document, path: &Path) -> Result<(), StorageError> {
    let wire = to_wire(document);
    let json = serde_json::to_vec_pretty(&wire)
        .map_err(|e| StorageError::invalid("the document", e.to_string()))?;

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| StorageError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    write_atomically(path, &json)
}

/// Reads and validates a document.
///
/// Returns an error rather than a partial document. Nothing is returned until
/// every object has been checked, so a caller that adopts the result is
/// adopting something whole.
pub fn load(path: &Path) -> Result<LoadedDocument, StorageError> {
    let metadata = std::fs::metadata(path).map_err(|source| StorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(StorageError::TooLarge {
            path: path.to_path_buf(),
            bytes: metadata.len(),
            limit: MAX_DOCUMENT_BYTES,
        });
    }

    let bytes = std::fs::read(path).map_err(|source| StorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    from_bytes(&bytes, path)
}

/// Validates bytes that are already in memory.
///
/// Separated from [`load`] so the whole validation path can be tested without
/// touching a filesystem.
pub fn from_bytes(bytes: &[u8], path: &Path) -> Result<LoadedDocument, StorageError> {
    let wire: wire::WireDocument =
        serde_json::from_slice(bytes).map_err(|e| StorageError::Malformed {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

    // Checked before anything else is trusted. A newer schema may mean
    // anything at all about the rest of the file.
    if wire.schema_version > SCHEMA_VERSION {
        return Err(StorageError::UnsupportedVersion {
            path: path.to_path_buf(),
            found: wire.schema_version,
            supported: SCHEMA_VERSION,
        });
    }

    from_wire(wire)
}

fn to_wire(document: &Document) -> wire::WireDocument {
    wire::WireDocument {
        schema_version: SCHEMA_VERSION,
        app: format!("yappyink {}", env!("CARGO_PKG_VERSION")),
        output: document.output().as_str().to_owned(),
        output_width: document.output_size().width(),
        output_height: document.output_size().height(),
        objects: document.objects().map(object_to_wire).collect(),
    }
}

fn object_to_wire(object: &Object) -> wire::WireObject {
    let style = object.style();
    let point = |p: &LogicalPoint| [p.x, p.y];
    wire::WireObject {
        id: object.id().get(),
        color: [style.color.r, style.color.g, style.color.b],
        width: style.width.get(),
        opacity: style.opacity.get(),
        shape: match object.shape() {
            Shape::Stroke { kind, points } => wire::WireShape::Stroke {
                highlighter: *kind == StrokeKind::Highlighter,
                points: points.iter().map(point).collect(),
            },
            Shape::Line { from, to } => wire::WireShape::Line {
                from: point(from),
                to: point(to),
            },
            Shape::Arrow { from, to } => wire::WireShape::Arrow {
                from: point(from),
                to: point(to),
            },
            Shape::Rectangle { a, b } => wire::WireShape::Rectangle {
                a: point(a),
                b: point(b),
            },
            Shape::Ellipse { a, b } => wire::WireShape::Ellipse {
                a: point(a),
                b: point(b),
            },
        },
    }
}

fn from_wire(wire: wire::WireDocument) -> Result<LoadedDocument, StorageError> {
    let size = LogicalSize::new(wire.output_width, wire.output_height).ok_or_else(|| {
        StorageError::invalid(
            "the output size",
            format!(
                "{}x{} is not a usable size",
                wire.output_width, wire.output_height
            ),
        )
    })?;
    if wire.output.is_empty() {
        return Err(StorageError::invalid("the output binding", "it is empty"));
    }
    if wire.objects.len() > limits::MAX_OBJECTS_PER_OUTPUT {
        return Err(StorageError::invalid(
            "the document",
            format!(
                "it holds {} objects, over the limit of {}",
                wire.objects.len(),
                limits::MAX_OBJECTS_PER_OUTPUT
            ),
        ));
    }

    let output = OutputId::new(&wire.output);
    let mut document = Document::new(output.clone(), size);
    let mut highest = 0u64;

    for entry in wire.objects {
        let object = object_from_wire(entry, &output)?;
        highest = highest.max(object.id().get());
        // The document enforces its own limits again. Two checks rather than
        // one, because the file's count and what actually lands can differ.
        document
            .add(object)
            .map_err(|e| StorageError::invalid("an object", e.to_string()))?;
    }

    Ok(LoadedDocument {
        document,
        saved_output: output,
        saved_size: size,
        next_object_id: highest.saturating_add(1),
    })
}

fn object_from_wire(entry: wire::WireObject, output: &OutputId) -> Result<Object, StorageError> {
    let width = Width::new(entry.width).ok_or_else(|| {
        StorageError::invalid(
            "a width",
            format!("{} is not a positive finite number", entry.width),
        )
    })?;
    let opacity = Opacity::new(entry.opacity).ok_or_else(|| {
        StorageError::invalid(
            "an opacity",
            format!("{} is not within 0.0 to 1.0", entry.opacity),
        )
    })?;
    let style = Style::new(
        Rgb::new(entry.color[0], entry.color[1], entry.color[2]),
        width,
        opacity,
    );

    let point = |value: [f64; 2]| {
        LogicalPoint::new(value[0], value[1]).ok_or_else(|| {
            StorageError::invalid(
                "a coordinate",
                format!("({}, {}) is not finite", value[0], value[1]),
            )
        })
    };

    // Each shape goes through the domain's own constructor, so a file cannot
    // contain something the application would have refused to create: a
    // degenerate rectangle, an empty stroke, one with too many samples.
    let shape = match entry.shape {
        wire::WireShape::Stroke {
            highlighter,
            points,
        } => {
            let kind = if highlighter {
                StrokeKind::Highlighter
            } else {
                StrokeKind::Pen
            };
            let points = points
                .into_iter()
                .map(point)
                .collect::<Result<Vec<_>, _>>()?;
            Shape::stroke(kind, points)
        }
        wire::WireShape::Line { from, to } => Shape::line(point(from)?, point(to)?),
        wire::WireShape::Arrow { from, to } => Shape::arrow(point(from)?, point(to)?),
        wire::WireShape::Rectangle { a, b } => Shape::rectangle(point(a)?, point(b)?),
        wire::WireShape::Ellipse { a, b } => Shape::ellipse(point(a)?, point(b)?),
    }
    .map_err(|e| StorageError::invalid("a shape", e.to_string()))?;

    Ok(Object::new(
        ObjectId::from_raw(entry.id),
        output.clone(),
        style,
        shape,
    ))
}

/// Writes bytes to `path` without ever leaving it partially written.
///
/// The temporary file is in the destination directory, because a rename across
/// filesystems is not atomic and `/tmp` is frequently a different filesystem.
/// The data is flushed and synced before the rename, so a crash immediately
/// after leaves either the old file or the complete new one.
///
/// The rename is POSIX. Windows needs `ReplaceFileW` and different contention
/// handling, which `specs/003-local-storage` calls out and T031 owns; this
/// build has no Windows backend to test it with.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| StorageError::invalid("the path", "it does not name a file"))?;

    let mut temporary = directory.join(name);
    temporary.as_mut_os_string().push(".tmp");

    let write = || -> Result<(), std::io::Error> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        // Without this the rename can land before the contents do, and a power
        // loss leaves an empty file where a good one used to be.
        file.sync_all()?;
        Ok(())
    };

    if let Err(source) = write() {
        // Leaving debris behind after a failed save would accumulate.
        let _ = std::fs::remove_file(&temporary);
        return Err(StorageError::Io {
            path: temporary,
            source,
        });
    }

    std::fs::rename(&temporary, path).map_err(|source| {
        let _ = std::fs::remove_file(&temporary);
        StorageError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}
