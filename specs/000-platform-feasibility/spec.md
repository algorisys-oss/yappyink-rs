# 000: prove native overlay feasibility

Status: not started. This is a risk-reduction specification, not a production UI feature.

## Problem

A cross-platform GUI library can create windows without meeting the screen-annotation contract. Before choosing the complete shell, prove transparent drawing, stacking, true input pass-through, focus handoff, activation, and safe withdrawal on the actual target desktops.

## Minimum demonstrator

The smallest demonstrator creates one per-output overlay, draws a fixed visible diagonal and one user-controlled stroke, switches Hidden/Draw/PassThrough, and prints capability diagnostics. No polished toolbar, accounts, capture, text layout, shape library, or persistent session catalog.

A tiny local browser page or native test app acts as the underlying fixture: a click counter, scroll area, text field, and continuously moving marker. Use it to detect hidden screenshot substitutions, duplicated clicks, focus theft, and swallowed input.

## Required experiments

1. Start a new stroke in an area where the overlay has never drawn anything. Verify the browser counter does not change.
2. Keep the moving marker visible and running beneath the overlay. Do not capture/freeze the desktop to simulate this.
3. Switch to PassThrough. Click the counter, scroll the page, select text, and type. Ink remains visible; each click increments once.
4. Return to Draw with a global action while the other app has focus. No extra phantom dot is created by the activation action.
5. Hide and show existing ink without clearing it. Quit and force a recoverable surface failure; no invisible input blocker remains.
6. Repeat with another ordinary application, then a browser in fullscreen. Record fullscreen separately.
7. Run at non-100% scale and move the test to another output. Record native versus nested compositor behavior.

## Platform scope

Start by detecting the actual Ubuntu environment. Include Windows, macOS, X11, layer-shell Wayland, and GNOME Wayland branches before claiming the architecture is cross-platform. Use `platform-matrix.md` for candidate APIs and evidence fields.

## GNOME outcome

A standalone xdg-shell experiment may produce a useful limited mode, but visible pass-through and stable stacking must still pass. If not, prototype the minimum GNOME companion and document its integration and installation contract. Do not assume success merely because an extension can draw a shell actor.

## Acceptance and exit

Relevant scenarios are AC-FR-001 through AC-FR-006, AC-FR-012 through AC-FR-014, AC-FR-018 through AC-FR-020, and AC-NFR-005.

Exit requires an explicit pass/fail/blocked record per environment and a framework decision based on those results. A limited-preview decision permits work on tools, but it does not close the full cross-platform requirement. Full release stays blocked for any promised environment without passing evidence.
