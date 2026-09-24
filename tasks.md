# Implementation tasks

Status vocabulary: not_started, in_progress, implemented, verified. `implemented` means the code exists and the recorded checks passed; it does not mean native desktop behavior was demonstrated. This file was written when every task was not_started and the repository contained no Rust application. Read dependencies before choosing the next task. A blocked platform result may close a feasibility investigation, but it does not satisfy the blocked product capability.

M0 = feasibility; M1 = overlay foundation; M2 = MVP; M3 = V1; M4 = optional later features.

## T001 [M0]: Create the implementation workspace and pin dependencies

Status: implemented on 2026-09-23. Dependencies: none.

Requirements: NFR-004, NFR-007, NFR-008.

Create a minimal Rust workspace, record toolchain and compatible dependency choices, enable fmt/clippy/core tests. Do not create placeholder implementations for every platform.

**Exit criterion:** A clean checkout builds the selected spike target; lockfile and actual version choices are recorded.

**Evidence:** Workspace `Cargo.toml` (resolver 3, edition 2024), `rust-toolchain.toml` pinning 1.95.0, `crates/ink-core` (no dependencies, 6 headless tests), `apps/yappyink`. Versions and checks recorded in `docs/toolchain-and-dependencies.md`. `cargo build --workspace --locked --offline` verified in a copy of the tree with `target/` excluded. No native, GUI, Windows, or macOS behavior is tested by this task.

## T002 [M0]: Detect the actual Ubuntu session and capability surface

Status: in_progress since 2026-09-23. Dependencies: T001.

Requirements: FR-013, FR-020, NFR-005.

Add diagnostic probing for active session, outputs, available protocols, shortcut portal, and failure reasons. Keep unknown distinct from unavailable.

**Exit criterion:** A doctor report from the real machine identifies the native backend and states what was not probed.

**Evidence:** `docs/evidence/E001-ubuntu-gnome-wayland.md` with the captured report in `E001-doctor-output.txt`. `yappyink doctor` resolves the session by connecting to a socket rather than reading a desktop name, lists the advertised Wayland globals, and enumerates outputs over the protocol. On the development machine (Ubuntu 24.04.4, GNOME Shell 46.0, Mutter 46.2) **`zwlr_layer_shell_v1` is not advertised**; the only shell protocol is `xdg_wm_base` v6. Every overlay capability is reported `unknown` with the task that would settle it, and only `output_enumeration` is `available`.

**Remaining before this task closes:** the `org.freedesktop.portal.GlobalShortcuts` interface is still unprobed, because no D-Bus client dependency has been chosen; X11 is only probed as far as socket reachability (T005 owns the rest); Windows and macOS have no probe at all.

## T003 [M0]: Prove the Windows overlay path

Status: in_progress since 2026-09-24. Dependencies: T001.

Requirements: FR-001, FR-002, FR-003, FR-005, FR-018.

Build only the transparent surface, fixed ink, one-stroke input, mode switching, and exit controls. Check empty transparent hit testing and other-process clicks.

**Exit criterion:** Native Windows evidence records success/failure for live overlay, no click leakage, visible pass-through, and withdrawal.

**Probe written, 2026-09-24, not yet run.** `experiments/windows-layered` is a throwaway feasibility program in the same spirit as `experiments/gnome-xdg-shell`: it depends on nothing in this workspace, because borrowing our own renderer would make a failure ambiguous between the platform and us. It puts up a layered, top-most, non-activating window on a chosen monitor, paints premultiplied BGRA straight into a top-down DIB through `UpdateLayeredWindow`, draws with the mouse, and switches pass-through by toggling one extended style bit. `WS_EX_TRANSPARENT` is real routing by the window manager, so FR-003 is satisfiable here without forwarding or synthesising anything, which is the boundary `AGENTS.md` draws.

