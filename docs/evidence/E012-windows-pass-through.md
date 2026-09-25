# E012: pass-through and the global chord on Windows

**Observed:** 2026-09-25, reported by the owner in words, from the same Windows
machine as E010, running the 0.7.x release binary.
**Tasks:** T003. **Requirements:** FR-003, FR-005.
**Status:** a verbal report, no screenshot or log. Two capabilities move to
`available`.

## What was reported

"The passthrough is working on windows." Taken to mean what FR-003 asks: with
the overlay in pass-through, ink stays visible and clicks reach the application
underneath.

The mechanism is `WS_EX_TRANSPARENT` on the whole layered window. The window
manager does the routing; nothing is forwarded or synthesised, which is the
line `AGENTS.md` draws.

Asked how pass-through was left, the owner answered: "ctrl+alt+d to go back
from pass through mode." The window takes no input in pass-through, so that is
`RegisterHotKey` delivering `WM_HOTKEY` while another application had focus
(FR-005).

## Not established by this report

- Conflict feedback: what happens when another application already owns the
  chord.
- `Ctrl+Alt+H`, and `yappyink toggle-draw` from a second terminal.
- Whether keyboard focus returned to the application underneath
  (`keyboard_release`).
- Withdrawal on hide, and the rest of T003's exit criterion.
