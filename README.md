# ScreenInk: specification-driven development kit

**Working repository name:** ScreenInk. Branding and name availability are not established.
**Prepared for:** Rajesh Pillai.
**Date:** 22 September 2026.
**Status:** implementation specification, not an implemented or tested desktop application.

## Product goal

Build a Rust desktop application that lets a presenter draw directly above other applications on Linux, Windows, and macOS, then keep the ink visible while returning control to the underlying application.

The app must not replace the live desktop with a screenshot as a shortcut to claiming overlay support. A separate freeze feature may be added later.

## Start here

Read `constitution.md`, `product-spec.md`, `architecture.md`, and `platform-matrix.md`. Then implement `specs/000-platform-feasibility/` before building the complete tool palette. Follow `tasks.md` in dependency order. Use `AGENTS.md` as the coding-agent contract and `agent-start.md` as the first instruction.

`requirements.json` and `traceability.json` index the requirement-to-task-to-scenario mapping. Gherkin files under `tests/acceptance/` are **scenario specifications, not wired-up tests**. No native application, Rust workspace, installer, backend, or capture integration is supplied in this kit.

## Critical decision

Linux is not a single overlay platform. X11, layer-shell Wayland, and GNOME Wayland need separate acceptance evidence. Full GNOME parity remains a release gate. A fallback that hides the ink to let the user click does not satisfy visible pass-through.

The main application and drawing engine are proposed in Rust. A GNOME companion, if the feasibility work selects one, may require a small GJS/JavaScript component. This is a decision to expose, not silently disguise as a pure-Rust standalone solution.

## What is included

A product contract, architecture, platform capability matrix, UX state rules, three detailed feature specs, ADRs, prioritized tasks, acceptance scenarios, traceability metadata, quality/release gates, and source notes.

## Verification in this kit

`python tools/check_specs.py` checks internal IDs, links, task dependencies, and traceability references. It does not run desktop or Rust tests. `VALIDATION.md` records the exact verification performed on the kit.
