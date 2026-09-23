# 002: deterministic drawing, erasing, and history

Status: specified, not implemented.

## User story

As a presenter, I draw simple annotations quickly, remove mistakes, and undo or redo whole actions without thinking about pointer events or graphics APIs.

## Domain contract

Implement FR-007 through FR-010 with the coordinate contract in FR-011/FR-012. The document stores vector objects, IDs, logical positions, and styles. Pointer samples live in a transient Gesture until committed.

One click with the pen creates a dot. One drag creates one stroke. A line/arrow/rectangle/ellipse previews during drag and commits on pointer-up; tiny or zero-area shapes have an explicit threshold and do not create invisible history entries. Record the threshold in logical units.

The highlighter stores the same kind of geometry with a different style/opacity. Rendering should not darken where triangles of a single stroke overlap; distinct strokes may accumulate. The sampling/smoothing policy must not move the pen far behind current input. Benchmark before adding complex filters.

MVP erasing removes whole objects whose geometry intersects the eraser sweep. A high-speed sweep must test swept geometry, not only widely spaced event sample points. The complete deletion set is one transaction; undo restores object order and styles exactly.

Undo removes one committed action, redo reapplies it, a new edit clears redo, canceling a preview does not, and Clear on an empty document is a no-op. History truncation happens on transaction boundaries.

## Core test sequence

Create stroke A, rectangle B, and stroke C. Erase A and C with one gesture. Undo once restores both in their original order. Undo again removes C. Redo restores C. Clear removes all. Undo restores all. After undoing a command, create D and verify redo is empty.

## Rendering tests

Test black, white, transparent, and high-contrast backgrounds; alpha edges; self-overlapping highlighter paths; narrow lines; tiny shapes; rotation; and fractional DPI. A rendered image comparison needs a defined tolerance and color/alpha space, not fragile cross-GPU byte equality.

## Acceptance

AC-FR-007, AC-FR-008, AC-FR-009, AC-FR-010, AC-FR-012, AC-NFR-001, and AC-NFR-004. Domain tests must run without a desktop or GPU. Native drawing tests establish that the same document behavior is reachable through actual input.
