# E011: the macOS overlay drawing

**Observed:** 2026-09-25, reported to the owner with a screenshot, by the user
from E009, running the `yappyink-aarch64-macos` 0.7.x release binary on an
Apple Silicon Mac.
**Screen:** 1440×900 points, backing scale 2 (Retina).
**Tasks:** T004. **Requirements:** FR-001, FR-002, FR-007, FR-017.
**Status:** first-hand screenshot. Two capabilities move to `available`; the rest
stay `unknown`.

## What the screenshot shows

The overlay over a Finder window and the desktop, with:

- the toolbar across the top, the ellipse tool selected, and the colour swatch
  row open beneath it;
- a rectangle, an ellipse and an arrow in magenta, drawn partly over the Finder
  window and partly over the desktop;
- Finder and the desktop visible through every part of the overlay without ink.

## What the log shows

```
[output] screen 1440x900 points, backing scale 2
[font] /System/Library/Fonts/Supplemental/Arial...
[font] fallback /System/Library/Fonts/Supplemen...
[hotkey] Control+Option+D registered
[hotkey] Control+Option+H registered
[mode] Draw
```

(The font lines are cut off by the window edge in the screenshot.) Above them,
an earlier run's exit line names an existing session file under the user's
home directory. The path is not reproduced here.

## What that establishes

- **Live overlay (FR-001): yes, on this machine.** A borderless window at
  screen-saver level with a clear background composites above other
  applications, and the image blitted through `CGImage` is the right way up
  and at Retina resolution.
- **Draw captures clicks on empty canvas (FR-002): yes.** The shapes were
  drawn over Finder and the desktop without the clicks reaching them.
- **The E009 fixes hold:** the overlay starts in Draw, it is not "frozen", the
  system fonts are found, and both Carbon chords register.
- **The toolbar, the swatch picker and the shape tools work.**

## Not observed

Pass-through, and whether either chord fires (only registration is logged).
Text entry, though a font is now found. Keyboard focus. Parked, hide, quit.
Spaces and fullscreen applications, the most doubtful questions for this
backend. Save and load, beyond one line naming a file an earlier run wrote.
**Those stay `unknown`.**