Six questions are posed in the program's own documentation and repeated on screen when it starts, mapped to FR-001, FR-002, FR-003, FR-018 and FR-005. Two of them are the interesting ones, because they are the capabilities GNOME measurably lacks: choosing which monitor to appear on, and registering a global shortcut with real conflict feedback. `RegisterHotKey` fails loudly when another application owns a chord, which is feedback Wayland cannot give because it has no registration at all.

The dependency is `windows-sys` 0.61.2, target-gated, justified in `docs/adr/ADR-005-windows-bindings.md`.

**What has actually been verified: that it compiles, and nothing else.** `cargo check` and `cargo clippy -D warnings` pass against `x86_64-pc-windows-gnu` from the development machine, and CI lints it on a real Windows runner. **No part of it has been run, and a CI runner cannot answer any of the six questions, because every one of them is about what a person sees on a screen.** Every Windows capability stays `unknown` until the owner runs it and reports. This status is `in_progress` and not `implemented` for exactly that reason.

## T004 [M0]: Prove the macOS overlay path

Status: in_progress since 2026-09-24. Dependencies: T001.

Requirements: FR-001, FR-002, FR-003, FR-005, FR-018.

Evaluate the minimal winit/AppKit path, pointer transparency, focus policy, deactivation, and Spaces/fullscreen separately.

**Exit criterion:** Native macOS evidence records ordinary-app behavior and any Spaces/fullscreen restrictions.

**Probe written, 2026-09-24, and unrunnable here.** `experiments/macos-overlay` is a throwaway program in the same spirit as the other two: a borderless `NSWindow` at screen-saver level with a clear background, a custom `NSView` that paints a frame, a badge and strokes, and pass-through by `setIgnoresMouseEvents:`. That last one is AppKit's own hit-test routing, so FR-003 is reachable without forwarding or synthesising anything. The dependency is `objc2` 0.6 with `objc2-app-kit` and `objc2-foundation`, target-gated, justified in `docs/adr/ADR-006-macos-bindings.md`.

Two findings came out of writing it, before any of it ran:

**The focus tension has no free answer.** Windows gets "never take focus" from `WS_EX_NOACTIVATE` and pays for it by having no keyboard at all, which is fine because `RegisterHotKey` supplies global shortcuts. macOS has `NSApplicationActivationPolicy::Accessory`, which keeps the app out of the Dock and stops it activating on launch, but a window that accepts a key press must be able to become key, and that takes focus from the application being annotated. The probe uses ordinary key presses and records the tension rather than pretending it is solved.

**FR-005 has no equivalent at all.** A global shortcut on macOS needs either Carbon's `RegisterEventHotKey` or an accessibility-permission grant. A permission prompt is a product decision, so the probe deliberately does not attempt one, and ADR-006 says it needs its own ADR.

**What has been verified: that it compiles.** `cargo check` and `cargo clippy -D warnings` against `aarch64-apple-darwin`, and CI lints it on a macOS runner. **Nobody on this project has a Mac.** It has never been launched, and no CI runner can answer any of its questions, because every one of them is about what a person sees on a screen. Unlike T003, where the owner has the hardware, **there is currently no route to closing this task.** Every macOS capability stays `unknown`. If that does not change, the honest outcome is to declare macOS unsupported rather than ship it quietly; ADR-006 says so explicitly.

## T005 [M0]: Prove the composited X11 path

Status: not_started. Dependencies: T001.

Requirements: FR-001, FR-002, FR-003, FR-005, FR-018.

Validate transparency, stacking hints, pointer input shape, keyboard release, and activation on an actual X11 desktop.

**Exit criterion:** Native X11 evidence includes compositor/WM versions and no hidden keyboard grab.

## T006 [M0]: Prove native layer-shell Wayland

Status: not_started. Dependencies: T002.

Requirements: FR-001, FR-002, FR-003, FR-005, FR-018.

Create a correctly assigned layer surface, configure/scale lifecycle, input-region switching, keyboard policy, and a portal or desktop-configured action.

**Exit criterion:** Recorded layer-shell environment passes the input fixture checks; missing protocols are reported, not faked.

