# Product specification

**Status:** draft baseline for implementation. All support and performance results are unverified until measured.
**Core promise:** draw over a live desktop, then operate the desktop without losing the visible annotations.

## User and business case

A developer or trainer is explaining code, a browser workflow, a presentation, or an architecture screen during a live session. They should be able to circle a function, add an arrow, then scroll or click in the underlying application while their explanation remains visible. The annotation layer belongs to the screen, not the content of the application underneath.

Example: show code in an editor, activate Draw, highlight a function, return to PassThrough, run the program, hide the annotations, and show the same annotations later. Scrolling does not attach ink to source-code lines. Content-tracking annotations are out of scope.

## Meaning of “any application”

The supported contract covers ordinary desktop applications on explicitly certified environments. Borderless/fullscreen apps and macOS Spaces require their own evidence. The product intentionally does not promise overlays above lock screens, secure credential desktops, protected/DRM surfaces, or exclusive fullscreen rendering. It does not bypass platform permissions or security UI.

Linux support includes a specific GNOME investigation because a typical Ubuntu setup may use GNOME Wayland. A limited windowed or xdg-shell fallback is useful but does not satisfy full visible-pass-through parity. See `platform-matrix.md` and ADR-002. [S01, S03, S09]

## Scope by release

**MVP:** one selected monitor at a time; live overlay; pen/highlighter; line/arrow/rectangle/ellipse; object eraser; undo/redo/clear; hidden/draw/pass-through modes; toolbar; activation and recovery; diagnostics; explicit local vector save/load.

**V1:** simultaneous multiple outputs, robust monitor/session lifecycle, Unicode text and object editing, ink-only SVG/PNG export, accessible controls, installers and platform certification.

**Later:** screenshot/freeze, composite screenshots, whiteboard modes, pressure input, presenter effects, and optional capture integrations. Do not conflate these with basic live drawing.

**Not planned for the first release:** cloud collaboration, AI/OCR, video recording, plugins, accounts, an embedded browser, raw global input monitoring, content-following ink, or a full vector-design editor.

## Requirements

