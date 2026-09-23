# Coding-agent contract for ScreenInk

This repository starts as a specification kit, not an implemented Rust app. Preserve that distinction in all summaries.

## Before coding

Read `constitution.md`, `product-spec.md`, `architecture.md`, `platform-matrix.md`, the active feature spec, and the selected task in `tasks.md`. Inspect the real repository/environment rather than assuming crates, APIs, file paths, or tested platforms exist.

Implement one dependency-ready task at a time. Start with T001/T002 and the platform feasibility feature. The initial coding change is not the full toolbar or a windowed whiteboard.

## For each task

State the requirement IDs and acceptance scenarios being addressed. Add the relevant failing test or reproducible native experiment first. Implement the smallest change that satisfies the contract. Run the actual checks available on the host. Record exact commands/results and explain which platform tests could not run.

Update `tasks.json`, `tasks.md`, and `traceability.json` consistently when status changes. A spec-file validator only checks reference integrity; it cannot certify implementation. Do not mark a native scenario passed from a mock or compiler result.

## Non-negotiable boundaries

Do not put OS/window/GPU types into ink-core. Do not equate transparent pixels with correct input capture. Do not forward/inject clicks to emulate PassThrough. Do not require root or raw input devices for basic drawing. Do not use screenshots to fake a live overlay. Do not assume generic winit stacking works on Wayland. Do not report a GNOME fallback as full parity.

Respect GUI-thread and native-surface lifetime requirements. Do not force Send/Sync onto AppKit/Wayland objects or fabricate unsafe lifetimes. Use typed errors for unsupported/denied/conflicted operations and withdraw input-intercepting surfaces on recoverable failure.

Do not add capture, AI, OCR, a backend server, accounts, SQLite, a webview, or new dependencies without an active requirement/ADR justifying them. Dependency versions must be verified as mutually compatible and pinned in the actual workspace.

## Ambiguity or platform failure

Record the observation and proposed spec/ADR amendment. Preserve the original acceptance criterion until the change is explicit. Use blocked/not_tested rather than claiming success. A framework workaround must not secretly reduce visible pass-through to Draw/Hide.

## Change report

Report the task, files changed, requirement/scenario mapping, exact checks run, actual results, untested environments, discovered limitations, and next dependency-ready task. Never claim GUI/native testing that was not performed.
