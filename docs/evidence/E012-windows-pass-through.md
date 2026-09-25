# E012: pass-through on Windows

**Observed:** 2026-09-25, reported by the owner in words, from the same Windows
machine as E010, running the 0.7.x release binary.
**Tasks:** T003. **Requirements:** FR-003.
**Status:** a verbal report, no screenshot or log. One capability moves to
`available`.

## What was reported

"The passthrough is working on windows." Taken to mean what FR-003 asks: with
the overlay in pass-through, ink stays visible and clicks reach the application
underneath.

The mechanism is `WS_EX_TRANSPARENT` on the whole layered window. The window
manager does the routing; nothing is forwarded or synthesised, which is the
line `AGENTS.md` draws.

## Not established by this report

- **How pass-through was left again.** The window takes no input in
  pass-through, so it was `Ctrl+Alt+D`, `yappyink toggle-draw`, or quitting.
  If it was the chord, `global_shortcut` is confirmed too; the report does not
  say, so it stays `unknown`.
- Whether keyboard focus returned to the application underneath
  (`keyboard_release`).
- Withdrawal on hide, and the rest of T003's exit criterion.
