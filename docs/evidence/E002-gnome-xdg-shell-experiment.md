# E002: plain xdg-shell overlay on GNOME Wayland

Task: T007 step 1. Requirements: FR-001, FR-002, FR-003, FR-013.
Scenarios: AC-FR-001, AC-FR-002, AC-FR-003 (manual, native).

**Status: partially observed on 2026-09-23.** Two constraints are measured and
recorded below. The stacking question, which is what the experiment exists for,
is still open.

## What is being measured

Whether a plain `xdg_wm_base` surface on Mutter can hold ink above other
applications while pointer input reaches them, using only core protocol. E001
established that this machine has no `zwlr_layer_shell_v1`, so this is the
standalone route ADR-002 requires to be tried before any Shell companion.

Pass-through is `wl_surface.set_input_region` with an empty region. Nothing is
forwarded, injected, or synthesised, which is the boundary the constitution
draws in §5.

## How to run it

```sh
# 1. Open the fixture and leave it visible.
xdg-open tests/fixtures/underlay.html

# 2. Run the experiment. It withdraws itself after ~42 seconds.
cargo run -p exp-gnome-xdg-shell

# Variants worth recording separately:
cargo run -p exp-gnome-xdg-shell -- --windowed        # normal toplevel, not fullscreen
cargo run -p exp-gnome-xdg-shell -- --mapped-seconds 20 --passthrough-seconds 60
```

## Observed so far, 2026-09-23

Tester: Rajesh Pillai, on the E001 machine (Ubuntu 24.04.4, GNOME Shell 46.0,
Mutter 46.2, Wayland, two outputs). Underlay: Firefox, plus an incidental
second browser window.

### Finding 1: fullscreen destroys transparency

`cargo run -p exp-gnome-xdg-shell` with `set_fullscreen(None)` produced **a
black screen with the magenta diagonal on it**. Nothing underneath was visible.

The same buffer in `--windowed` mode was **fully transparent**, with the browser
underneath clearly visible through it and the diagonal and cyan square drawn
over the top. A screenshot confirms this.

So the buffer, the ARGB8888 byte order, and the premultiplied-alpha convention
are all correct, and the compositor does blend our surface. The alpha is lost
only when the surface covers an entire output. The explanation that fits is
fullscreen unredirection: Mutter stops compositing a surface that covers the
whole output and scans it out directly, at which point per-pixel alpha has
nothing to blend against.

**`set_fullscreen` and live overlay are therefore mutually exclusive on this
compositor.** This is independent of stacking and independent of ADR-002's
companion question.

### Finding 2: a non-fullscreen surface cannot choose its output

`--maximized` mapped onto the HDMI monitor, which was not the intended one.

This is the protocol, not a bug: xdg-shell deliberately gives clients no
positioning, and only `set_fullscreen(Some(output))` names an output. Combined
with finding 1, a standalone xdg-shell overlay on GNOME cannot both be
transparent and be placed on a chosen output.

FR-011 requires annotations to be owned by a selected output. A route that
cannot pick the output does not satisfy it.

### Finding 3: maximized covers the work area, not the output

With `--maximized` on the intended monitor, the surface rendered correctly over
VS Code: transparent background, the application fully visible through it, ink
on top. The configure reported maximized and tiled state flags.

The GNOME top bar and the dock remained uncovered. That is what maximized means,
and it cannot be changed from the client side. A presenter could not annotate
over either of them, nor over anything else outside the work area.

This is a usable limitation rather than a fatal one, but it has to be stated
plainly in any support claim: the annotation area would be the work area, not
the screen.

### Finding 4: with no user action, the ink does not survive a click

In phase 2 on a **maximized** surface, with the input region empty, clicking into
VS Code underneath raised VS Code **above** the overlay. The ink was hidden.

This is the failure the experiment was built to detect. Pass-through itself
works at the protocol level, and that is the point: the click reaches the
application below exactly as intended, and the consequence of the click is that
the application is raised over the ink. FR-003 requires ink to remain visible
while ordinary interaction belongs to the application underneath. Both halves
have to hold at once, and on Mutter they cannot.

There is no client-side fix. Mutter exposes no always-on-top request, and
`xdg_activation_v1` is not one: re-raising ourselves after every click would
steal focus back from the application the user just clicked, which breaks FR-003
from the other side and is explicitly forbidden by `ux-state-machine.md`
("prevent the ink surface from reactivating itself during PassThrough").

**With no user action, the standalone xdg-shell route does not meet the overlay
contract on GNOME Wayland.** Finding 5 qualifies this: a user-applied window
property changes the outcome.

### Finding 5: a user-applied "Always on Top" makes it work, on a floating surface

Sequence, run twice on 2026-09-23:

1. `--windowed` (a floating toplevel, not maximized, not fullscreen).
2. `Alt+Space` to open Mutter's window menu, then **Always on Top**.
3. Wait for phase 2, so the input region is empty.
4. Interact with the application underneath, on the same output.

