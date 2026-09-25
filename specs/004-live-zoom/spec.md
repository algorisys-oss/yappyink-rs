# 004: live zoom

Status: specified, not implemented. Requirement FR-029. Decision: ADR-008.

## User story

As a presenter, I zoom into part of the screen while it keeps updating, point
at and draw on the magnified detail, then zoom back out, without granting
screen-recording permission and without my annotations drifting off what they
mark.

## Contract

Zoom is a toggle, from a toolbar button and a chord, with zoom in and zoom out
in steps (2×, 3×, 4× as a starting table). The magnified view follows the
pointer and keeps updating. It is independent of the overlay's mode: zoom works
in Draw, pass-through and hidden.

Where the platform compositor has a magnifier (GNOME, Windows), yappyink drives
it and never receives magnified pixels. The overlay is magnified with
everything else, so ink stays on the content it marks.

Settings changed to drive a system magnifier are the user's own. Record them
before the first change, restore them when zoom is turned off and on normal
exit, and say at startup how to turn the magnifier off if yappyink is killed
while zoomed.

Report zoom as a capability like any other: `available` only with native
evidence, `unavailable` with a reason on a platform without a mechanism,
`unknown` until probed. A platform without a mechanism shows no zoom button,
rather than a button that does nothing.

## Not in scope

Still zoom (freeze, magnify, draw on the frozen image) is capture, FR-026, and
needs its own contract. Zoom levels beyond the table, smooth animated zoom, and
a lens (magnifying glass) mode are later refinements.

## Acceptance

AC-FR-029, per platform: zoom in and out while a video plays underneath and
the magnified view keeps updating; draw while zoomed and the stroke lands under
the pointer; zoom out and the stroke is still on the content it marked; quit
while zoomed and the user's magnifier settings are as they were.