| ID | Phase | Name | Contract |
|---|---|---|---|
| FR-001 | MVP | Live desktop overlay | Display annotation ink above normal desktop applications without capturing or freezing their content. The desktop underneath must keep updating. Fullscreen, Spaces, and compositor guarantees are separately certified. |
| FR-002 | MVP | Draw mode | In Draw mode, the canvas receives pointer input across its entire selected output, including visually transparent pixels. Pointer events used to draw must not activate controls underneath. |
| FR-003 | MVP | True pass-through | In PassThrough mode, ink remains visible while ordinary pointer, wheel, and keyboard interaction belongs to underlying applications. Do not implement this by forwarding or synthesizing input. |
| FR-004 | MVP | Hide without clearing | Hidden mode removes canvas and toolbar visibility and input interception while retaining committed annotations in memory. Startup begins Hidden. |
| FR-005 | MVP | Activation and escape | Provide configurable ToggleDraw and EmergencyHide actions, registration/conflict feedback, a non-tray recovery path, and a CLI control command. Wayland may require the GlobalShortcuts portal or a user-configured compositor shortcut. |
| FR-006 | MVP | Toolbar isolation | Provide a compact floating toolbar. Its pointer events never create strokes. It is a separate platform surface or a backend-proven input region, not an imaginary clickable button on an entirely click-through surface. |
| FR-007 | MVP | Pen and highlighter | Support freehand pen and highlighter, configurable color, width in logical units, and opacity. A dot is a valid pen gesture; a canceled gesture is not an undoable document edit. |
| FR-008 | MVP | Basic shapes | Support line, single-ended arrow, rectangle, and ellipse with a live preview. One completed drag creates one object and one history entry. |
| FR-009 | MVP | Object eraser | Erase whole annotation objects intersecting the eraser gesture. An eraser drag is one undoable transaction; it never modifies another application or captured background pixels. |
| FR-010 | MVP | Undo, redo, and clear | Undo/redo act on completed user actions, not pointer samples. Clear is undoable. A new committed edit clears the redo branch; a no-op does not. |
| FR-011 | MVP | Output-local document | Own annotations by an application OutputId and store output-local logical coordinates. One selected output is sufficient for MVP, but output switching must preserve each document. |
| FR-012 | MVP | Coordinate correctness | Convert native input, render pixels, output rotation, and display scaling at the backend boundary. Test fractional and mixed DPI. Do not assume a universal Wayland global desktop coordinate space. |
| FR-013 | MVP | Capabilities and honest fallback | Probe and report overlay, pass-through, shortcut, focus, and output capabilities. Unknown is not Supported. A limited GNOME/windowed fallback must never be advertised as full live-overlay parity. |
| FR-014 | MVP | Minimal privileges | Basic overlay drawing must not depend on screenshot permission, root, administrator elevation, raw input devices, screen recording, or system-wide keystroke monitoring. Do not bypass operating-system security boundaries. |
| FR-015 | MVP | Explicit local document files | Save/load an explicit versioned vector document locally. Loading is atomic from the user perspective: validation failure leaves the existing document intact. Saving annotations does not capture the desktop. |
| FR-016 | MVP | Privacy defaults | No accounts, cloud upload, analytics, content logging, or automatic screenshot retention. Vector autosave is off by default for the MVP. Explicit file exports are user-directed. |
| FR-017 | MVP | Safe file and settings handling | Validate document/settings sizes and numeric values. Use same-directory temporary writes and platform-appropriate atomic replacement, retaining the previous valid file after failure. |
| FR-018 | MVP | Gesture and transition safety | Cancel transient drawing on EmergencyHide, focus/input cancellation, or output loss. During a mode transition no already-held pointer button may leak into an accidental underlying click or start a new stroke. |
| FR-019 | MVP | Fault recovery | On recoverable surface, GPU, or mode-application failure, withdraw interactive overlays before reporting the fault. On backend disconnect clean up resources. Do not show PassThrough as effective until its native transition completes. |
| FR-020 | MVP | Settings and diagnostics | Persist validated tool preferences and user-approved shortcut bindings. Provide a doctor command with backend, versions, capabilities, and failure reasons, without document text or desktop pixels. |
| FR-021 | V1 | Simultaneous multiple outputs | Support multiple active per-output surfaces. A stroke cannot jump coordinates between outputs; crossing an output boundary ends the current stroke. A new press begins a new stroke on the other output. |
| FR-022 | V1 | Hotplug and session lifecycle | Handle output removal, reconnection, scale/rotation changes, lock/unlock, suspend/resume, and workspace changes without invisible input-blocking surfaces or silent annotation loss. |
| FR-023 | V1 | Text and editing | Add Unicode text with IME/preedit support, selection/move/delete, and style editing. Text edit keyboard focus is explicit and released on leaving the editor. |
| FR-024 | V1 | Annotation-only export | Export transparent PNG and SVG from the vector scene without reading desktop content. Clearly label these as ink-only exports, not screenshots. |
| FR-025 | V1 | Distribution and verification | Package for Windows, macOS, and selected Linux environments. Every supported environment has native acceptance evidence; a successful cross-compilation alone is insufficient. |
| FR-026 | Later | Capture and freeze | Add explicit user-initiated screenshot capture and freeze as separate capabilities. Permission denial/cancellation must leave live overlay usable; freeze blocks underlying interaction until dismissed. |
| FR-027 | Later | Screenshot composite export | Capture the selected output, then composite committed annotations exactly once, excluding app controls and avoiding recursive self-capture. Treat captured pixels as sensitive and ephemeral by default. |
| FR-028 | Later | Presenter extras | Consider whiteboard/blackboard, fading laser in Draw mode, spotlight, and pressure input after their platform and interaction contracts exist. Passive system-wide cursor effects are a separate opt-in project. |
| FR-029 | V1 | Live zoom | Zoom a live, continuously updating view of the screen that follows the pointer, in steps, from a toolbar button and a chord, in any mode. Use the platform compositor's magnifier where one exists so no desktop pixels reach the app and no capture permission is needed; ink is magnified with the content it marks. Restore any user setting changed to drive a system magnifier. See ADR-008 and specs/004-live-zoom. |
| NFR-001 | MVP | Drawing responsiveness | Proposed target on a documented 1080p/60 Hz reference machine: p95 input-dispatch-to-render-submit <= 16.7 ms for the benchmark scene. Measure input-to-photon separately; submission is not display latency. |
| NFR-002 | MVP | Idle efficiency | When Hidden or displaying unchanged ink with no animation, schedule no continuous redraw loop. Proposed Hidden CPU target: < 1% of one logical core averaged over 60 seconds on a recorded reference machine. |
| NFR-003 | MVP | Responsiveness and memory bounds | No blocking capture, disk, or portal calls in the GUI event loop. Bound input queues, geometry caches, object count, and undo history; report truncation or limits rather than silently growing without bound. |
| NFR-004 | MVP | Testable core | The domain and command reducer have no OS, window, GPU, or portal dependencies. Their tests run headlessly and deterministically with injected clocks/IDs. |
| NFR-005 | MVP | Failure visibility | Typed errors distinguish Unsupported, PermissionDenied, ShortcutConflict, Disconnected, SurfaceLost, and InvalidData. Unsupported native operations must not return fake success. |
| NFR-006 | V1 | Accessible controls | All toolbar actions have labels, keyboard navigation, visible focus, and a non-color-only selected state. Verify platform accessibility output; egui selection alone is not evidence of conformance. |
| NFR-007 | MVP | Traceable development | Every implementation change references requirement IDs, scenario IDs, and a task. Tests, evidence, changed assumptions, and remaining gaps are recorded before a task is marked verified. |
| NFR-008 | V1 | Dependency and release hygiene | Pin a compatible Rust toolchain/dependency set during implementation, commit Cargo.lock, review unsafe FFI, audit licenses/dependencies, and record platform signing/package checks. No guessed version compatibility. |

