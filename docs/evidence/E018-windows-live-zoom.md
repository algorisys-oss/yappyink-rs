# E018: live zoom on Windows through the Magnification API

**Observed:** 2026-09-25, reported by the owner in words, on a Windows machine,
running the `yappyink-x86_64-windows.exe` 0.10.0 release binary: unsigned,
started from a download, with no installation.
**Tasks:** T038. **Requirements:** FR-029. **Decision tested:** ADR-008.
**Status:** a verbal report against a five-point checklist. One capability moves
to `available`.

## What was asked

The owner was given five checks: the startup line saying the Magnification API
is available; `z` zooming and following the mouse, then stepping and `0`
resetting; a stroke drawn while zoomed landing under the pointer;
Ctrl+Alt+Z and Ctrl+Alt+0 working from another application in pass-through;
and quitting while zoomed restoring the screen.

## What was reported

"Zoom is working on Windows as expected." The checks were not answered one by
one.

## What that establishes

- **Live zoom without capture works on Windows.** `MagSetFullscreenTransform`
  magnifies the desktop, overlay included, and yappyink never receives a pixel.
  ADR-008's approach holds on a second platform.
- **UIAccess is not needed.** The binary was unsigned and ran from a download,
  which is exactly the case UIAccess would have refused. This was T038's first
  open question.
- **Drawing while zoomed works**, as far as "as expected" against the checklist
  says; it was not reported separately. This was the second open question.

## Not established

- Each checklist item individually, including whether Ctrl+Alt+Z works from
  another application in pass-through.
- Whether Windows resets the magnification if the process is killed while
  zoomed; quitting normally resets it by design.
- Pen and touch input while zoomed, which `MagSetInputTransform` would govern
  and which needs UIAccess; only the mouse is in scope.
- A second monitor, and any scale other than 96 dpi.
