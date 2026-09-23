# Architecture and implementation plan

Status: proposed architecture, to be ratified after the platform spike.

## Decision summary

Use a shared Rust domain and application layer, egui for toolbar/settings UI, and a replaceable wgpu renderer. Use winit for platform windows where appropriate; do not make winit own every backend. Layer-shell Wayland needs its own native surface and event integration. [S03, S08, S12, S13, S14]

This is not a webview application by default. A Tauri settings window could be added later, but it is not a solution to desktop stacking and input policy. Keeping the drawing surface in the native Rust stack makes those concerns explicit. This is an architectural preference, not a claim that webview-based tools are impossible.

## Logical architecture

```text
Toolbar / local shortcuts / global actions / CLI
                         |
                 application commands
                         |
              controller + mode coordinator
                  /          |          \
        document core    tool pipeline    effect execution
            |                |              /       \
      objects + history  transient preview  platform  persistence
            \               /               |
               scene snapshot         normalized native events
                     |
             renderer + UI painter
                     |
      platform-owned per-output native surface
                     |
         live applications remain underneath
```

## Proposed workspace

```text
screenink/
  Cargo.toml                   # create during implementation
  Cargo.lock                   # pin actual compatible dependencies
  rust-toolchain.toml
  apps/screenink/
  crates/ink-core/              # objects, commands, undo, geometry types
  crates/ink-app/               # controller, mode transitions, orchestration
  crates/ink-render/            # scene -> cached geometry -> GPU commands
  crates/ink-ui/                # toolbar/settings; emits commands
  crates/ink-platform/          # typed capability/request/event contracts
  crates/ink-platform-winit/    # common Windows/macOS/X11 integration
  crates/ink-platform-wayland/  # native layer surfaces + event loop
  crates/ink-storage/           # validated document/settings files
  integrations/gnome/           # only if ADR-002 selects a companion
  specs/
  tests/
  docs/adr/
```

Avoid an empty crate for every noun. Start with a small workspace and split when boundaries contain real code. Windows/macOS/X11 native adapter modules can live under ink-platform-winit until independent crates are justified.

## Dependency direction

ink-core knows no egui, winit, wgpu, portal, or OS type. ink-app uses ink-core and abstract service contracts. Adapters implement those contracts. The executable selects a platform driver and wires services together. The renderer consumes snapshots, not mutable native window state.

UI widgets do not call OS APIs directly. A toolbar click becomes the same AppCommand as a keyboard action. A command creates domain changes and/or typed effects. Events from effects update application state on the UI thread.

## Window/event-loop ownership

Each platform driver owns its native event loop and window/surface lifetimes. Do not create independent winit and Wayland event loops that both assume control of the same GUI thread.

On the native Wayland path, create wl_surface and its layer-shell role deliberately. Do not take an xdg-toplevel surface already created by winit and try to assign a second incompatible role. Implement the necessary pointer/keyboard, configure, output-scale, and egui RawInput translation for this backend. The SCTK layer example is a starting point, not a drop-in cross-platform window. [S08, S12, S13]

A platform surface can expose a rendering target through safe ownership/handles supported by the selected graphics stack. Keep the native display and surface alive longer than their GPU surface. If raw-handle construction needs unsafe code, isolate and document those lifetime guarantees. Transparency/alpha support must be validated for the chosen backend and GPU.

macOS UI/AppKit work stays on the main thread. Observe the global-hotkey crate's main-thread/event-loop rules. Portal and filesystem work returns results through events rather than blocking that thread. [S10]

## State and effect boundary

The controller maintains desired_mode and effective_mode separately. A request to enter PassThrough is not declared effective before the platform confirms the input and focus policy has been applied.

Transitions have a generation/transition ID so a delayed callback from an older transition cannot overwrite current state. While transitioning, stop accepting new gestures. A failed transition withdraws the interactive surface, reports the error, and stays Hidden/Faulted rather than pretending success.

