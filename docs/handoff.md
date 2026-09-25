# Handoff

Everything needed to pick this up cold. Updated after each successful commit.

**Last updated:** 2026-09-25, release 0.8.1 (startup 146 ms, 28 MB, cached ink). Release 0.8.0: no arguments, or a double-click, opens the overlay. T003 (Windows) is implemented on native evidence (E010, E012, E014); T004 (macOS) is closed by the owner with Spaces deferred and untested. Three backends exist. Windows draws, passes clicks through, and Ctrl+Alt+D brings it back, seen on one machine (E010, E012), and 0.7.1 fixes saving there; macOS draws, passes clicks through, and its Control+Option+D chord brings it back, seen on one Mac (E011, E013), after a first launch that appeared to freeze (E009). Both are now complete enough that running them is the only thing left to learn from.

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
| `docs/learning.md` | eighteen entries so far, and what changed after each |
| `docs/evidence/` | what was actually run on a real machine, including failures |
| `tasks.md` | per-task status, with evidence and what is still missing |
| `docs/ship-it.md` | the version scheme, what "Ship it" does, what CI does and does not prove |

`docs/evidence/E002` is the single most useful document: it is mostly the story
of a route that did not work, and it is why the GNOME extension is small.

## Where things stand

**Three backends now exist and share everything above the window.** The key
bindings live once in `ink_app::keymap` and each adapter only translates its
platform's spelling of a key into `Key`; the chrome lives once in `ink-ui`. Both
were extracted rather than copied, because the alternative is three products
that resemble each other.

Neither the Windows nor the macOS backend has ever been run. As of 0.7.0 the
Windows one has closed every gap in its list: IME composition, `--monitor N`,
Parked shrinking, cursors per tool, window move and resize, DPI and display
changes, and the CLI verbs as window messages. macOS gained a global chord
(Control+Option+D, Carbon, ADR-007), so pass-through has a way back there too.
Reading both adapters for that work found seven defects that would have shown
on the first run, including Draw not catching clicks on empty canvas and both
overlays starting Hidden; `docs/learning.md` §18 lists them.

The gesture preview and document painting moved into `ink-ui`
(`paint_preview`, `paint_document`, `clear`), so all three backends share them.
**Performance (E015):** idle is essentially free. The committed ink is a
cached layer (`ink_ui::InkLayer`, keyed on `Document::revision`), so 1080p
frames are 4–11 ms p95 at any ink density; Retina is 11–30 ms and not yet in
budget. Fallback fonts load on first use: startup 125 ms, 32 MB. Before quoting any number from a live run, grep its log for `fault` (docs/learning.md §20). A heavy page
still takes one slow frame per commit, which incremental layer updates would
fix.

**Known and not fixed:** the Wayland adapter never calls
`Controller::set_surface_size`, so its resize corner never appears. It is
testable on this machine and deserves its own change.

**Implemented:** T001 workspace, T009 document model, T010 reducer, T013
toolbar, T014 rendering, T015 pen and highlighter, T016 shapes, T017 eraser and
history, T020 save, T021 validated load, T036 selection.

Also done outside the task list: the toolbar is pinned in pass-through, there is
a Parked mode that shrinks the overlay to its toolbar, a colour swatch picker,
tooltips, save and quit buttons, and a window grip and resize corner.

**In progress:** T002 (capability probe; the GlobalShortcuts portal is
unprobed), T007 (GNOME route; extension prototype written, never loaded), T011
(vertical slice; half the checklist unobserved), T012 (control socket works; no
portal, no bound chord, no conflict feedback).

**Not started:** 21 tasks, including output and DPI correctness (T018), the
capability and settings UX (T019), text and IME (T028), and X11 and
layer-shell Wayland, which have no backend at all. (T003 Windows is
`implemented` on native evidence; T004 macOS is `implemented` with Spaces
deferred by the owner and untested.)

336 tests. `cargo fmt`, `cargo clippy -D warnings` and `python tools/check_specs.py`
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

## Releasing

