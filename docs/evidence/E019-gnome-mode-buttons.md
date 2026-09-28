# E019: the mode shown on the toolbar, on GNOME

**Observed:** 2026-09-28, on the E001 machine (Ubuntu 24.04, GNOME Shell 46,
Wayland, the yappyink extension enabled, a laptop panel and an HDMI monitor),
running a release build of this change (`cargo build --release -p yappyink`).
**Tasks:** T040, T041. **Requirements:** FR-030, NFR-006. **Scenario:**
AC-FR-030.
**Status:** GNOME seen. Windows and macOS not run.

## What was done, and by whom

Screenshots were taken with `gnome-screenshot` after each switch and cropped
to the toolbar. The full frames showed the owner's desktop during a live
stream and are not kept in the repository.

The agent drove the routes that need no input on the desktop, through the
control socket:

| Route | Mode logged | Toolbar seen |
|---|---|---|
| start (`yappyink draw`) | draw | neither mode button coloured; pen highlighted |
| `yappyink pass-through` | pass-through | Pass through button amber, outlined |
| `yappyink hide` | hidden | no toolbar |
| `yappyink hide` again | pass-through | Pass through button amber, outlined |
| `yappyink toggle-draw` | draw | amber gone |

The owner then did the routes that need a click or a key: the Pass through
button, clicked in pass-through (the toolbar is clickable there on GNOME), and
Shrink to toolbar and back. Reported: "Its working on my machine." Parked was
not screenshotted, and the individual checks were reported as a whole.

## What that establishes

- The shared painter's output reaches the screen on GNOME, and the Wayland
  adapter repaints the toolbar on each of these switches.
- Neither mode button ever showed the previous mode after a switch.

## Not tested

- Windows and macOS: the owner runs these offline (T041).
- The `p`, `g`, `d` and `Esc` keys by themselves on GNOME. The keymap routes
  are covered headlessly in `crates/ink-ui/tests/mode_buttons.rs`, not
  natively.
- Whether the colours read well on a light desktop. Both runs here were over
  dark windows.

## Found along the way, not fixed here

**The owner saw Shrink to toolbar and back move yappyink to the other
monitor.** The cause is not yet known. What is known:

- **Hide and show can change monitor, and that one is explained.** Hiding
  destroys the Wayland toplevel and showing creates a new one; the extension
  adopts each new window and fits it to `window.get_monitor()`, whatever
  monitor Mutter chose for it. After the agent's hide and show at 15:51 the
  extension sized the overlay to monitor 1 (1920x1080+1366+0).
- **Shrink and restore does not create a new window,** so the extension is
  not involved. An earlier version of this record said it was, reading a
  15:56 "took charge" line as the owner's shrink. The agent's overlay never
  logged a shrink at all: the owner was using another yappyink, and 15:56 was
  that window being created.
- **Not reproduced by command.** With `yappyink park` (added for this),
  seven shrink-and-restore cycles from Draw and from pass-through left the
  toolbar at exactly the same screen position every time. What differs in the
  owner's run (the button or `g` rather than the socket, where the pointer
  was, which build) is the open question.
- **The extension's fit did not take on one launch.** At 16:05 it logged
  "sized to monitor 0 at 1366x768+0+193" (the laptop panel), yet screenshots
  put the window's top-left at about (1366, 193), on the HDMI monitor at the
  panel's height. The window was 1366x768 as asked; its position was not.
