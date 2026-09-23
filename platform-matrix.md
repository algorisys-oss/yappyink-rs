# Platform capability and certification matrix

**Status of this app on every row: NOT IMPLEMENTED / NOT TESTED.** The implementation routes below are proposals informed by platform documentation, not verified application support.

## Backend plan

| Environment | Proposed native route | Main proof obligations |
|---|---|---|
| Windows desktop | winit + explicit Win32 adapter where needed; transparent composition; controlled stacking, focus, and hit testing | Empty transparent canvas captures Draw gestures; PassThrough reaches other processes; no unintended activation; high DPI; borderless fullscreen |
| macOS desktop | winit where sufficient, supplemented/replaced by AppKit window or panel adapter through Rust bindings | Mouse transparency; text-entry focus; app deactivation; multi-display; Spaces and fullscreen behavior |
| Linux X11 | winit/X11 with ARGB/compositing and explicit EWMH/input-shape adapter | Compositor availability; WM stacking; input regions; no keyboard grab in PassThrough; different WMs |
| Wayland with advertised layer-shell | wayland-client + smithay-client-toolkit; dedicated layer surfaces and input regions | Correct surface role; empty input region for PassThrough; keyboard interactivity; output scale and configure lifecycle; shortcut availability |
| GNOME Wayland | Dedicated feasibility branch: xdg-shell compatibility experiment, then a versioned GNOME companion if necessary | Full visible pass-through + stable above-app placement is not assumed; companion installation/lifecycle and Shell-version compatibility must be proven |

Microsoft documents that alpha-zero areas of layered windows can pass pointer events through, and that layered-window WS_EX_TRANSPARENT changes hit testing. Therefore, simply removing a pass-through flag is not sufficient evidence that an empty Draw canvas captures input. Test the chosen composition path; do not fix it by silently tinting the entire desktop. [S05]

Apple provides mouse-event transparency and collection behaviors, but a particular flag combination is not proof that every fullscreen Space works. Explicitly test the targeted OS versions. [S06, S07]

winit exposes general hit-test APIs, but its generic WindowLevel abstraction does not implement Wayland stacking. Using winit for other platforms does not eliminate the separate Wayland backend. [S03, S04]

Layer-shell controls surface stacking and keyboard policy. A zero-sized/empty input region means the surface does not take pointer input; keyboard interactivity must be configured separately. Detection is based on advertised protocols, not a desktop-name string. [S08, S12]

## GNOME release decision

First test stock GNOME Wayland on the actual Ubuntu development machine and record its versions. Run the same contract scenarios as every other backend. Do not make “switch to X11” the acceptance criterion for GNOME.

If the standalone route cannot meet the contract, write ADR-002 with one of two explicit outcomes: (a) ship a minimal GNOME Shell companion and certify specific Shell versions; or (b) ship a clearly limited compatibility preview while leaving full GNOME support blocked. The Rust engine remains the main application. A companion may be JavaScript/GJS and creates a separate distribution/maintenance surface. [S09, S20]

A companion architecture is itself a spike. Determine whether it manages app windows, owns overlay actors, or needs a small display/input bridge. Validate that ink can remain visible as keyboard and pointer control return to another app. Do not assume a Shell extension can be bolted onto the Rust renderer with zero changes.

## Shortcut routes

Windows, macOS, and X11 can evaluate the global-hotkey crate with the documented event-loop constraints. Wayland evaluates the GlobalShortcuts portal at runtime. If unavailable, provide a command such as `screenink toggle-draw` that users bind through their compositor/desktop settings. Display registration conflicts and permission denial. [S10, S11]

The actual app ID, desktop file, portal backend, session lifetime, and bindings approved by the user are part of the evidence. Do not parse a guessed desktop name and assume the portal works.

## Capability vocabulary

Capabilities include live_overlay, draw_pointer_capture, visible_passthrough, keyboard_release, global_shortcut, output_enumeration, simultaneous_outputs, fullscreen_overlay, workspace_overlay, capture, and capture_exclusion.

Each runtime entry records state (unknown/available/needs_user_action/unavailable), reason, backend, and scope. Version-specific test evidence belongs in a separate certification record. An API call returning success is not itself certification.

## Initial certification candidates

These are test candidates, not fixed minimum OS versions: the user's actual Ubuntu installation on its native display server; another GNOME Wayland release; a KDE Plasma Wayland environment; one wlroots-based compositor; a composited X11 desktop; Windows 11 x64; Apple Silicon macOS. Add Intel macOS and other CPU targets before advertising those builds. Record exact versions during the spike instead of asserting a guessed latest release.

For each row, test one monitor first, then mixed-DPI displays, hotplug, virtual desktops/Spaces, a fullscreen browser, a code editor, and a terminal. Keep secure/exclusive/protected surfaces outside the contract.

## Evidence record template

```text
Environment ID:
App commit and build profile:
Date and tester:
OS / architecture:
Desktop / compositor / protocol versions:
GPU / driver / output layout / scaling:
Native or nested session:
Backend and capture/shortcut permission state:
Scenario IDs executed:
Expected versus observed behavior:
Evidence artifact locations:
Known restrictions:
Result: pass / fail / blocked / not tested
```

Nested compositors and CI jobs are useful but do not replace the real desktop acceptance run.
