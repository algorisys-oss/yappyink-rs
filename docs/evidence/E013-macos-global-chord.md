# E013: pass-through and the global chord on macOS

**Observed:** 2026-09-25, reported by the owner in words, from the Mac in E011,
running the 0.7.x release binary.
**Tasks:** T004. **Requirements:** FR-003, FR-005.
**Status:** a verbal report, no log. Two capabilities move to `available`.

## What was reported

"On mac ctrl+option+D -> the user can draw again." The overlay was in
pass-through, where the window ignores all mouse events and takes no keys, and
Control+Option+D returned it to Draw.

That is the path ADR-007 chose and could not test: Carbon's
`RegisterEventHotKey`, registered by an `Accessory` application with no
accessibility permission, delivered through the main run loop while another
application was in front. Before 0.7.0 there was no way back from pass-through
on macOS except the terminal (E009).

Asked whether the application underneath could be used during pass-through,
the owner answered: "Yes, the app can be accessed." So `setIgnoresMouseEvents:`
routed input to the application beneath while the ink stayed visible, with
nothing forwarded by us (FR-003).

## Not established by this report

- Whether keyboard focus returned to the application underneath without a
  click (`keyboard_release`).
- Control+Option+H, hide and show.
- Conflict feedback: what happens when another application already owns the
  chord.