## Input and content rules

A completed pen gesture is one object. Undo never removes half a stroke because there were many pointer samples. An eraser drag removes a set of objects as one transaction. Clear creates one inverse command when the document is nonempty.

Committed document state is separate from the transient gesture preview. A failed or canceled mode change must not accidentally commit a stroke. Draw-mode pointer capture may last only for a currently owned gesture; all such capture must be released before control is handed back.

Highlighter opacity is applied to the completed stroke as a whole: tessellation overlaps inside that stroke should not create dark seams. Separate strokes may intentionally accumulate opacity. Geometry and visual regression tests must include this distinction.

## Proposed limits and benchmark fixture

Implementation starting limits: 10,000 objects per output, 100,000 samples per single stroke, 64 MiB maximum input document, 1,000 undo transactions or a 64 MiB estimated history budget, whichever is reached first. These are defensive design defaults, not measured optimal values. Revisit with benchmark evidence and an ADR.

A reproducible normal drawing fixture contains 1,000 committed objects and 100,000 total sampled points on a 1920 × 1080 output at 60 Hz. A stress fixture targets the configured limits on a 4K output. Record CPU/GPU model, OS/compositor, driver, power mode, display scaling, app commit, and build profile.

## Definition of MVP acceptance

Each environment advertised as supported passes every MVP scenario applicable to it and provides platform evidence. The full cross-platform target is not complete while GNOME parity remains unproven. An earlier preview can explicitly state its narrower support matrix.

Source identifiers resolve in `sources.md`. Requirements and numerical targets are this project's proposed design choices, not claims made by the sources.
