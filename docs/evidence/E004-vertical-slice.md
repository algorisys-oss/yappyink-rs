# E004: the first vertical slice on GNOME Wayland

Task: T011. Requirements: FR-001, FR-002, FR-003, FR-004, FR-019.
Scenarios: AC-FR-001, AC-FR-002, AC-FR-003, AC-FR-004.

**Status: built and smoke-tested on 2026-09-23; the observation run is
outstanding.** The checklist below is not filled in, and nothing here may be
read as a passing scenario until it is.

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
| 1 | Is the background transparent, with the page visible through it? | |
| 2 | Does dragging the left button leave magenta ink that follows the pointer? | |
| 3 | Does drawing work over areas that have never been painted? | |
| 4 | Does the page's moving marker keep animating underneath? | |
| 5 | Do clicks stay off the page while drawing? | |

### PassThrough (FR-003)

Press `p`.

| # | Question | Observed |
|---|---|---|
| 6 | Does the ink stay visible? | |
| 7 | Does the click counter increment exactly once per click? | |
| 8 | Do scrolling and typing reach the page? | |
| 9 | Does drawing stop working, as it should? | |

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
