# E005: the local control channel

Task: T012 (partial). Requirements: FR-005, FR-020, NFR-005.
Scenario: AC-FR-005 (partial).

**Status: verified end to end on 2026-09-23** for the socket route. The
GlobalShortcuts portal is not implemented, and a user-bound desktop chord has
not been tried by hand. Both are marked below.

## Why this exists

E004 recorded the problem as it appeared in use: once PassThrough works
properly, clicking the application underneath takes keyboard focus with it, so
no shortcut inside our own surface can reach us. Hidden is starker still, as
there is no surface at all.

FR-005 asks for activation and escape routes. On Wayland an ordinary
application cannot register a global shortcut for itself. `platform-matrix.md`
names the two routes: the GlobalShortcuts portal, or a command the user binds
through their own desktop settings. This is the second one.

## What was built

A running overlay listens on a Unix-domain socket. A second invocation of the
binary connects and sends one verb.

```sh
yappyink draw           # runs the overlay and the listener
yappyink toggle-draw    # from anywhere, while anything has focus
yappyink pass-through
yappyink hide
yappyink emergency-hide
yappyink quit
```

The verb set is a closed enum with no arguments. Nothing read from the socket
reaches a process, a path, or a document.

## Verification, 2026-09-23

```sh
./target/debug/yappyink draw &          # in one shell
ls -l $XDG_RUNTIME_DIR/yappyink.sock
./target/debug/yappyink toggle-draw
./target/debug/yappyink rm-rf
./target/debug/yappyink hide
./target/debug/yappyink toggle-draw
./target/debug/yappyink quit
```

Observed:

| Check | Result |
|---|---|
| Socket permissions | `srw-------`, owner only, inside `XDG_RUNTIME_DIR` |
| `toggle-draw` from a separate process | exit 0; server logged `[mode] pass-through` |
| `hide` | exit 0; server logged `hidden. 0 object(s) kept in memory` |
| `toggle-draw` after hiding | exit 0; server logged `[mode] draw`, surface recreated |
| `quit` | exit 0; server exited cleanly |
| Unknown verb `rm-rf` | refused, exit 2, usage printed, never sent to the socket |
| Socket after exit | removed |

The whole Draw / PassThrough / Hidden / Draw cycle was driven from another
process, which is the property that matters: it does not depend on the overlay
having focus, and it works when the overlay has no surface at all.

## Security posture

Per `quality-gates.md`, "Use a protected Unix-domain socket ... Bound and
validate commands. Never expose arbitrary shell execution through the control
API."

- The socket is in `XDG_RUNTIME_DIR`, created per user at mode 0700, and is
  additionally chmod 0600. Verified above.
- If `XDG_RUNTIME_DIR` is unset the channel is refused rather than falling back
  to a world-writable directory. Covered by a test.
- Reads are bounded to 64 bytes per line (NFR-003).
- The verb set is closed and takes no arguments. Unknown input is refused
  without echoing it back.
- A stale socket from a crashed run is probed before removal: if something
  answers, the new instance refuses to start rather than stealing the channel.
- If the channel cannot start, the overlay still runs and says what was lost.
  A degraded capability is visible rather than silently absent (NFR-005).

## Also added: raise on entering Draw

`xdg_activation_v1` is advertised on this machine (E001), and the adapter now
requests activation when a mode change to Draw is applied.

This is deliberately limited to an explicit request to Draw.
`ux-state-machine.md` forbids the ink surface *reactivating itself during
PassThrough*, because that would steal focus back from the application the user
just clicked. Raising because the user asked to draw is the opposite.

**Untested.** Mutter is entitled to refuse self-activation without a recent
input serial, and we pass none. If it refuses, nothing breaks and the user still
applies "Always on Top" by hand as in E002. Whether it helps at all is an open
observation.

## Not done, and still required by FR-005

- **The GlobalShortcuts portal** is not implemented. No D-Bus client dependency
  has been chosen. `yappyink doctor` continues to report the capability as
  `unknown` with that reason.
- **A user-bound desktop chord** has not been tried. The command is designed for
  it, but binding `yappyink toggle-draw` in GNOME's keyboard settings and
  pressing it while another application is focused has not been observed.
- **Shortcut conflict feedback** (FR-005, AC-FR-005) does not exist, because
  nothing registers a shortcut yet.
- **A launcher or settings window** for recovery without a terminal does not
  exist.
- **EmergencyHide** is reachable over the socket but has not been exercised on
  screen with ink present.
