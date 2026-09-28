# 005: mode shown on the toolbar

Status: specified, not implemented. Requirement FR-030, with NFR-006.

## User story

As a presenter, I glance at the toolbar and know whether my next click draws,
goes through to the application underneath, or does nothing because the
overlay is shrunk to its toolbar, without pressing anything to find out.

## Why this is needed

The toolbar is visible in Draw, pass-through and Shrink to toolbar (Parked),
and today it looks the same in all three. Only the selected tool is
highlighted. The amber frame that marks pass-through runs around the edge of
the surface, which is often off to one side or covered by a fullscreen
application, and Parked has no frame at all. The toolbar is where the user is
already looking.

This matters most where the toolbar cannot be clicked in these modes: on
Windows and macOS the whole overlay lets clicks through in pass-through, and
on macOS in Parked too, so trying a button is not a way to find out which mode
you are in. On macOS the window does not shrink when Parked, so the button is
the only thing that shows Parked at all.

## Contract

The button for the mode currently in effect is drawn as active:

| Mode in effect | Pass through button | Shrink to toolbar button |
|---|---|---|
| Draw | normal | normal |
| PassThrough | **active, pass-through colour** | normal |
| Parked | normal | **active, parked colour** |
| Hidden | no toolbar | no toolbar |

- **Two colours, one per mode.** Pass-through uses the amber already used for
  the pass-through frame, so the frame and the button mean the same thing.
  Parked uses a colour of its own. Both must be clearly different from each
  other and from the fill that marks the selected tool, so an active mode
  button is never mistaken for a selected tool.
- **Not colour alone (NFR-006).** An active mode button also carries a mark
  that does not depend on seeing colour, as the selected tool does. The icon
  stays readable against its fill: at least 3:1 contrast for the icon, the
  WCAG figure for graphical objects.
- **The mode in effect, not the one requested.** The button follows the
  controller's effective mode. During a transition it shows the mode the
  platform has confirmed, so the toolbar never claims a mode that has not
  happened.
- **Repainted on every change.** Every way into or out of these modes updates
  the toolbar at once: the buttons, the keys (`p`, `g`, `d`, `Esc`), the global
  chords, and `yappyink` commands. A state change with no repaint is the bug
  class in `docs/learning.md` §1 and is what the tests below target.
- **The selected tool keeps its highlight** in every mode. The mode buttons are
  a separate indication and do not replace it.
- **Tooltips** name the state when the mode is active, so a pointer resting on
  the button reads, for example, "PASS THROUGH: ON (P)".
- **One implementation.** The painting lives in the shared `ink-ui` crate,
  which all three backends already call. No backend draws its own version.

Colours are chosen while implementing, within these constraints, and written
here once chosen.

## Visual test

The requirement is that this is checked visually on every platform. Checking
it has three parts, and each proves something different. Only the third counts
as native evidence.

1. **Pixel tests in `ink-ui`** (headless). Paint the toolbar in each of Draw,
   PassThrough and Parked, then assert that the three differ, that the
   pass-through colour appears inside the Pass through button only in
   PassThrough, that the parked colour appears inside the Shrink button only in
   Parked, that each active button has its non-colour mark, and that the
   selected tool's highlight is unchanged. A controller-level test drives each
   route into and out of each mode and asserts that the toolbar repaints. These
   run in CI on the Linux, Windows and macOS runners. They show that the shared
   painter is correct on each OS's build, **not** that a window displays it.
2. **Rendered images for review.** Generate the toolbar in all three modes from
   the shared painter as PNGs, for the owner to look at before any native run
   and for the README's toolbar table.
3. **Native observation on each platform.** Run the release binary and enter
   Draw, pass-through and Parked through each route: button where it can be
   clicked, key, chord, and command where the platform has one. Confirm by eye
   and with a screenshot that the toolbar shows the right state each time and
   changes as soon as the mode does. Record each platform as its own evidence
   file (`docs/evidence/E0NN`), including what was not tried.

| Platform | Who can run the native check | Routes to try |
|---|---|---|
| Ubuntu, GNOME Wayland | on this machine | buttons (the toolbar is clickable in pass-through here), `p`, `g`, `d`, `yappyink toggle-draw`, `yappyink hide` then show |
| Windows | the owner | `p`, `g`, Ctrl+Alt+D, Ctrl+Alt+H, the buttons in Draw |
| macOS | the owner | `p`, `g`, Control+Option+D, Control+Option+H, the buttons in Draw |

A platform is reported as passing only after its native check. Until then it
stays `not_tested`, whatever CI says.

## Not in scope

- **The application's own icon** in the taskbar, Dock or panel. Changing it by
  mode is a different mechanism on each OS (Windows has taskbar overlay icons;
  macOS runs yappyink as an Accessory app with no Dock icon; GNOME has no
  per-window icon a client controls reliably), and it would need its own spec.
- **Hidden.** There is no toolbar to colour. The chords and commands are how
  you get back.
- **The frame and mode badge.** They stay as they are.
- **Making the toolbar clickable** in pass-through or Parked on Windows and
  macOS. This feature shows the mode and does not change who gets the clicks.

## Acceptance

AC-FR-030, per platform: with ink on screen, switch Draw → pass-through → Draw
→ Parked → Draw using each route the platform offers. After every switch, the
Pass through button is in the pass-through colour and marked in pass-through
and only then, the Shrink to toolbar button is in the parked colour and marked
in Parked and only then, neither is in Draw, the selected tool stays
highlighted throughout, and no switch leaves the toolbar showing the previous
mode.