## T007 [M0]: Resolve the GNOME implementation route

Status: in_progress since 2026-09-23. Dependencies: T002.

Requirements: FR-001, FR-002, FR-003, FR-005, FR-013.

Measure standalone xdg-shell behavior, then prototype the minimum companion only if needed. Record install, lifecycle, and Rust/helper boundaries.

**Exit criterion:** ADR-002 identifies a verified route or explicitly leaves full GNOME parity blocked. A limited fallback is not a pass.

**Step 3 (2026-09-23):** a GNOME Shell companion prototype exists in `integrations/gnome/`, scoped to the two things a Wayland client cannot do for itself: staying above other windows, and being placed and sized. About 100 lines of GJS, declaring Shell 46 only. **It has never been loaded by a running Shell**; `docs/evidence/E007-gnome-shell-extension.md` holds the checklist and the question it exists to answer, which is whether a window sized to the whole monitor keeps its transparency when a fullscreen one does not.

**Result so far (2026-09-23):** the standalone route fails the full contract but yields a narrow limited mode. Fullscreen destroys transparency; a non-fullscreen surface cannot choose its output; a maximized surface cannot be raised above; a **floating** surface with the user applying "Always on Top" from Mutter's window menu **does** keep ink visible while input reaches the application underneath. ADR-002 records this as outcome (c), a limited preview with a `NeedsUserAction` capability, and leaves the companion route (b) as the only path to parity. Details in `docs/evidence/E002-gnome-xdg-shell-experiment.md`.

**Probe:** `experiments/gnome-xdg-shell/` implements the standalone xdg-shell probe: a transparent fullscreen surface with a fixed diagonal, a timed switch to an empty `wl_surface` input region for pass-through, and self-withdrawal. A windowed smoke test on 2026-09-23 confirmed it maps, configures, switches the input region, and withdraws with no protocol error. **It has not been observed on screen**, so nothing is yet known about stacking, transparency, or pass-through on Mutter. The observation checklist is `docs/evidence/E002-gnome-xdg-shell-experiment.md`.

## T008 [M0]: Ratify architecture and support intent

Status: not_started. Dependencies: T003, T004, T005, T006, T007.

Requirements: FR-013, FR-025, NFR-007, NFR-008.

Compare platform evidence; update ADRs and minimum tested environments. Decide full release versus an explicitly limited preview.

**Exit criterion:** The architecture and dependency set are evidence-backed; every claimed environment has a result, including failures/blocked states.

## T009 [M1]: Implement the platform-free document model

Status: implemented on 2026-09-23. Dependencies: T007 (amended from T008 by ADR-004; T008 remains the gate for support claims).

Requirements: FR-011, FR-012, NFR-004.

Add typed output-local coordinates, object IDs, styles, vector objects, and document validation without GPU/OS dependencies.

**Exit criterion:** Headless tests cover valid objects, invalid numeric values, and independent output documents.

**Evidence:** `crates/ink-core/` gains `geometry`, `id`, `style`, `object`, `document`, `error`, and `limits`. All five shapes, validated `Width` and `Opacity`, degenerate-drag rejection, stroke and object limits, per-output independence, and paint order. 20 headless tests in `tests/document.rs` and `tests/identity.rs`, no dependencies. `cargo fmt`, `cargo clippy -D warnings`, and `cargo test --workspace` (42 tests) all clean.

## T010 [M1]: Implement the pure interaction reducer

Status: implemented on 2026-09-23. Dependencies: T009.

Requirements: FR-002, FR-003, FR-004, FR-018, FR-019.

Add desired/effective modes, transient gesture states, transition IDs, cancellation, and typed effects.

**Exit criterion:** Table-driven tests cover each allowed transition, held-button rules, failure rollback, and stale completion events.