Versioning started at 0.3.0 on 2026-09-24. **"Ship it" from the owner means:
bump the version in the root `Cargo.toml`, commit, push, then push a `v<version>`
tag.** The tag is what publishes; a branch push never does. The release workflow
refuses a tag that disagrees with `Cargo.toml`, because a rule with nothing
enforcing it is a preference. `docs/ship-it.md` has the increment table.

CI runs the full workspace on Linux and the portable crates plus the binary on
Windows and macOS, where it builds, links and unit-tests both backends and
checks that `doctor` still calls them unverified. **It does not run `draw`
there, and it does not mean the app works there.** From the Windows backend
landing until 0.7.0 it did run `draw`, which hung both jobs to the six-hour
limit on every push (`docs/learning.md` §17); each job now has a 30-minute
limit. The Linux job installs `libxkbcommon-dev`, which the overlay links.

The release notes are fixed text in `release.yml`. **When what a download does
changes, change that text in the same commit.** It went stale for two releases.

## The immediate next action, if you have a Windows machine

**Run `yappyink draw` from the 0.7.0 release, then the probe, and write down
what happens.** The first run should answer, in this order: does the frame
appear, do clicks on *empty* canvas draw rather than reach the window
underneath (the alpha-1 floor), does `Ctrl+Alt+D` switch to pass-through and
back, does `yappyink toggle-draw` from a second terminal do the same, and does
`--monitor 2` land on the second monitor.

The probe is the narrower check. It is written, it compiles,
CI lints it on a Windows runner, and **nobody has ever run it**. It cannot
answer anything until someone does: all six of its questions are about what
appears on a screen.

```sh
cargo run -p exp-windows-layered        # primary monitor
cargo run -p exp-windows-layered -- 1   # the second one
```

It prints its own checklist on startup. Ctrl+Alt+D switches mode, Ctrl+Alt+Q
quits. Record the answers in a new `docs/evidence/E0NN`, failures included, and
then T003 can move off `in_progress`.

The two questions that matter most are the ones GNOME failed: does it appear on
the monitor you asked for, and does `RegisterHotKey` work. If both do, Windows
is the first platform where the product works as specified, and ADR-002's
"limited preview" framing stops being the whole story.

### macOS, which nobody here can run

`experiments/macos-overlay` and the real backend exist on the same terms and
**have no route to being tested**: nobody on this project has a Mac. They
compile for `aarch64-apple-darwin` and CI builds and links them on a macOS
runner; that is the whole of what is known.

FR-005 is settled on paper by ADR-007: Carbon's `RegisterEventHotKey`, no
permission prompt. The focus tension from ADR-006 remains: a key-accepting
window takes focus from the application being annotated.

If a Mac ever becomes available, run it and write `docs/evidence/E0NN`. If one
never does, the honest end state is to declare macOS unsupported.

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

## Verify this on Linux before building on it

**The chrome moved.** It now lives in `crates/ink-ui` and both backends call
it; the Wayland adapter lost 538 lines and calls `ink_ui::paint_*` instead.
Behaviour should be identical — the code was moved, not rewritten — but it is a
refactor of the one thing that *was* known to work, and nobody has watched it
since.

```sh
cargo build --workspace && ./target/debug/yappyink draw
```

Check the frame, the mode badge, the toolbar and its highlight, the swatches,
the tooltips and the selection handles. Anything that looks different from
before is a regression from this refactor, not a new feature.

There are now 8 chrome tests in `crates/ink-ui/tests/chrome.rs`, which is 8 more
than this code has ever had. They cover the failures from `learning.md` §1, but
they compare buffers rather than looking at a screen, and a thing can be drawn
and still be wrong.

## What to know before changing anything

**The observation debt is the biggest risk.** Six features are implemented with
passing tests and never watched working. Four bugs so far were found by the
owner running the application, none by the suite. See `docs/learning.md` §1 and
§9.

**The adapter has no tests and cannot easily have them.** Everything in
`crates/ink-platform-wayland` needs a compositor. When something is right in the
model and absent on screen, look first at `VisualState` in `overlay.rs`: if a
thing that is drawn is not in that struct, it will not trigger a repaint. That
has been the bug six times now, including once where the comparison ran after
the value it compared had already been updated.

