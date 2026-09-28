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

**The overlay can change monitor when it is shown again.** The owner saw
Shrink to toolbar and back move yappyink to the other monitor. The extension's
log shows the same thing after the agent's hide and show: at 15:51 it sized
the overlay to monitor 1 (1920x1080+1366+0), and at 15:56, after the owner's
shrink and restore, to monitor 0 (1366x768+0+193). The extension fits the
window to `window.get_monitor()` each time it is shown, and does not remember
the monitor it was on before. A Wayland client cannot choose where it goes,
so the fix belongs in the extension. This did not come from this change: the
toolbar colours do not touch the window's placement.
