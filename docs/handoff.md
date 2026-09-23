# Handoff

Everything needed to pick this up cold. Updated after each successful commit.

**Last updated:** 2026-09-23, after `ec290f5` and the configure-logging fix.

## What this is

yappyink: a Rust screen-annotation overlay. Draw on top of the screen, then keep
working underneath while the ink stays. Specification-driven; the specification
came first and is still the authority.

Repository: https://github.com/algorisys-oss/yappyink-rs (public, MIT).
Developed on Ubuntu 24.04, GNOME Shell 46, Wayland, two monitors.

## Read these first

| File | Why |
|---|---|
| `AGENTS.md` | the contract: what may and may not be claimed |
| `docs/learning.md` | ten mistakes made so far, and what changed because of them |
| `docs/evidence/` | what was actually run on a real machine, including failures |
| `tasks.md` | per-task status, with evidence and what is still missing |

`docs/evidence/E002` is the single most useful document: it is mostly the story
of a route that did not work, and it is why the GNOME extension is small.

## Where things stand

**Implemented:** T001 workspace, T009 document model, T010 reducer, T013
toolbar, T014 rendering, T015 pen and highlighter, T016 shapes, T017 eraser and
history, T036 selection.

**In progress:** T002 (capability probe; the GlobalShortcuts portal is
unprobed), T007 (GNOME route; extension prototype written, never loaded), T011
(vertical slice; half the checklist unobserved), T012 (control socket works; no
portal, no bound chord, no conflict feedback).

**Not started:** 23 tasks, including save and load (T020, T021), output and DPI
correctness (T018), text and IME (T028), and everything on Windows, macOS, X11
and layer-shell Wayland, none of which has any backend at all.

183 tests. `cargo fmt`, `cargo clippy -D warnings` and `python tools/check_specs.py`
all clean.

## Build and run

```sh
cargo build --workspace
./target/debug/yappyink doctor          # what this machine offers
./target/debug/yappyink draw            # the overlay
./target/debug/yappyink toggle-draw     # from anywhere, over the control socket
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python tools/check_specs.py             # specification cross-references only
```

## The immediate next action

**Find out whether the GNOME Shell extension works.** It is written, installed
at `~/.local/share/gnome-shell/extensions/yappyink@algorisys-oss.github.io`, and
enabled in dconf, but no Shell has loaded it yet.

```sh
dbus-run-session -- gnome-shell --nested --wayland --wayland-display=wayland-99
# then, in another terminal:
WAYLAND_DISPLAY=wayland-99 ./target/debug/yappyink draw
```

Look for `[yappyink]` in the terminal that launched the nested Shell. Launch it
redirected to a file if someone else needs to read it:

```sh
dbus-run-session -- gnome-shell --nested --wayland --wayland-display=wayland-99 \
    > /tmp/nested-shell.log 2>&1
```

**Attempted on 2026-09-23 and inconclusive.** The app reached the nested
compositor and bound to its output, so the plumbing works, but the window came
back 800x529 with `MAXIMIZED | TILED_*`, which is Mutter fitting an oversized
request rather than our extension, which uses `move_resize_frame` and sets no
such flags. No `[yappyink]` line was seen. Querying the nested Shell over D-Bus
does not work: it shares `/run/user/1001/bus` with the outer session, which owns
the `org.gnome.Shell` name, so the query answers for the wrong Shell.

Two questions, and the second is the one that matters:

1. Does the overlay come up already above and filling the screen, with no
   Always on Top?
2. **Is the background transparent at that size, or black?**

E002 measured that a *fullscreen* surface loses its transparency on Mutter,
because a surface covering the output is unredirected. Whether a window merely
*sized* to the monitor trips the same optimisation is unknown, and it decides
whether full-output coverage is reachable on GNOME at all. `docs/evidence/E007`
holds the checklist and what each outcome means for ADR-002.

Note a nested compositor is not the real thing; `platform-matrix.md` says so.
It answers whether the extension loads and behaves. Confirming it on the real
session needs a logout, which was deferred because the owner was streaming.

## What to know before changing anything

**The observation debt is the biggest risk.** Six features are implemented with
passing tests and never watched working. Four bugs so far were found by the
owner running the application, none by the suite. See `docs/learning.md` §1 and
§9.

**The adapter has no tests and cannot easily have them.** Everything in
`crates/ink-platform-wayland` needs a compositor. When something is right in the
model and absent on screen, look first at `VisualState` in `overlay.rs`: if a
thing that is drawn is not in that struct, it will not trigger a repaint. That
has been the bug four times.

**Dependency direction is one way.** `ink-core` knows nothing about a window,
`ink-app` knows nothing about Wayland, and the adapter decides nothing about
modes or documents. Everything except the adapter is testable headlessly, which
is why almost all the tests exist.

**Honesty rules are enforced by the specification, not by taste.** A capability
nobody probed is `unknown`, not `unavailable`. A task is not verified because it
compiles. "Not observed on screen" in `tasks.md` means exactly that, and should
not be quietly upgraded.

## GNOME's measured ceiling

From E002 and E003, on Mutter, without the extension:

| | |
|---|---|
| Drawing, transparency, input over transparent pixels | works |
| Pass-through via an empty input region | works |
| Held button cancelled safely across a mode change | works |
| Staying above other windows | needs the user to apply Always on Top |
| Choosing which monitor | impossible by any route |
| Covering a whole output | impossible; fullscreen destroys transparency |

The app has a `t` key and a toolbar button that open the compositor's own window
menu, where Always on Top lives. That saves remembering `Alt+Space`; it does not
remove the step, and ADR-002 says so explicitly so nobody later mistakes it for
a fix.

## Open decisions

- **ADR-002 is still open.** Outcome (a), a standalone route, is closed as
  failed. It is now between (b) a Shell companion and (c) an honestly limited
  preview, and E007's result decides it.
- **The GlobalShortcuts portal** needs a D-Bus client dependency that has not
  been chosen. Until then `doctor` reports the capability as `unknown` with
  that reason.
- **A pinned toolbar in pass-through** is anticipated by `ux-state-machine.md`
  and not built: set the input region to the toolbar's rectangle rather than
  empty. It would remove the "no controls and no keyboard" dead end. The owner
  has been offered it twice and not taken it up.
- **The GPU renderer and geometry caching** are deliberately deferred until
  something is profiled (T024). `architecture.md` warns against a second
  renderer before the first is measured.

## Conventions

- Every feature updates `README.md` in the same commit.
- Every task updates `tasks.md`, `tasks.json` and `traceability.json` together.
- Native observations go in `docs/evidence/` as a new `E0NN` file, including
  what was *not* tested.
- Mistakes go in `docs/learning.md`.
- This file is updated after every successful commit, or when the owner says
  "handoff".
- Commit messages explain why, not what. Attribution line at the end.
