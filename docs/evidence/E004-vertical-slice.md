# E004: the first vertical slice on GNOME Wayland

Task: T011. Requirements: FR-001, FR-002, FR-003, FR-004, FR-019.
Scenarios: AC-FR-001, AC-FR-002, AC-FR-003, AC-FR-004.

**Status: partially observed on 2026-09-23.** Draw, PassThrough, and the
recovery route were confirmed on screen by the owner. The hide-and-show items
and the transition-safety items were not reported and are left blank; blank
means not observed, not passed.

This is M1 from `roadmap.md`: activate, draw a stroke, enter PassThrough, work
underneath while the ink stays, hide, show it again. No screenshot shortcut, no
injected clicks.

## What was wired together

| Crate | Role in the slice |
|---|---|
| `ink-core` | the document and its objects |
| `ink-app` | the controller: modes, transitions, gesture rules |
| `ink-render` | software rasteriser, premultiplied ARGB8888 |
| `ink-platform-wayland` | the native surface, event loop, and effects |
| `yappyink draw` | wiring, plus the terminal recovery route |

The adapter decides nothing. Wayland events become `PlatformEvent`s, the
controller returns `Effect`s, and the adapter carries them out. Every rule about
what a mode means or when a gesture commits lives in the headless crates and is
covered by their tests.

## Smoke test, 2026-09-23

```sh
cargo build --workspace
./target/debug/yappyink draw
```

Output: `[mode] draw`, then `[output] bound to HDMI-1`. The surface was created
and mapped, and the compositor reported which output it landed on. That is all
a smoke test establishes: nothing about what appeared on screen.

## Observation checklist

Run `./target/debug/yappyink draw`, then **Alt+Space -> Always on Top**, with
`tests/fixtures/underlay.html` open behind it.

### Draw (FR-001, FR-002)

| # | Question | Observed |
|---|---|---|
| 1 | Is the background transparent, with the page visible through it? | Yes |
| 2 | Does dragging the left button leave magenta ink that follows the pointer? | Yes |
| 3 | Does drawing work over areas that have never been painted? | Yes |
| 4 | Does the page's moving marker keep animating underneath? | not reported |
| 5 | Do clicks stay off the page while drawing? | not reported |

### PassThrough (FR-003)

Press `p`.

| # | Question | Observed |
|---|---|---|
| 6 | Does the ink stay visible? | Yes, with Always on Top applied |
| 7 | Does the click counter increment exactly once per click? | not reported |
| 8 | Do scrolling and typing reach the page? | Yes |
| 9 | Does drawing stop working, as it should? | Yes |

### Hidden, and showing again (FR-004)

Press `d` to return to Draw, draw something, then press `h`.

| # | Question | Observed |
|---|---|---|
| 10 | Does the ink disappear along with the surface? | |
| 11 | Does the terminal report how many objects are kept in memory? | |
| 12 | After pressing Enter in the terminal, does the **same ink** come back? | |
| 13 | Does the desktop behave normally while hidden, with nothing swallowing clicks? | |

### Transitions and safety (FR-018, FR-019)

| # | Question | Observed |
|---|---|---|
| 14 | Start a stroke, keep the button held, press `p`. Is the stroke discarded, and does the page not react to the release? | |
| 15 | Does the terminal log `[cancelled] a stroke of N sample(s) was discarded`? | |
| 16 | Press Esc mid-stroke: does it cancel the stroke without changing mode? | |
| 17 | Press `q`. Does the surface withdraw cleanly, leaving nothing behind? | |

## Two usability failures found by running it

Neither was visible from the smoke test, which is the point: "the surface was
created and mapped" and "a person can use this" are different claims, and only
the second one matters.

### The overlay was invisible and unfindable

The first build painted nothing until ink existed. A transparent, undecorated
window with no content cannot be located on screen, so there was nowhere to aim
the pointer. The owner reported "I get the browser, but 'd' is not drawing",
which was the correct observation of an unusable application.

Fixed by painting chrome after the document: a thin frame and a corner badge,
cyan in Draw and amber in PassThrough. It is chrome, never stored in the
document, and an ink-only export must exclude it (FR-024). A real toolbar is
T013.

Also worth recording because it shaped the confusion: `d` selects Draw mode,
which is already active at startup, so pressing it correctly did nothing.
Drawing is a left-button drag. The startup banner now says so.

### PassThrough leaves no way back

Once PassThrough is working properly, clicking the application underneath gives
it keyboard focus, and every key after that belongs to it. The overlay cannot
hear a keystroke, so no in-surface shortcut can return the user to Draw.

This is not a defect in the implementation. It is FR-005 becoming concrete: a
working pass-through mode *requires* an activation route that does not depend on
the overlay having focus. The terminal recovery line is standing in for it, and
T012 owns the real one.

Related, and still open: entering Draw does not raise the surface. With Always
on Top applied it does not need to; without it the application underneath stays
stacked above and the restored input region is unreachable. `xdg_activation_v1`
is advertised on this machine (E001) and is the legitimate protocol for raising
on an explicit user request, as distinct from the self-reactivation during
PassThrough that `ux-state-machine.md` forbids. Untested.

## Known limitations of this slice

Recorded here so the checklist is not read as a claim of completeness.

- **Always on Top is manual**, every launch (E002). The application cannot
  request it.
- **The output is not chosen.** The compositor picks; the document binds to
  whatever it lands on (E003 finding 4). Moving a document between outputs is
  T027, so a surface that moves outputs mid-run logs a warning and keeps the
  ink where it is.
- **The overlay does not cover the output.** It is a floating window at a size
  the compositor grants. Fullscreen would cover it but destroys transparency.
- **Nothing is saved.** Explicit local files are T020.
- **One pen, one colour, one width.** The palette is T015 and T016, undo is
  T017, the toolbar is T013.
- **Startup is Hidden, then immediately asks for Draw**, because launching with
  `draw` is the activation. A real global shortcut is T012.
- **Recovery from Hidden is a terminal Enter**, a stand-in for T012's control
  channel.
- **The loop polls every 8 ms** rather than being event-driven, because the
  recovery channel is not a Wayland file descriptor. NFR-002 wants no idle
  wakeups; T012 and T014 own fixing that.
- **The renderer is a CPU rasteriser.** T014 owns the real one. What it fixes
  now, and what must survive, is the premultiplied-alpha convention, which has
  pixel tests in `crates/ink-render/tests/painting.rs`.