**Evidence:** `crates/ink-app/` holds the controller: desired and effective modes kept apart, numbered transitions so a late callback cannot overwrite current state, gesture cancellation on any mode change, held-button suppression on re-entering Draw, EmergencyHide that never awaits confirmation, and a failed transition that withdraws before it reports. 25 headless tests in `tests/reducer.rs`, no platform dependency. The suite was mutation-checked: deliberately removing the transition-id comparison and turning a cancellation into a commit both made tests fail, and the first mutation exposed a missing case that is now covered.

## T011 [M1]: Integrate production overlay lifecycle

Status: in_progress since 2026-09-23. Dependencies: T010.

Requirements: FR-001, FR-002, FR-003, FR-004, FR-019.

Connect the reducer to native adapters, surface ownership, focus/hit-test changes, and confirmed completion events.

**Exit criterion:** The native fixture validates effective-state reporting and no invisible blockers.

**Progress:** `yappyink draw` runs the overlay against the real compositor. `crates/ink-render` (CPU rasteriser, premultiplied ARGB8888, 9 pixel tests) and `crates/ink-platform-wayland/src/overlay.rs` (surface, event loop, effect execution) join the controller and document into a working application. A smoke test on 2026-09-23 created and mapped the surface, entered Draw, and bound to HDMI-1. **On-screen behaviour has not been observed**; the checklist is `docs/evidence/E004-vertical-slice.md`. Limitations carried from E002/E003: Always on Top is manual, the output cannot be chosen, and the surface does not cover the output.

## T012 [M1]: Implement activation, CLI control, and recovery

Status: in_progress since 2026-09-23. Dependencies: T011.

Requirements: FR-005, FR-020, NFR-005.

Register approved global actions with conflict feedback; add local single-user CLI IPC and settings/launcher recovery. Respect native event-loop ownership.

**Exit criterion:** Activation works while another app is focused; denied/conflicting bindings are visible; no-tray operation and EmergencyHide are demonstrated.

**Progress:** a Unix-domain control socket in `XDG_RUNTIME_DIR`, chmod 0600, with a closed six-verb command set, bounded reads, stale-socket detection, and cleanup on exit. `yappyink toggle-draw` and friends drive a running overlay from any process. The full Draw/PassThrough/Hidden/Draw cycle was verified from a separate process on 2026-09-23; see `docs/evidence/E005-control-channel.md`. The adapter also requests `xdg_activation_v1` when entering Draw, which is untested. **Still required:** the GlobalShortcuts portal, a user-bound desktop chord tried by hand, shortcut conflict feedback, and a non-terminal recovery path.

## T013 [M1]: Implement the independent compact toolbar

Status: implemented on 2026-09-23. Dependencies: T011.

Requirements: FR-006.

Wire toolbar actions through AppCommand. Distinguish canvas/toolbar hit testing and keep toolbar hidden in default PassThrough.

**Exit criterion:** Toolbar click/drag never creates a stroke; no click-through surface contains a falsely interactive control.

**Evidence:** `crates/ink-app/tests/toolbar.rs`, 13 headless tests. Layout and hit testing live in `ink-app::toolbar` because the contract is pointer arbitration, which is the controller's job; `architecture.md`'s proposed `ink-ui` crate is deferred until a settings panel gives that boundary real code. A press on the toolbar is checked before any tool sees it, so no tool can draw over its own controls, and the gaps between buttons belong to the toolbar rather than being holes to draw through. Buttons act on release over the same button, so a drag off one cancels it and a drag from one onto the canvas leaves no ink. The toolbar is shown only in Draw, where the surface accepts pointer input: in PassThrough it is hidden rather than left on screen looking clickable. Icons are drawn as unit-square paths, with no font stack. **Not observed on screen**, and the icons have no rendering tests.

## T014 [M1]: Implement transparent scene rendering

Status: implemented on 2026-09-23. Dependencies: T009, T011.

Requirements: FR-001, FR-007, NFR-001, NFR-002, NFR-003.

Add the selected compatible renderer, explicit alpha convention, scene caches, and demand-driven redraw.

**Exit criterion:** Blank background is genuinely transparent; unchanged scenes do not continuously redraw; highlighter edge behavior has a test fixture.

