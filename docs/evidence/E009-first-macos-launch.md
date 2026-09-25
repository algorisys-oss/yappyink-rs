# E009: the first launch of the macOS build

**Observed:** reported to the owner on 2026-09-25, by a user on an Apple Silicon
Mac. Not observed by anyone on this project.
**Build:** the `yappyink-aarch64-macos` release binary. The report says 0.5;
the macOS backend first shipped in 0.6.0, and the startup text quoted below is
0.6.0's, so it was the 0.6.0 binary.
**Tasks:** T004. **Requirements:** FR-001, FR-002, FR-003, FR-005, FR-023.
**Status:** partial and second-hand. It moves no capability off `unknown`.

## Where this came from

The report reached the owner as text written by another AI assistant, which was
reading the user's screenshots. The screenshots were not shared with this
project. What follows separates what the quoted terminal output shows from what
the assistant concluded. Only the first counts as evidence.

## What the terminal output shows

- `uname -m` printed `arm64`, so the Apple Silicon binary was the right one.
- `yappyink doctor` reported no display backend. **That was a bug in doctor**,
  not a fact about the Mac: session detection looks only for Linux display
  servers, and the native backend was reported only from the Wayland branch, so
  on macOS it was never mentioned.
- `yappyink draw` found the screen: `[output] screen 1440x900 points, backing
  scale 2`. This is the first time any line of the macOS backend is known to
  have run.
- `[font] no usable font was found, so the text tool is unavailable`. The only
  macOS font path in the renderer was `/Library/Fonts/Arial.ttf`, which a stock
  Mac does not have.
- The startup banner printed, including the warning that pass-through has no
  way back except the terminal.

## What the user experienced

The Mac "appeared to freeze". Afterwards no `yappyink` process was running, so
it ended one way or another; how is not recorded.

## Why, as far as can be told without the screen

Not confirmed on a Mac. It is the explanation that fits the code as it was, and
each part was found by reading before this report arrived:

1. **The overlay started Hidden.** The controller starts in Hidden, and this
   adapter never asked for Draw. But the window was shown at screen-saver level
   with `ignoresMouseEvents` false, over a 1280×720 area of a 1440×900 screen.
   So clicks in most of the screen went to a window whose controller ignored
   them, and nothing visible happened. That looks like a freeze.
2. **Keys went to the terminal.** An `Accessory` application is not activated
   on launch, so the overlay window never became key, and `d` or `q` could not
   reach it.
3. **Nothing to press in any case.** There was no global chord.

## What changed in 0.7.0 because of it

- The overlay enters Draw at startup, as the Wayland adapter always has.
- Entering Draw activates the application so the window receives keys.
- Control+Option+D and Control+Option+H work from anywhere (ADR-007), and the
  banner says to use them, or Control+C in the terminal, if the screen seems
  stuck.
- macOS font candidates are now under `/System/Library/Fonts/Supplemental`.
- `doctor` reports the native backend on macOS and Windows.

## Not observed

Whether a frame was ever visible. Whether any click drew anything. Pass-through.
The alpha-zero hit-testing question. Spaces or fullscreen behaviour. Retina
rendering correctness. **Every macOS capability stays `unknown`.** The next
useful report is a 0.7.0 run with the terminal output and a screenshot of the
frame, following the order in `docs/handoff.md`.