**Do not reason about a cursor or a repaint you cannot see.** Two fixes were
shipped for a cursor bug on the strength of reading the code, and a single run
with logging settled it immediately. Successful cursor changes are logged as
well as failures for exactly this reason. Ask for the output.

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

The toolbar is pinned in PassThrough, and Parked shrinks the surface to just
the toolbar. Both work by choosing what the input region covers, which is the
one part of the surface contract Mutter does hand over completely.

The app has a `t` key and a toolbar button that open the compositor's own window
menu, where Always on Top lives. That saves remembering `Alt+Space`; it does not
remove the step, and ADR-002 says so explicitly so nobody later mistakes it for
a fix.

## In flight

**The text tool works for Latin and is unfinished beyond it.** Entry, rendering
through `fontdue`, move and resize through the selection machinery, a caret that
follows the pointer, and its own size setting defaulting to 30 logical units.
`zwp_text_input_v3` is bound, the composition is kept apart from the committed
text and drawn underlined, and the caret rectangle is reported so the candidate
window follows.

**Non-Latin input is parked, with its evidence written up.**
`docs/evidence/E008-input-method-latin-only.md` records the one run so far and
what each outcome of the next run would mean. Read it before touching the IME
code; it exists so that attempt does not start from memory.

**What is still missing is shaping.** Per-glyph font fallback landed after Hindi
turned out to be invisible because DejaVuSans has no Devanagari at all
(`docs/learning.md` §13), so the characters now reach the screen — but nothing
reorders or joins them, so Devanagari matras sit after their consonant and
Arabic does not connect. That needs a shaping engine, which would mean a new
dependency and therefore an ADR; `AGENTS.md` does not allow one without.

None of the non-Latin path has been watched working. **The one run so far was
Latin throughout** — every key arrived as a plain keysym and the engine sent no
preedit, no commit and no delete, which means it was not engaged for that
window. Whether that is the engine, the compositor or us is still open.

The diagnostics are in place to settle it in one more run: startup logs every
loaded face, every key at an open editor logs its keysym and UTF-8 bytes, and
every event from the input method is logged including the empty ones. A run
that shows `[ime] preedit` lines puts the fault in our rendering; a run with
none puts it outside this codebase.

Also missing within text: caret movement inside a run, selecting part of one,
and re-editing a committed text object.

**Nothing on GNOME has been verified since the extension was written.** The
nested-Shell run is still the outstanding action below.

## Open decisions

- **ADR-002 is still open.** Outcome (a), a standalone route, is closed as
  failed. It is now between (b) a Shell companion and (c) an honestly limited
  preview, and E007's result decides it.
- **The GlobalShortcuts portal** needs a D-Bus client dependency that has not
  been chosen. Until then `doctor` reports the capability as `unknown` with
  that reason.
- **A shaping engine** for T028. `fontdue` rasterises a glyph and will never
  reorder a cluster. `rustybuzz` or `cosmic-text` would do it and both are new
  dependencies, so this needs an ADR before any code. Until then, non-Latin
  scripts are honestly described as broken rather than quietly shipped.
- **A file picker.** Saving uses one fixed path, because choosing a path needs
  the desktop's file portal, which needs the D-Bus dependency that is also
  blocking the shortcuts portal. Deciding that dependency unblocks both.
- **The GPU renderer and geometry caching** are deliberately deferred until
  something is profiled (T024). `architecture.md` warns against a second
  renderer before the first is measured.

## Conventions

- Every feature updates `README.md` in the same commit.
- Every task updates `tasks.md`, `tasks.json` and `traceability.json` together.
  `tools/check_specs.py` enforces that the first two agree, after they drifted
  silently for a day.
- Native observations go in `docs/evidence/` as a new `E0NN` file, including
  what was *not* tested.
- Mistakes go in `docs/learning.md`.
- This file is updated after every successful commit, or when the owner says
  "handoff".
- Commit messages explain why, not what. Attribution line at the end.