Result: **the ink stayed visible while typing went to the application
underneath.** Confirmed with YouTube Studio's chat box, with a screenshot showing
the diagonal and the cyan marker drawn over the browser while text was entered
into it. This is the FR-003 pair holding at once: visible ink, real input
delivery, nothing forwarded or synthesised.

The decisive variable is **maximized versus floating**. Mutter greys out both
"Always on Top" and "Resize" for a maximized window, which is why finding 4
failed: that run had no way to request the property. On a floating surface the
menu entry is enabled and effective.

Constraints that come with it:

- It is a **manual step, every launch**. Mutter exposes no client-side request
  for always-on-top, so the application cannot set it. The user must open the
  window menu themselves. In the capability vocabulary this is
  `NeedsUserAction`, not `Available`.
- The surface is floating, so it covers part of a screen, at a position the
  compositor chooses. Findings 1 and 2 still stand: fullscreen would restore
  full coverage but destroys transparency, and no non-fullscreen surface can
  choose its output.

### Not yet tested on this route

Whether always-on-top survives a workspace switch, a fullscreen application
underneath, output hotplug, or a second overlay; whether the overlay ever
reclaims focus by itself; and what happens across suspend/resume. None of these
may be assumed from the two runs above.

### Incidental observation

The ink appeared correctly in an active screen-capture/streaming session running
at the time. `quality-gates.md` requires recorder and meeting-app capture to be
verified separately, so this is not that evidence, but it is a positive early
signal for the teaching use case.

## Observation checklist

Fill in what actually happened. "Not observed" is a valid answer and is better
than a guess.

### Phase 1, surface mapped, input region untouched

| # | Question | Observed |
|---|---|---|
| 1 | Is the background transparent, with the browser visible through it? | |
| 2 | Is the magenta diagonal visible? | |
| 3 | Does the browser's moving marker keep animating underneath? (If it freezes, something is capturing rather than overlaying.) | |
| 4 | Click the counter. Does it stay unchanged? | |
| 5 | Does the surface cover the whole output, or only part of it? Which output did the compositor choose? | |

### Phase 2, empty input region

| # | Question | Observed |
|---|---|---|
| 6 | **Does the diagonal stay visible after you click the browser?** This is the question the experiment exists for. | |
| 7 | Does the counter increment exactly once per click? | |
| 8 | Does scrolling reach the page? | |
| 9 | Does typing in the text box work? | |
| 10 | If the ink disappeared, what happened to it: did the browser raise above it, or did the surface unmap? | |

### Phase 3, withdrawn

| # | Question | Observed |
|---|---|---|
| 11 | Does the desktop behave normally, with no invisible region swallowing clicks? | |
| 12 | Did anything remain in the window list, dash, or alt-tab? | |

## Protocol notes from the smoke test

A windowed run on 2026-09-23 reported `first configure: 1280x720, state
WindowState(0x0), decorations Client`. Mutter asks the client to draw its own
decorations; this experiment draws none, which is what an overlay wants. Paste
the fullscreen run's configure line here, since its `state` flags say whether
Mutter treated the surface as fullscreen and activated.

## Result

```text
Environment ID:            E001's machine (Ubuntu 24.04.4, GNOME Shell 46.0, Mutter 46.2, Wayland)
App commit and profile:    exp-gnome-xdg-shell 0.0.0, cargo dev profile
Date and tester:           2026-09-23, Rajesh Pillai
Scenario IDs executed:     AC-FR-001 partial, AC-FR-002 not reached,
                           AC-FR-003 fail without user action / pass with it
Expected versus observed:  Expected ink to remain visible while input reaches the
                           application underneath. On a maximized surface the
                           application raised above the ink and hid it. On a
                           floating surface with "Always on Top" applied by the
                           user from Mutter's window menu, the ink stayed visible
                           while typing reached the application.
Result:                    FAIL for a fully automatic standalone route.
                           PASS, narrowly, for a floating surface plus a manual
                           per-launch user action, covering part of one screen at
                           a compositor-chosen position.
```

What did work, and is worth carrying forward: transparency and per-pixel alpha in
a non-fullscreen surface; ink drawn above other applications while the overlay
holds focus; `set_input_region` as a genuine pass-through mechanism with no
forwarding or synthesis; clean withdrawal leaving no invisible input blocker.

The blocker is stacking alone.

## What happens next, either way

**If the ink survives a click**, the standalone route is alive. Widen the
experiment to the remaining checks in `specs/000-platform-feasibility/spec.md`:
a user-drawn stroke, Hidden/Draw/PassThrough switching, activation while another
app has focus, a second application, fullscreen, and a non-100% scale.

**If the ink is raised over**, the standalone route fails the contract on this
compositor, and that is a real finding for ADR-002 rather than a bug to work
around. Do not reach for a workaround that reduces visible pass-through to
Draw/Hide and call it support (AGENTS.md). The next step is then a minimal
GNOME Shell extension prototype, measuring whether it can hold a surface above
the stack, and what its install and lifecycle contract has to be.
