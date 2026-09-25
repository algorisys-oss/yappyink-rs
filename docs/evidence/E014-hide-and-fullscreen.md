# E014: hide and show, and fullscreen, on Windows and macOS

**Observed:** 2026-09-25, reported by the owner in words, on the Windows machine
from E010 and the Mac from E011, running the 0.7.x release binaries.
**Tasks:** T003, T004. **Requirements:** FR-004, FR-001, FR-002.
**Status:** a verbal report, confirmed for both platforms when asked. One
capability moves to `available` on each backend, and T003's exit criterion is
met.

## What was reported

> When YappyInk is running: press it once — drawings/ink disappear. Press it
> again — drawings/ink come back. Full screen also works — I tried fullscreen
> and I can access it.

Asked which platforms: **both**. Asked what "access it" meant: after an
application went fullscreen, the overlay's mode could be toggled and ink drawn
over it.

## What that establishes

- **Hide and show (FR-004), on both.** `Ctrl+Alt+H` on Windows and
  `Control+Option+H` on macOS withdraw the ink and bring it back unchanged, so
  the document survives Hidden as the requirement says. On Windows that is
  `ShowWindow(SW_HIDE)`; on macOS, `orderOut:`.
- **Over a fullscreen application, on both.** The overlay stayed reachable and
  drawable. On macOS that answers the question ADR-006 marked as most likely to
  fail: a screen-saver-level window with `FullScreenAuxiliary` does appear over
  a fullscreen application.

## Not established

- **Which fullscreen.** The applications are not named. On Windows a browser's
  F11 or a borderless-windowed game is an ordinary top-level window; an
  exclusive-fullscreen DirectX game is a different case, and is the one this
  report cannot speak for. On macOS, native fullscreen puts the application in
  its own Space, so this report suggests the overlay joined it, but Spaces were
  not tested on their own.
- **Spaces on macOS** (switching desktops with the overlay running). This was
  the remaining item in T004's exit criterion; the owner closed T004 with it
  deferred, so it is still untested.
- Emergency hide mid-gesture (FR-018), which is a separate path from the chord.
- Virtual desktops on Windows.