See `ux-state-machine.md` and `specs/001-overlay-input/` for exact semantics. The platform abstraction should be request/event-oriented; avoid a giant synchronous trait promising instant success for compositor operations.

## Rendering and document design

Store retained vector objects in the domain even though the UI is immediate-mode. Proposed primitives are Stroke, Line, Arrow, Rectangle, and Ellipse, each with an ID, style, output ID, and logical coordinates. V1 adds text and transforms.

Use output-local top-left logical units. Width 4.0 means four logical units, not four physical pixels. Scale to render pixels only at the boundary. On macOS invert native coordinate direction where required. Handle rotation explicitly. Wayland backend events already identify a surface/output; do not invent global cursor coordinates. [S04, S12]

An in-progress stroke has a sampled-point buffer and preview cache; it is not yet part of undo history. Keep committed geometry cached and redraw only when there is a changed scene, UI interaction, resize, or active animation. Batch input processing without dropping gesture start/end/cancel ordering.

Use a single documented premultiplied-alpha convention through geometry colors, shader blending, textures, and surface configuration. Transparent clear must remain transparent. A black window, white fringe, or dark highlighter seam is a bug, not an accepted visual fallback.

MVP geometry may start with egui/epaint-compatible meshes for simple shapes, but the scene format must remain independent. If highlighter semantics or stroke quality requires custom tessellation, change the renderer without changing document semantics. Do not implement two renderers before the first one is profiled.

## Commands and history

Domain commands include AddObject, DeleteObjects, ClearOutput, and (V1) TransformObjects/SetObjectStyle. Store inverse information or before/after object values sufficient to undo without re-reading the desktop. An eraser gesture groups its deletion set into one command.

A no-op command adds no history. Canceling a preview preserves redo. Applying a new committed edit truncates redo. History budgets are enforced at transaction boundaries, with a user-visible history limit rather than corrupting the document.

## Persistence

Start with versioned local JSON for the document and a small settings file. A database is not needed for a single local annotation session. Add SQLite only after a requirement such as searchable session catalogs makes it worthwhile.

The document contains schema_version, app metadata, output bindings/logical dimensions, objects, and styles. MVP need not persist undo history. Loading resolves missing outputs through an explicit remap UI; never use a stale monitor index as identity. Output names/IDs may change across reboots, so persistence bindings are best-effort hints, not guaranteed physical identities.

Serialize to a same-directory temporary file, flush as appropriate, then use platform-appropriate atomic replacement. Handle Windows replacement semantics deliberately. Validate and stage the entire incoming document before replacing the in-memory one. Reject nonfinite coordinates, invalid opacity/width, excessive sizes, and unknown newer schema versions without clobbering current work.

## Capture is a separate service

Live overlay does not require a desktop image. Later CaptureService manages user selection, permissions, and optional pixel acquisition. Linux may use screenshot/screencast portals; macOS evaluates ScreenCaptureKit; Windows selects a documented native capture API during that feature's design. [S17, S18, S19]

Composite exports must avoid capturing the existing ink and drawing it again. Prefer explicit capture exclusion where supported, otherwise carefully withdraw application surfaces and synchronize before capture. Specify cancellation, self-capture, permission revocation, and toolbar exclusion before implementation. Capture success never implies overlay success.

## Concurrency and resource budget

Keep document mutation and native UI events on one owner thread. Use bounded worker jobs for file I/O and optional capture/geometry work; return immutable results. Introduce an async runtime only when a concrete integration requires it. Never clone the whole scene on every pointer sample.

On output loss or device loss, cancel the transient gesture, keep committed scene data, withdraw input-intercepting surfaces, and recreate only after the capability/lifecycle state permits it. Recoverability must be visible in diagnostics.

## Why this split matters

The pen algorithm should not know whether input came from Win32, AppKit, X11, or Wayland. Platform code should not know how undo works. This makes it possible to fix GNOME integration or replace a renderer without rewriting the annotation model.
