# E003: pointer, mode switching, and output placement on GNOME Wayland

Task: T007 step 2. Requirements: FR-002, FR-003, FR-011, FR-018.
Scenarios: AC-FR-002, AC-FR-003, AC-FR-018 (manual, native).

**Status: observed on 2026-09-23** on output HDMI-1. Not yet repeated on
eDP-1.

Environment: the E001 machine. Ubuntu 24.04.4, GNOME Shell 46.0, Mutter 46.2,
native Wayland, two outputs (eDP-1 1366x768, HDMI-1 1920x1080). Probe:
`experiments/gnome-xdg-shell`, floating surface, "Always on Top" applied by the
user through Mutter's window menu as E002 requires.

## How to run it

```sh
./target/debug/exp-gnome-xdg-shell --on-output eDP-1 2>&1 | tee /tmp/e003.log
# then: Alt+Space -> Always on Top
```

Cyan corner marker means Draw, amber means PassThrough. The mode flips on a
timer so that switching does not depend on the overlay keeping keyboard focus.

## Findings

### Finding 1: pointer input works over transparent pixels

In Draw mode, dragging the left button over areas the overlay had never painted
produced a stroke that followed the pointer. The whole surface is transparent
apart from the ink and the corner marker, so every stroke begins on a fully
transparent pixel.

This is FR-002's core requirement and it holds on Mutter without any special
handling: the default `wl_surface` input region is the whole surface, and
transparency has no bearing on it.

Worth stating explicitly because it is **not** transferable. Windows layered
windows treat alpha-zero areas as click-through by default, which is exactly the
trap `platform-matrix.md` flags for T003. Wayland behaving well here says
nothing about Windows.

### Finding 2: an empty input region is honoured

Across repeated PassThrough spells, the probe never logged
`A POINTER PRESS ARRIVED WHILE IN PASS-THROUGH`, the warning it emits if a
pointer press reaches the surface while its input region is empty.

So pass-through on Mutter is genuine protocol behaviour: input is delivered to
the application underneath by the compositor, and the overlay simply stops
being a target. Nothing is forwarded, injected, or synthesised.

### Finding 3: Draw and PassThrough switch repeatedly, both ways

The probe cycles on a timer and ran through several full cycles. Drawing worked
again after PassThrough spells, so the input region is genuinely restored by
`set_input_region(None)` and the switch is not one-way.

### Finding 4: output selection is impossible, even indirectly

`--on-output eDP-1` resolves the output by name and uses the one call in
xdg-shell that can name an output: it goes fullscreen on the target, then drops
back to floating once the compositor confirms. The log shows both steps
happening.

**The window still appeared on HDMI-1.** Mutter does not keep an un-fullscreened
window on the output it was fullscreen on.

This closes the question E002 opened. A floating xdg-shell surface cannot be
placed on a chosen output by any route, direct or indirect. On a multi-monitor
GNOME session the user must move the window themselves, for example with
`Super+Shift+Left`.

FR-011 requires annotations to be owned by a selected output. On this route that
selection is a manual user action, exactly like "Always on Top".

### Finding 5: a held button does not leak across a transition (FR-018)

Observed. A stroke was started in Draw with the left button held, the mode
flipped to PassThrough mid-drag, and the button was released over the
application underneath. The half-drawn stroke was discarded rather than
committed, and the application underneath did not react to the release.

This is the safety-critical case: a half-finished drag turning into a real click
in someone else's application during a live presentation is the failure that
would lose trust in the tool. On Mutter the compositor's own input routing
handles it, because once the input region is empty the surface simply stops
being a pointer target, and the release is delivered to whoever owns the pointer
now.

Note what this does and does not establish. It shows the *compositor* does not
leak the click. The equivalent guarantee on Windows and macOS, where pointer
capture and hit-testing work differently, is untested and must not be assumed
from this result (T003, T004).

### Not yet repeated on eDP-1

Every observation above was made on HDMI-1, because the window could not be
steered to the laptop panel (finding 4). Both outputs report integer scale 1 in
E001, so no difference is expected, but "not expected" is not "tested". Repeat
with the window moved via `Super+Shift+Left` before this route is described as
working on multi-monitor GNOME.

## What this means for the GNOME route

Everything about *drawing and input* works. What does not work is everything
about *placement and stacking*, and both of those now require a manual user
action per launch:

| Behaviour | State |
|---|---|
| Pointer input over transparent pixels | works |
| Held button cancelled safely across a mode change | works |
| Pass-through via empty input region | works |
| Repeated Draw/PassThrough switching | works |
| Ink visible while interacting underneath | works, after the user sets Always on Top |
| Choosing which output to appear on | impossible; user must move the window |
| Covering a whole output | impossible; fullscreen destroys transparency |

ADR-002's outcome (c), a limited preview, therefore costs the user two manual
steps at every launch. That is honest, and it is worth shipping, but it is not
parity and must never be described as such.

It also sharpens what a GNOME Shell companion would be for: not drawing, not
input, not pass-through, all of which already work. Only placement and
stacking.