**Evidence:** `docs/evidence/E006-rendering-and-idle.md`. Transparency is pixel-tested. The event loop moved to `calloop` and blocks on its sources: measured 0 CPU ticks over 10 idle seconds with a surface mapped. The highlighter fixture found a real bug, a 50% highlighter rendering at alpha 255, fixed by compositing whole-object coverage once instead of blending sample by sample; separate strokes still accumulate, as FR-007 requires. 12 pixel tests. **Deferred with reasons:** the GPU renderer and geometry caching, because `architecture.md` warns against a second renderer or an optimisation before the first is profiled (T024). NFR-001 latency is unmeasured.

## T015 [M2]: Add pen and highlighter tools

Status: implemented on 2026-09-23. Dependencies: T010, T014.

Requirements: FR-007, FR-018.

Implement transient sampling, dot creation, preview, commit/cancel, width/color/opacity, and bounded point storage.

**Exit criterion:** One gesture creates one object; canceled gestures produce no history; highlighter self-overlap has no mesh seam.

**Evidence:** `crates/ink-app/tests/tools.rs`, 14 headless tests. Pen and highlighter each keep their own colour, width and opacity, stepped through bounded tables so width can never reach zero and opacity can never reach full transparency. The tool and style are captured when a gesture starts, so changing either mid-drag cannot rewrite what the user drew. Samples closer than 0.5 logical units are dropped and a stroke stops growing at `MAX_STROKE_POINTS`, while a press-and-release in one place still commits a dot. Self-overlap was fixed in T014 and has pixel fixtures. Keys `1`, `2`, `c`, `[`, `]`, `-`, `=` drive it, and the chrome shows the current colour and width. **Not observed on screen.**

## T016 [M2]: Add line, arrow, rectangle, and ellipse

Status: implemented on 2026-09-23. Dependencies: T015.

Requirements: FR-008.

Implement shared drag lifecycle, preview geometry, explicit degenerate-shape thresholds, and style reuse.

**Exit criterion:** Each completed shape creates one object; canceled/degenerate operations do not create invisible edits.

**Evidence:** `crates/ink-app/tests/tools.rs` and `crates/ink-render/tests/painting.rs`. The four shape tools share one drag lifecycle with the freehand tools and one style between them, since they are one pen held differently. A shape keeps only its endpoints, so the wandering middle of a drag is not stored, and the release point defines the geometry. A degenerate drag emits `GestureDiscarded` with the reason rather than committing an object nobody could see; a shape whose drag has not moved previews as nothing, matching what committing it would do. Arrowheads scale with stroke width, rectangles and ellipses are outlines, and drag direction does not change the result. **Not observed on screen.**

## T017 [M2]: Implement object erasing and command history

Status: implemented on 2026-09-23. Dependencies: T015, T016.

Requirements: FR-009, FR-010.

Add swept-geometry hit testing, grouped deletion, inverse commands, redo branching, clear, and history budgets.

**Exit criterion:** The drawing-history spec sequence passes headlessly with exact document restoration.

**Evidence:** `crates/ink-core/tests/history.rs::core_test_sequence` is the sequence from `specs/002-drawing-history/spec.md` written out step by step, and it passes. `ink-core` gains `Command`, `History` and `Session`: three commands that are each other's inverses, with `Clear` expressed as a removal of everything so an empty clear is naturally a no-op. Inverses carry whole objects and their positions, so undo restores order and style exactly. Eraser hit testing is segment-to-segment against the object's outline, so a fast sweep cannot skip between samples, and the tolerance includes half the object's width so a thick line is hit where it looks hit. History is bounded at 1,000 transactions and reports when it truncated. **Not observed on screen.** The 64 MiB byte budget from `product-spec.md` is not enforced; T024 owns measuring it.

## T018 [M2]: Complete selected-output and DPI behavior

Status: not_started. Dependencies: T011, T014.

Requirements: FR-011, FR-012.

