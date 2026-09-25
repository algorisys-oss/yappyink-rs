# E007: the GNOME Shell companion prototype

Task: T007 step 3. Requirements: FR-001, FR-002, FR-003, FR-011, FR-013.
ADR: ADR-002 outcome (b).

**Status: written, never loaded.** No GNOME Shell has run this code. Nothing
below is a result; the checklist is what the run is for.

## What is being tested

E002 and E003 established that a plain Wayland client on Mutter already draws,
takes input over transparent pixels, passes input through, and withdraws
cleanly. Two things it cannot do, and no protocol offers: stay above other
windows, and choose where it appears.

The prototype is deliberately the smallest extension that could supply exactly
those two, so that a failure is informative rather than ambiguous. About 100
lines of GJS. It matches only on the app id `dev.yappyink.Overlay`, makes the
window above and sticky, sizes it to its monitor, and undoes all three when
disabled.

## The question that matters

**Does a window sized to the whole monitor keep its transparency?**

E002 finding 1 measured that a *fullscreen* surface loses it: Mutter
unredirects a surface covering the output and scans it out directly, at which
point per-pixel alpha has nothing to blend against. An ordinary window merely
sized to the monitor may or may not trip the same optimisation, and nothing in
the documentation settles it. If it does, full-output coverage is unreachable on
GNOME by any route we know of, and the honest outcome is a work-area overlay.

## How to run it

```sh
./integrations/gnome/install.sh
./target/debug/yappyink draw
journalctl --user -f -o cat /usr/bin/gnome-shell   # in another terminal
```

Editing the extension afterwards requires logging out and back in: GNOME Shell
will not reload an extension it has already loaded, and on Wayland the Shell
cannot be restarted.

## Checklist

### The two things it exists for

| # | Question | Observed |
|---|---|---|
| 1 | Does the overlay appear above other windows without touching the window menu? | |
| 2 | Does the ink stay visible after clicking another application in pass-through? | |
| 3 | Is the window sized to the whole monitor? | |
| 4 | **Is the background still transparent at that size, or black as it is in fullscreen?** | |

### What must not have broken

| # | Question | Observed |
|---|---|---|
| 5 | Does drawing still work, over transparent areas? | |
| 6 | Does pass-through still deliver clicks, scrolling and typing underneath? | |
| 7 | Does `yappyink hide` still withdraw, and `toggle-draw` bring it back above? | |
| 8 | Does the surface still cover the monitor after hiding and showing? | |
| 9 | Does the toolbar still work, and the grip and resize corner? | |

### Scope and safety

| # | Question | Observed |
|---|---|---|
| 10 | Do other applications' windows behave normally? | |
| 11 | Does a workspace switch keep the annotations on screen? | |
| 12 | After `gnome-extensions disable`, does the overlay go back to an ordinary window? | |
| 13 | Anything in the Shell journal that looks like an error from this extension? | |

### The known gap

| # | Question | Observed |
|---|---|---|
| 14 | Can you annotate over the top bar and the dock, or are they still above the overlay? | |

## Result

```text
Environment ID:            E001's machine (Ubuntu 24.04.4, GNOME Shell 46.0, Mutter 46.2, Wayland)
Extension version:         0.0.1-prototype
Date and tester:
Expected versus observed:
Result:                    pass / partial / fail / not tested
```

## What each outcome means for ADR-002

**Transparency survives and stacking works.** Outcome (b) is viable. The next
questions are distribution and the Shell-version matrix, and every version
claimed needs its own run.

**Stacking works but transparency is lost at monitor size.** Still a large win,
since the two manual steps disappear, but the overlay must be sized to less than
the full output. The ADR would record full-output coverage as unreachable on
GNOME rather than merely unimplemented.

**Stacking does not work.** The narrow approach fails, and the remaining option
is the much larger one: hosting the overlay as a Shell actor, which means the
drawing surface living inside the Shell process. That is a different project,
and ADR-002's outcome (c), an honestly limited preview, becomes the likely
answer.

## 2026-09-25: loaded for hours, and never took charge of a window

The journal on the E001 machine shows the extension enabled and disabled
around every screen lock since 15:47, so GNOME Shell has been loading it. It
never logged "took charge of": it never recognised a yappyink window.

The cause, by reading: `window-created` fires when a Wayland toplevel is
created, before the client's `set_app_id` and `set_title` requests are
processed, so both are empty then. `_adopt` decided at that instant, saw an
anonymous window, and did not look again. Fixed to wait for `shown` and decide
then. The adapter had also, for an hour, declined floating size suggestions,
which would have refused the extension's resize; that is reverted.

**Not yet observed:** the fixed extension, which needs a log-out to load, and
the question this file exists for, whether a window sized to the whole monitor
keeps its transparency.

