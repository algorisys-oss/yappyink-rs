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
/// On Windows, `%APPDATA%\yappyink\session.json`, the per-user roaming data
/// folder. Elsewhere, `XDG_DATA_HOME`, or `~/.local/share`, which is where a
/// user's own data belongs. Returns an error rather than guessing when none of
/// them is available.
pub fn default_session_path() -> Result<PathBuf, StorageError> {
    session_path_for(std::env::consts::OS, |name| std::env::var_os(name))
}

/// The resolution itself, with the platform and environment passed in so every
/// branch is tested on any machine.
///
/// Until 0.7.1 there was no Windows branch. Windows sets neither
/// `XDG_DATA_HOME` nor `HOME`, so the first person to run the Windows build
/// drew, pressed save, and was told there was nowhere to put it (E010).
fn session_path_for(
    os: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Result<PathBuf, StorageError> {
    let set = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
    };
    let base = if os == "windows" {
        set("APPDATA")
            .or_else(|| set("LOCALAPPDATA"))
            .ok_or_else(|| {
                StorageError::invalid(
                    "the session path",
                    "neither APPDATA nor LOCALAPPDATA is set, so there is nowhere to put it",
                )
            })?
    } else {
        set("XDG_DATA_HOME")
            .or_else(|| set("HOME").map(|home| home.join(".local/share")))
            .ok_or_else(|| {
                StorageError::invalid(
                    "the session path",
                    "neither XDG_DATA_HOME nor HOME is set, so there is nowhere to put it",
                )
            })?
    };
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
            Shape::Text { at, content, size } => wire::WireShape::Text {
                at: point(at),
                content: content.clone(),
                size: *size,
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
        wire::WireShape::Text { at, content, size } => Shape::text(point(at)?, content, size),
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

#[cfg(test)]
mod session_path_tests {
    use super::*;
    use std::ffi::OsString;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    /// The E010 bug, as a test: a Windows environment, which has APPDATA and
    /// no HOME, must still have somewhere to save.
    #[test]
    fn windows_saves_under_appdata() {
        let path =
            session_path_for("windows", env(&[("APPDATA", "C:/Users/v/AppData/Roaming")])).unwrap();
        assert_eq!(
            path,
            PathBuf::from("C:/Users/v/AppData/Roaming")
                .join("yappyink")
                .join("session.json")
        );
    }

    #[test]
    fn windows_falls_back_to_local_appdata_and_then_refuses() {
        let path = session_path_for("windows", env(&[("LOCALAPPDATA", "C:/L")])).unwrap();
        assert!(path.starts_with("C:/L"));
        assert!(session_path_for("windows", env(&[])).is_err());
        // HOME alone is not a Windows answer; it is usually unset there, and
        // when a shell sets it, it is not where Windows keeps app data.
        assert!(session_path_for("windows", env(&[("HOME", "/h")])).is_err());
    }

    #[test]
    fn unix_prefers_xdg_then_home() {
        let xdg = session_path_for("linux", env(&[("XDG_DATA_HOME", "/x"), ("HOME", "/h")]));
        assert_eq!(xdg.unwrap(), PathBuf::from("/x/yappyink/session.json"));
        let home = session_path_for("macos", env(&[("HOME", "/h")]));
        assert_eq!(
            home.unwrap(),
            PathBuf::from("/h/.local/share/yappyink/session.json")
        );
        assert!(session_path_for("linux", env(&[("XDG_DATA_HOME", "")])).is_err());
    }
}