Support output selection and output-local document switching. Normalize native coordinate scale and rotation once at the boundary.

**Exit criterion:** The same logical stroke aligns under tested scales; switching outputs preserves documents and does not assume a global Wayland cursor.

## T019 [M2]: Build capability/settings UX

Status: not_started. Dependencies: T012, T013.

Requirements: FR-013, FR-020, NFR-005.

Show effective support state, errors, shortcuts, tool preferences, and compatibility-mode labeling.

**Exit criterion:** Unavailable operations are clearly disabled/explained; unknown results do not become green supported badges.

## T020 [M2]: Implement explicit vector save

Status: implemented on 2026-09-23. Dependencies: T009, T017.

Requirements: FR-015, FR-017, NFR-003.

Serialize the versioned scene and apply bounded, failure-safe same-directory writes with platform-specific replacement.

**Exit criterion:** Save/reopen roundtrip works and disk/access/interruption faults preserve the previous valid file.

**Evidence:** `crates/ink-storage/`, 19 tests. The file format lives in its own `wire` types rather than deriving on the domain, so `ink-core` keeps its zero dependencies and the gap between the two is where validation happens. A save writes a temporary file in the destination directory, flushes and syncs it, then renames, so an interrupted save cannot destroy the previous file; failures clean up the temporary. Verified live against a running overlay. **Untested:** Windows replacement semantics, which `specs/003-local-storage` calls out specifically and which no backend exists to exercise.

## T021 [M2]: Implement validated load and output remapping

Status: implemented on 2026-09-23. Dependencies: T020.

Requirements: FR-015, FR-017.

Stage validation/migration before document replacement; handle missing output bindings explicitly.

**Exit criterion:** Malformed, oversized, nonfinite, or newer-schema inputs leave the current document unchanged.

**Evidence:** the failure half of `crates/ink-storage/tests/roundtrip.rs`. Size is checked against the file's metadata before any parsing, so a hostile file cannot make the parser allocate first. A newer schema gets its own error class, because that is the one failure where the file is probably fine and the application is out of date. Every shape goes through the domain's own constructors, so a file cannot smuggle in a degenerate rectangle or an empty stroke that the application would have refused to create. An unknown shape tag fails to parse rather than being skipped. The loaded document is built whole before the caller adopts it, so a refusal cannot leave a half-loaded state. **Partial:** an output mismatch is reported and the annotations keep their saved coordinates, but there is no remap UI; that needs T019's settings surface.

## T022 [M2]: Audit permissions, content logging, and network behavior

Status: not_started. Dependencies: T019, T020.

Requirements: FR-014, FR-016.

Run basic drawing with capture permissions denied; inspect logging/IPC/default settings and verify no unnecessary privilege or network dependency.

**Exit criterion:** Drawing remains usable offline without capture permission; diagnostics contain no annotation text, pixels, or raw key stream.

## T023 [M2]: Exercise cancellation and recoverable failures

Status: not_started. Dependencies: T011, T012, T015.

Requirements: FR-018, FR-019, NFR-005.

Inject mode failure, stale callbacks, surface loss, shortcut disconnect, and active-gesture cancellation.

**Exit criterion:** Interactive surfaces withdraw on recoverable failure and committed work remains intact; limitations of a fully hung process are documented.

## T024 [M2]: Measure responsiveness and resource use

Status: not_started. Dependencies: T014, T015, T017.

Requirements: NFR-001, NFR-002, NFR-003.

Run documented normal/stress scenes in release mode; record p50/p95/p99 submission delay, frame pacing, idle CPU, allocations, and memory.

**Exit criterion:** A reproducible report separates submission timing from photon latency and explains any missed proposed target.

## T025 [M2]: Run the MVP acceptance matrix

Status: not_started. Dependencies: T017, T018, T019, T021, T022, T023, T024.

Requirements: FR-001, FR-002, FR-003, FR-004, FR-005, FR-006, FR-007, FR-008, FR-009, FR-010, FR-011, FR-012, FR-013, FR-014, FR-015, FR-016, FR-017, FR-018, FR-019, FR-020, NFR-001, NFR-002, NFR-003, NFR-004, NFR-005, NFR-007.

