# 004: live zoom

Status: implemented on all three backends. Observed on GNOME (E017) and
Windows (E018); macOS is built and not yet run. Requirement FR-029. Decision:
ADR-008, amended for macOS.

## User story

As a presenter, I zoom into part of the screen while it keeps updating, point
at and draw on the magnified detail, then zoom back out, without my
annotations drifting off what they mark, and without granting screen-recording
permission wherever the platform can avoid it.

## Contract

Zoom steps through 2×, 3× and 4× and back to off, and has a one-press reset.
The magnified view follows the pointer and keeps updating. It is independent
of the overlay's mode: zoom works in Draw, pass-through and hidden.

| | GNOME (Wayland) | Windows | macOS |
|---|---|---|---|
| Mechanism | the Shell magnifier, driven through its settings | the Magnification API, `MagSetFullscreenTransform` | a ScreenCaptureKit capture, magnified and shown by yappyink |
| Desktop pixels in the process | none | none | frames in ScreenCaptureKit's recycled IOSurfaces, shown and never copied or stored |
| Permission | none | none | Screen Recording, asked once |
| Ink magnified with the content | yes | yes | **no**: both yappyink windows are excluded from the capture |
| Pass-through clicks | land where they appear | land where they appear | land at the real, unmagnified positions |
| Offered when | GNOME is the desktop and the magnifier's settings exist | `MagInitialize` succeeds | ScreenCaptureKit exists (macOS 12.3+) |

Controls, on every platform where zoom is offered:

- `z` steps, `0` resets, and a toolbar button steps.
- A global chord, for when the overlay has no keyboard focus: Ctrl+Alt+Z and
  Ctrl+Alt+0 on Windows, Control+Option+Z and Control+Option+0 on macOS. On
  GNOME, `yappyink zoom` and `yappyink zoom-off` over the control socket,
  bound to a key in the desktop's settings.

A platform without a mechanism shows no zoom button, and `z` there says why,
rather than a control that does nothing.

**The user's own settings.** On GNOME the magnifier is the user's accessibility
setting. Record it before the first change, restore it on zoom off and on
exit, and after a kill restore it on the next launch from a restore file; say
at startup that Alt+Super+8 turns the magnifier off. Windows' transform is
reset on zoom off and on exit.

**Capture on macOS** follows the rules FR-027 sets for captured pixels:
ephemeral, never stored, and our own surfaces excluded. A refused permission
leaves the overlay working and zoom off, with a message saying where to grant
it. Pass-through clicks are not remapped to the magnified picture, because
that would be input injection.

Report zoom as the `live_zoom` capability: `available` only with native
evidence, `unavailable` with a reason where there is no mechanism, `unknown`
until observed.

## Not in scope

Still zoom (freeze, magnify, draw on the frozen image) is capture, FR-026, and
needs its own contract. Magnifying the ink on macOS needs a view transform in
that adapter and is a later change. Zoom levels beyond the table, smooth
animated zoom, and a lens (magnifying glass) mode are later refinements.

## Acceptance

AC-FR-029, per platform: zoom in and out while a video plays underneath and
the magnified view keeps updating; draw while zoomed and the stroke lands
under the pointer; zoom out, and where the mechanism magnifies the ink, the
stroke is still on the content it marked; quit while zoomed and the screen,
and any setting of the user's, is as it was. On macOS additionally: refuse the
permission once and the overlay keeps working.
