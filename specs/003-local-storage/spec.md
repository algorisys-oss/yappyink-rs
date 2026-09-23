# 003: explicit local vector sessions

Status: specified, not implemented.

## User story

As a presenter, I save my annotations and later reopen them without granting screen capture permission or storing the applications that were behind them.

## Contract

Implement FR-015 through FR-017 and settings/diagnostics in FR-020. Use a versioned local document format. MVP stores objects/styles/output binding hints but need not persist undo history. Loading a document creates a fresh history baseline.

The file must never contain captured desktop pixels unless a later explicitly approved capture format says so. Vector autosave is initially off. Users can still save sensitive annotation text, so logs must not print object contents.

Validate finite coordinates, width > 0, opacity within [0, 1], known object/schema variants, object counts, point counts, and total file size before replacing the active document. Unknown newer schema versions fail non-destructively. Stage migration in memory and retain the original source file.

Output mappings are hints. A missing or changed monitor requires a clear remapping choice. Do not silently stretch, move, or discard the user's annotations to fit a different output. A portable export may define an explicit scaling choice later.

Write to a temporary file in the destination directory, then perform a platform-appropriate atomic replace. Handle disk full, access denied, interrupted write, and target-file contention. Keep the last valid file and current in-memory document on failure. Clean up abandoned temporary files safely.

Settings are schema-validated independently from documents. Corrupt shortcut preferences must not disable all recovery controls. Provide a reset/settings recovery path that starts Hidden.

## Acceptance

AC-FR-015, AC-FR-016, AC-FR-017, AC-FR-020, and AC-NFR-003. Use temporary directories and deterministic document fixtures. Include real Windows replacement behavior in platform tests rather than assuming POSIX semantics.