Execute domain/contract/native scenarios for every promised environment. Report real failures, untested hardware, and support restrictions.

**Exit criterion:** All advertised MVP environments have passing evidence; GNOME cannot be silently omitted from a full-support claim.

## T026 [M3]: Add simultaneous outputs

Status: not_started. Dependencies: T025.

Requirements: FR-021, FR-012.

Create independent surfaces/documents per output and implement an explicit stroke boundary policy.

**Exit criterion:** Cross-output input cannot jump coordinates; mixed-DPI surfaces keep separate correct transforms.

## T027 [M3]: Handle output and desktop lifecycle

Status: not_started. Dependencies: T026.

Requirements: FR-022, FR-019.

Handle unplug/reconnect, scale/rotation changes, lock/unlock, suspend/resume, and workspace movement using native lifecycle events.

**Exit criterion:** No ghost input surfaces or silent document loss occur in the lifecycle test matrix.

## T028 [M3]: Add text entry and editing

Status: in_progress since 2026-09-24. Dependencies: T036.

Requirements: FR-023.

Implement explicit text focus, Unicode/IME preedit and commit/cancel, and history for text edits.

**Amended on 2026-09-23:** object selection, move, resize and delete were split out into T036 and are implemented. FR-023 bundled text with selection, and the two are separable: selection needs hit testing and transforms, text needs a focus policy and an input-method protocol. Splitting let the smaller half ship. FR-023 is not satisfied until both are done.

**Exit criterion:** IME tests and local shortcuts work without leaking keystrokes to the underlying app while editing.

**Progress on 2026-09-24, Latin only:** `Shape::Text` in the domain, glyph rasterisation through `fontdue` in `ink-render`, and an editor in the controller. Clicking with the text tool places a caret; while it is open a key is a character rather than a shortcut, which is FR-023's explicit focus made testable, and `is_editing_text()` is the question the adapter asks before deciding which. Escape discards, clicking elsewhere or picking another tool keeps what was typed, and empty or whitespace-only text commits nothing because FR-008 forbids an invisible object. Text moves and resizes through the existing selection machinery. 21 tests, including six against a real system font.

**Input methods, 2026-09-24:** `zwp_text_input_v3` is bound and confirmed present on the E001 machine. A composition is kept apart from the committed text, shown inline and underlined, and discarded rather than stored if the editor closes mid-word, because it is the engine's working state and not something the user chose. `delete_surrounding_text` removes whole characters rather than bytes, which matters precisely in the scripts that need an engine. The protocol's events are accumulated and applied on `done`, in the order the specification gives, since applying them as they arrive would show half-composed states. The caret rectangle is reported so the candidate window follows the text. 33 tests.

**Font fallback, 2026-09-24:** the first attempt to type Hindi produced nothing, and the reason was not the protocol. `fc-query` reports that DejaVuSans, first in the renderer's candidate list and the face this machine loads, has **zero Devanagari coverage**; `lookup_glyph_index` answered 0 and every glyph rasterised to an empty bitmap. The renderer now keeps fallback faces behind the primary one and picks, per character, the first face that has the glyph. Measurement uses the same face as rasterisation, or a fallback run would advance by the primary face's `.notdef` width and pile up. Five fallback faces load on the E001 machine, Devanagari among them, logged at startup. 35 tests.

**Remaining, and why the exit criterion is not met:** none of this has been watched working against a live input method, so there is still no evidence that composing Hindi or CJK produces correct text on screen. Fallback is per glyph and `fontdue` does no shaping, so scripts needing reordering or joining render as separate base glyphs in visual order — the characters are right and their arrangement is not. That is a shaping engine, and it is unwritten. There is also no caret movement within a run, no selection inside text, and no re-editing of a committed text object.

## T036 [M3]: Object selection, move, resize and delete

