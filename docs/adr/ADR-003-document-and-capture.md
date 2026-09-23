# ADR-003: vector documents first; capture is a separate service

Status: proposed.

## Decision

The live drawing path does not read desktop pixels. Retain annotations as vector objects with output-local logical coordinates and command history. Use explicit versioned local files before introducing a database.

Capture/freeze is a later feature with its own permission and lifecycle contract. It is never a hidden implementation of the normal annotation mode. [S17, S18, S19]

## Consequences

Basic drawing has a smaller permission/privacy surface. Undo and tests do not depend on external application content. PNG/SVG ink-only export can operate independently of screen capture. Composite screenshots must explicitly address self-capture and accidental duplicate ink.

## Revisit when

A session catalog requires queries, users request content-following annotations, or capture performance requires a different pixel pipeline. None of these changes should force a rewrite of the command model.
