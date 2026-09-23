# ADR-001: shared Rust engine, explicit native overlay backends

Status: proposed, awaiting platform spike.

## Context

The product needs desktop input/stacking behavior rather than only an application window with a drawing widget. A single cross-platform window API does not establish parity, particularly on Wayland. [S03, S04, S08]

## Proposed decision

Keep the drawing model/controller in Rust. Use egui for controls and a replaceable GPU rendering layer. Use winit where sufficient and explicit native adapters where policy differs. Create a separate layer-shell client path rather than forcing all platforms through a generic toplevel window.

## Alternatives

A webview/Tauri UI could speed UI development but would not eliminate native overlay policy. A complete fork of Wayscriber could accelerate Wayland-specific features but carries its existing platform assumptions; inspect boundaries before committing to a port. A custom native UI per OS increases duplicated work. A windowed whiteboard does not satisfy the live-overlay contract.

## Consequences

A shared engine is testable without a desktop. Platform glue, focus rules, and event-loop integration remain real work. The renderer may be shared even when window creation cannot be. No percentage of code reuse is promised before implementation.

## Evidence required before acceptance

Link environment results for transparency, first-stroke capture, pass-through, keyboard handoff, global activation, and withdrawal. Pin the dependency set used. State whether GNOME is fully supported, companion-based, or still blocked.
