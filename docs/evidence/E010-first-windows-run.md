# E010: the first Windows run

**Observed:** 2026-09-25, reported to the owner with a screenshot, by a user
running the `yappyink-x86_64-windows.exe` 0.7.0 release binary.
**Machine:** Windows, two monitors: 1366×768 primary at (0, 0) and 1920×1080
at (−290, −1080), both 96 dpi.
**Tasks:** T003. **Requirements:** FR-001, FR-002, FR-007, FR-017, FR-023.
**Status:** first-hand screenshot. Two capabilities move to `available`; the rest
stay `unknown`.

## What the screenshot shows

The overlay covering monitor 1, over a terminal window. On it:

- the toolbar across the top, with the pen selected and the colour swatch
  showing magenta;
- a freehand pen stroke, a rectangle, a translucent brown highlighter stroke,
  an arrow, and the word "Hello" drawn with the text tool;
- the terminal text visible through every part of the overlay that has no ink.

## What the log shows

```
[output] monitor 1: 1366x768 at 0,0, 96 dpi, primary
[output] monitor 2: 1920x1080 at -290,-1080, 96 dpi
[output] covering monitor 1: 1366x768 at 0,0
[dpi] 96 dpi, scale 1
[font] C:\WINDOWS\Fonts\segoeui.ttf
[font] fallback C:\WINDOWS\Fonts\msyh.ttc
[hotkey] Ctrl+Alt+D registered
[hotkey] Ctrl+Alt+H registered
[mode] Draw
[window] placed
```

Above the new run, an earlier session's lines:

```
[save] the session path is not valid: neither XDG_DATA_HOME nor HOME is set, so there is nowhere to put it
[exit] 4 object(s) were on screen.
[exit] Nothing was written, and there is nowhere to write: ...
```

## What that establishes

- **Live overlay (FR-001): yes, on this machine.** Ink painted through
  `UpdateLayeredWindow` stays above another application with per-pixel
  transparency everywhere else.
- **Draw captures clicks on empty canvas (FR-002): yes.** Every shape sits over
  an area that was fully transparent before it was drawn, and each one was
  drawn rather than the click reaching the terminal. That is the alpha-1 floor
  added in 0.7.0; without it, per Microsoft's documentation, these clicks would
  have gone through.
- **The overlay starts in Draw.** The 0.7.0 fix for starting Hidden works.
- **Monitor enumeration** reports both monitors, including one at negative
  coordinates, and the primary is chosen.
- **Fonts on Windows** are found, main face and CJK fallback.
- **Every tool shown works**: pen, highlighter, rectangle, arrow, text.
- **The grip moves the window** (`[window] placed`).
- **Save was broken.** The session path knew only `XDG_DATA_HOME` and `HOME`,
  and Windows sets neither. Fixed in 0.7.1: `%APPDATA%\yappyink\session.json`.

## Not observed

Pass-through, and whether `Ctrl+Alt+D` and `Ctrl+Alt+H` fire (only their
registration is logged). `yappyink toggle-draw` from a second terminal.
`--monitor 2`. Parked. Resizing from the corner. Input-method composition.
Any DPI other than 96. Fullscreen applications and virtual desktops. Save and
load after the fix. **Those capabilities stay `unknown`.**