Status: implemented on 2026-09-23. Dependencies: T013, T017. Split from T028.

Requirements: FR-023, FR-010, NFR-004.

Selection picking, move and scale as inverse commands, corner handles, delete, and a live drag preview.

**Exit criterion:** Selecting, moving, resizing and deleting are each one undoable edit that restores geometry exactly, and a click that does not move records no history.

**Evidence:** `crates/ink-app/tests/selection.rs` (16 tests) and the transform tests in `crates/ink-core/tests/history.rs`. `Command::Replace` carries the objects as they were, so undoing a move or resize restores geometry exactly rather than applying an approximate reverse, and a moved object keeps its place in the paint order. A transform that would collapse a shape is refused whole, since FR-008 will not create an invisible object and a transform must not produce one either. A click that does not move is a selection, not an edit. **Not observed on screen.** Picking uses bounding boxes rather than exact geometry, there is no multi-select, and there is no rotation.

## T029 [M3]: Add annotation-only PNG/SVG export

Status: not_started. Dependencies: T014, T020.

Requirements: FR-024.

Render/serialize the vector document without capturing desktop pixels; preserve alpha and use safe escaped SVG output.

**Exit criterion:** Exports contain only ink, with stable dimensions/styles and no screenshot permission request.

## T030 [M3]: Validate accessible controls

Status: not_started. Dependencies: T013, T019, T028.

Requirements: NFR-006.

Add labels, keyboard navigation, visible focus, scalable controls, and platform accessibility integration for each certified UI path.

**Exit criterion:** A recorded keyboard/screen-reader walkthrough covers all toolbar/settings actions; unverified custom Wayland accessibility is stated.

## T031 [M3]: Package and harden distribution

Status: not_started. Dependencies: T025.

Requirements: FR-025, NFR-008.

Choose exact targets, build installers/packages, review dependency licenses/unsafe code, and perform platform signing/notarization checks where applicable.

**Exit criterion:** Clean-machine installation and uninstall preserve user documents and show truthful permissions/support notes.

**Ahead of this task, 2026-09-24:** a build and release pipeline exists at the owner's request, and the status above is unchanged because none of the work this task actually asks for has been done — no installers, no licence review, no signing or notarisation, and no clean-machine test. What exists is GitHub Actions running the full workspace on Linux and the portable crates on Windows and macOS, and a tag producing three binaries with release notes that state plainly that two of them cannot draw. Versioning also starts here, at 0.3.0; `docs/ship-it.md` holds the scheme. T025 is still the dependency and is still not started.

## T032 [M3]: Certify V1 support

Status: not_started. Dependencies: T027, T028, T029, T030, T031.

Requirements: FR-025, NFR-007.

Run the full release matrix on real machines, including fullscreen/Spaces cases promised by the product.

**Exit criterion:** Every published support statement maps to evidence and all limitations are visible in release notes.

## T033 [M4]: Specify and implement optional capture/freeze

Status: not_started. Dependencies: T032.

Requirements: FR-026, NFR-003.

Select native capture integrations, permission UI, frame lifetime, freeze input blocking, and cancellation behavior.

**Exit criterion:** Deny/revoke/cancel capture without breaking live drawing; freeze never controls stale underlying pixels.

## T034 [M4]: Implement screenshot-plus-ink export

Status: not_started. Dependencies: T033, T029.

Requirements: FR-027, FR-016.

Prevent self-capture/duplicate ink, exclude toolbar, composite once with correct transforms, and clear captured pixels by default.

**Exit criterion:** A known fixture is exported with exactly one ink layer and no app controls; no background image is silently retained.

## T035 [M4]: Add presenter enhancements one spec at a time

Status: not_started. Dependencies: T032.

Requirements: FR-028.

Write a focused spec for each chosen board/laser/spotlight/pressure feature before implementation. Separate passive global input monitoring from Draw-mode effects.

**Exit criterion:** Each new feature has its own capability/privacy contract and native tests; no universal pressure/global tracking claim is assumed.
