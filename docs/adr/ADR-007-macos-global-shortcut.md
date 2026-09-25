# ADR-007: A global shortcut on macOS through Carbon

Status: accepted, 2026-09-25. Adds no dependency: it links the system Carbon
framework and declares five of its functions by hand. No requirement or
acceptance criterion changes.

## Context

ADR-006 left FR-005 open on macOS and said it needed its own decision. The
consequence of leaving it open was worse than on either other platform. In
PassThrough the overlay sets `ignoresMouseEvents`, so it receives no input at
all, and without a global chord **the only way back was the terminal that
launched it**. Windows has `RegisterHotKey`; Wayland has the control socket.
macOS had nothing.

There are two ways to see a key press while another application is in front:

| Route | Permission | What it can see |
|---|---|---|
| Carbon `RegisterEventHotKey` | none | only the chords it registered |
| `CGEventTap` or `NSEvent` global monitor | Accessibility (Input Monitoring) grant, with a system prompt | every key the user types, in every application |

## Decision

**Use `RegisterEventHotKey`, for Control+Option+D (draw or pass through) and
Control+Option+H (hide).**

It asks for no permission because it cannot observe anything but the chords it
asked for, and that is the right amount of power for an annotation overlay.
Asking a user to let this program read every key they type, so that it can
notice two of them, would be a bad trade, and on a managed Mac the grant may
not be available at all.

Carbon is deprecated as a whole, but this part of it has no replacement and is
still what menu-bar utilities use for exactly this. Apple has changed it once,
in macOS 15: a hot key whose only modifiers are Option, or Option and Shift, is
refused, because those combinations type characters on many layouts. Both
chords include Control, and a test in `chords` fails if one ever does not.

No crate is added. The functions are declared in `hotkey.rs` from
`CarbonEvents.h`, and every constant they take lives in `chords.rs`, which has
no platform types and is tested on the development machine. A wrong four-
character code or modifier mask does not fail; it silently registers a
different chord or none, which is why those numbers are pinned by tests against
Apple's documented hexadecimal values rather than trusted.

## Rejected

- **`CGEventTap` or `addGlobalMonitorForEventsMatchingMask:`.** The permission
  above. It also cannot swallow the chord, so the application in front would
  receive it too.
- **A Unix-domain socket, as on Linux.** Useful and not exclusive with this. It
  would reuse `control.rs`, but that resolves its path from `XDG_RUNTIME_DIR`,
  which macOS does not set, and it only helps a user who has bound a command to
  a chord somewhere else. Left for later.

## Consequences

- The overlay registers both chords at startup and prints whether each was
  accepted. A refusal (another application owns the chord) is reported and not
  fatal.
- The Carbon event handler runs on the main thread's run loop, the same thread
  as AppKit, so it reaches the overlay's thread-local state the same way a key
  press does. Nothing is made `Send`.
- **It has never been run.** Nobody on this project has a Mac. `clippy` checks
  it against `aarch64-apple-darwin`, and CI links it on a macOS runner, which
  at least proves the five symbols resolve. Whether a chord fires is unknown,
  and `doctor` says so.

## What would reverse this

Evidence that `RegisterEventHotKey` does not deliver to an `Accessory`
application, or that a future macOS removes it. The fallback is the event tap
with its permission prompt, and that would need this ADR revisited, not
quietly replaced.
