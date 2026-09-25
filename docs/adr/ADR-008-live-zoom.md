# ADR-008: live zoom through the compositor's own magnifier

Status: accepted, 2026-09-25, on the evidence of E017 for GNOME; Windows and macOS still to be probed. Adds FR-029. No dependency is added by this ADR;
each platform's task decides its mechanism against the constraints below.

## Context

A user asked for ZoomIt-style zoom. The owner chose **live zoom**: the
magnified screen keeps updating (video, typing) and follows the pointer, as
opposed to freezing a still image and drawing on it. The order is GNOME on the
owner's machine first, then Windows, then macOS.

ZoomIt reads the screen. yappyink is built never to (ADR-003, FR-014): basic
drawing needs no capture permission and never holds desktop pixels, and
AGENTS.md forbids adding capture without a requirement and an ADR. So the
question is whether live zoom can be had without capture at all.

It can on two of three platforms, because the compositor already has a
magnifier and will do the work itself:

| Platform | Mechanism | Pixels reach yappyink? | Permission |
|---|---|---|---|
| GNOME (Mutter) | Shell's built-in magnifier, driven through `org.gnome.desktop.a11y.applications screen-magnifier-enabled` and `org.gnome.desktop.a11y.magnifier mag-factor` | no | none |
| Windows | Magnification API, `MagSetFullscreenTransform` | no | none known; to be probed |
| macOS | no public API drives the system's Zoom | only with ScreenCaptureKit | Screen Recording |

Probed read-only on the E001 machine, 2026-09-25 (E016): GNOME Shell 46 has
all the magnifier settings; the `org.gnome.Magnifier` D-Bus service is **not**
available, so settings are the route.

## Decision

**Live zoom is the platform compositor's magnifier, switched on and set by
yappyink, wherever one exists.** yappyink never receives magnified pixels on
GNOME or Windows, so FR-014 and ADR-003 hold unchanged there.

Consequences that follow from using the compositor:

- **Ink zooms with the content.** The overlay is an ordinary window on screen,
  so the magnifier magnifies it too, and ink stays on what it was drawn over.
  That is the behaviour a presenter wants and it costs nothing.
- **Input mapping is the compositor's job.** Whether drawing while zoomed puts
  ink under the magnified pointer is a native question for each probe; it must
  not be emulated by transforming input ourselves (AGENTS.md: no forwarding or
  injection).
- **These are the user's own accessibility settings.** yappyink records their
  values before changing them and restores them when zoom is turned off and on
  exit. If it is killed while zoomed the magnifier stays on; GNOME's own
  shortcut (Alt+Super+8) turns it off, and the startup banner says so.

### GNOME specifics (T037)

Writing the two settings needs either the `gsettings` command, a D-Bus client
(the dependency still undecided for the GlobalShortcuts portal and the file
picker), or GIO bindings. **The probe uses the `gsettings` command**: it adds no
dependency and is present wherever GNOME is. It runs off the GUI thread
(NFR-003). If the D-Bus dependency is chosen for the portals, zoom moves to it
in the same change.

GNOME already has this feature on Alt+Super+8, off by default and little known.
What yappyink adds is a toolbar button and chord next to the pen, zoom in and
out in steps while drawing, and restoring the user's settings afterwards. The
spec says so rather than presenting the magnifier as ours.

### Windows specifics (T038)

`MagInitialize` then `MagSetFullscreenTransform(level, x, y)`, from the
`Win32_UI_Magnification` feature of the `windows-sys` already in the workspace
(a feature flag, not a new crate). The full-screen transform does not follow
the pointer by itself, so the offset is updated as the pointer moves. Open
questions for the probe: whether any of this requires UIAccess, and how mouse
input maps while magnified (`MagSetInputTransform`, for pen and touch, is known
to need UIAccess).

### macOS (T039)

No public API drives the system Zoom. The choices are ScreenCaptureKit, which
means Screen Recording permission, pixels in our process and the full FR-026
capture contract, or deferring to macOS's own Zoom (Accessibility settings)
and documenting it. **Not decided here.** T039 must amend this ADR before any
capture code is written.

**Amended 2026-09-25: capture, by the owner's decision.** Offered both, the
owner chose a ScreenCaptureKit magnifier over deferring to the system Zoom.
What that commits to, and what it costs:

- **Mechanism.** `SCStream` captures a rectangle of the main display around
  the pointer, scaled by ScreenCaptureKit to the display's full pixel size,
  and each frame's IOSurface is set as the contents of a layer in a
  borderless, click-through window one level below the overlay. A 30 Hz
  timer moves the rectangle with the pointer.
- **Permission.** macOS asks for Screen Recording the first time. Refusing it
  leaves the overlay working and zoom off, with a message saying where to
  grant it (NFR-005: a typed refusal, not a silent failure).
- **Pixels.** They exist only in the IOSurfaces ScreenCaptureKit recycles and
  in the layer showing them. Nothing is copied into our memory, stored,
  written, logged or sent (FR-027's "sensitive and ephemeral by default").
  Both yappyink windows are excluded from the capture.
- **Two limits the compositor-based platforms do not have.** The ink is not
  magnified, because the overlay is excluded to avoid capturing itself. And in
  pass-through, clicks go to the real positions, not the magnified ones;
  mapping them would be input injection, which AGENTS.md forbids. Making the
  ink zoom with the picture needs a view transform in the macOS adapter and is
  left for a later change.
- **Dependencies.** `objc2-screen-capture-kit` and `objc2-core-media`, both
  0.3.2 from the same objc2 release as the pinned `objc2-app-kit`, and
  `objc2-core-video`, `objc2-io-surface`, `objc2-quartz-core`, `block2` and
  `dispatch2`, which were already in the lockfile through the existing macOS
  stack. Target-gated to macOS. `objc2-av-foundation` arrives transitively.
- **One `Send` assertion.** The stream is built in a ScreenCaptureKit
  completion handler on a queue it chooses and is used from the main thread,
  so it is kept in one mutex with `unsafe impl Send` on its holder. Swift
  declares these classes `Sendable`; the Rust bindings do not yet. No AppKit
  object crosses threads: the window and layer stay on the main thread, and
  frames reach them through the main queue.
- **Never run.** Nobody on this project has a Mac. It type-checks against
  `aarch64-apple-darwin` and CI links it; the owner's Mac tester is the first
  run.

## Rejected

- **Capturing the screen on every platform, ZoomIt-style.** Uniform, but it
  puts desktop pixels in our process and a permission prompt in front of a
  feature the compositor can do for free on two platforms.
- **Transforming pointer input ourselves** so drawing lands in the right place
  while zoomed. That is input emulation, which AGENTS.md forbids.

## What would reverse this

A probe showing that drawing under the compositor's magnifier puts ink in the
wrong place with no platform fix, or that changing the user's accessibility
settings cannot be made safe to restore.
