# Quality, privacy, and release gates

## Evidence levels

Specification: behavior is written down. Unit test: domain logic is checked headlessly. Contract test: fake/native adapter behavior is checked against the event contract. Native acceptance: an actual desktop demonstrates stacking, hit testing, focus, scaling, and recovery. Release certification: packaged builds pass on each published environment.

These levels are cumulative, not substitutes for each other. No native app behavior is verified by the checks in this documentation kit.

## Core checks

At implementation time, run formatting, clippy, unit tests, property tests, and the selected platform build matrix. Commands depend on the real workspace and targets; do not claim `cargo test --all-features` is meaningful if platform-only features conflict.

Command/history tests must cover exact inverse behavior, no-op history, redo branching, grouped erase, point limits, and canceled previews. Reducer tests must cover stale callbacks, transition rollback, button handoff, EmergencyHide, and capability changes.

## Native manual gate

For every published environment, execute the underlay fixture and actual editor/browser/terminal cases. Check blank transparent-area drawing, pass-through click counts, text/wheel delivery, local undo ownership, a missing tray, a blocked shortcut, multiple applications, and fullscreen only when promised.

Record native versus nested/composited tests distinctly. Headless CI cannot certify actual GNOME/Windows/macOS overlay behavior by compiling successfully.

For the teaching/screen-sharing use case, also test the chosen recorder or meeting app with display capture and application-window capture as separate cases. Verify whether the exported/shared output actually contains the ink and whether it includes app controls. Local on-screen visibility is not sufficient evidence of recording visibility; publish only the sharing modes actually verified.

## Rendering and performance

Use fixed vector fixtures and a documented pixel tolerance/color space for renderer checks. Include alpha fringes, highlighter self-overlap, scale, rotation, and narrow strokes. Measure event-to-submit timing independently of actual display latency. Use release builds and recorded hardware. Do not derive claims from debug-mode impressions.

Define resource limits and exercise them. A two-hour manual drawing/undo/erase session or equivalent replay should expose cache/history growth. This is a test-duration proposal, not an estimate for project delivery.

## Permissions, IPC, and data handling

Basic drawing is tested with capture permissions denied. No root/raw-device dependency. Capture permission is requested only by a later explicit capture action. Verify that failure does not disable the live overlay.

The single-instance CLI channel is local to the current user/session. Use a protected Unix-domain socket or appropriately secured named pipe rather than an unauthenticated network listener. Bound and validate commands. Never expose arbitrary shell execution through the control API.

Logs may contain backend names, error categories, and redacted diagnostic metadata. They must not contain desktop pixels, annotation text, paths unnecessarily revealing user details, or raw keystrokes. A GNOME helper, if chosen, follows the same policy and has an explicit compatibility/lifecycle contract.

## Packaging

Select exact minimum OS/architecture targets from actual build and test evidence. Verify installation, startup Hidden, launcher recovery, shortcut setup, updates, and uninstall. Preserve user documents on uninstall unless the user explicitly chooses removal. Perform signing/notarization/package-integrity checks appropriate to the selected distribution route; no store acceptance is presumed.

## Definition of done for a feature

The spec is current; referenced tests are implemented and run; native evidence exists where required; performance/privacy regressions are checked; user-visible limitations are documented; an ADR records changed assumptions; and the task/traceability status reflects what was actually verified.

## Whole-product release rule

Every advertised capability must have an environment-specific evidence record. All three OS families can be a product target without already being proven support. Full GNOME parity remains blocked until its route passes the same live/Draw/PassThrough contract.
