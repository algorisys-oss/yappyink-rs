# yappyink

Draw on top of your screen while the applications underneath keep working.

You circle a function in your editor, draw an arrow, and then keep scrolling and
typing in that editor while your annotations stay on the screen. The ink belongs
to the screen, not to the document underneath, so it does not scroll with the
page.

**Status: early. One backend, partly working, on one desktop environment.**
Nothing here is a release, and the table below is the whole truth about what has
been demonstrated.

Formerly specified under the name *ScreenInk*, which still appears throughout
the specification documents. Same project.

## What actually works today

| Environment | State |
|---|---|
| Linux, GNOME Wayland (Mutter) | drawing, pass-through, hide and show, control socket — with the limitations below |
| Linux, wlroots compositors (layer-shell) | not implemented |
| Linux, X11 | not implemented |
| Windows | not implemented, never built there |
| macOS | not implemented, never built there |

On GNOME specifically, measured rather than assumed
([evidence](docs/evidence/)):

- The overlay is a **floating window**, not a full-screen layer. Making it
  cover the whole output makes Mutter stop compositing it, and the transparency
  is lost.
- It **cannot choose which monitor** it appears on. xdg-shell gives clients no
  positioning at all.
- It **cannot raise itself** reliably. You apply *Always on Top* once per
  launch, or the ink is covered as soon as you click another window. The app
  has a button for it (see below), but the setting is still the compositor's to
  make, not ours.

None of that is worked around by faking anything. See
[ADR-002](docs/adr/ADR-002-gnome.md) for why GNOME is a limited preview.

A **GNOME Shell extension** that would lift the last two limits lives in
[integrations/gnome/](integrations/gnome/). It is a prototype and has never been
loaded by a running Shell, so it is not part of the instructions below.

## Try it

Requires Rust 1.95 and a Wayland session.

```sh
cargo build --workspace
./target/debug/yappyink doctor   # what your machine actually offers
./target/debug/yappyink draw     # run the overlay
```

Look for a **thin outlined rectangle with a small square in its corner**. That
is the overlay; it is transparent everywhere else, so the outline is the only
way to find it. The corner square is cyan in draw mode and amber in
pass-through.

Press **`t`**, or the pinned-window button on the toolbar, and choose **Always
on Top**. (`Alt+Space` does the same thing; the button just saves remembering
it.) Then drag with the left mouse button to draw.

That is the compositor's own menu. Mutter gives an application no way to set
Always on Top for itself, so asking for the menu is as close as a Wayland client
can get. The [GNOME extension](integrations/gnome/) removes the step entirely.

A toolbar sits in the top-left of the overlay: the tools, delete, undo, redo,
clear, and the ways out. Rest the pointer on a button for a tooltip naming it
and its key.

The pointer tells you what a click will do: an I-beam for text, a crosshair for
the drawing tools, an arrow over the toolbar and in pass-through, and move or
resize cursors over the grip and the corner.

With the **text** tool, a caret follows the pointer showing exactly where the
line will sit and how tall it will be. Click to place it and type. `Esc` discards what
you typed, clicking elsewhere keeps it and starts a new one, and picking
another tool keeps it too. Text moves and resizes with the select tool like any
other object.

Input methods are wired up through `zwp_text_input_v3`: while you compose, the
provisional text is drawn underlined until the engine commits it, and the
candidate window follows the caret. If a character is missing from the main
font the renderer falls back to another installed face, so CJK, Devanagari,
Arabic, Hebrew and Thai reach the screen rather than rasterising to nothing.

**Latin is what actually works.** There is no text shaping, so scripts that
reorder or join characters come out as a row of separate base glyphs: readable
in principle, wrong in practice. Devanagari matras land after their consonant
instead of around it, and conjuncts do not form. Arabic letters do not join.
Fixing that means a shaping engine, which is not written yet. None of the
non-Latin path has been watched working by the author either; see
`docs/evidence/` for what that distinction means here.

The colour button shows the colour you are about to draw with; click it for a
row of swatches, and clicking one picks it and closes the row. `c` cycles
through the same palette without opening anything.

**It stays in pass-through**, so you can switch tools, undo and come back to
drawing while working in another application. It works there because the
overlay tells the compositor that only the toolbar's rectangle accepts pointer
input: the buttons really are clickable, and every other pixel really does pass
through to whatever is underneath. Without that the toolbar would be a picture
of buttons, which is worse than no toolbar.

**`g` shrinks the overlay to just the toolbar.** The ink goes away, the document
is kept, and the controls stay put. Worth using when you have finished
annotating for a while: a transparent window covering a whole screen and held
above everything can stop the compositor unredirecting a fullscreen application
underneath, which costs that application performance.

`q` saves and quits.

**Drag the ridged grip** at the left of the toolbar to move the overlay, and
**drag the bottom-right corner** to resize it. Both ask the compositor to run
the drag, which is the only way a Wayland client can move or size its own
window. Resizing is worth knowing about: this backend cannot go fullscreen
without losing transparency, so enlarging the window is how you annotate more
of the screen.

Keys, while the overlay has focus:

| Key | Does |
|---|---|
| `d` | draw |
| `p` | pass through: ink stays, input goes to what is underneath |
| `h` | hide the ink, keeping it in memory |
| `1` / `2` | pen / highlighter |
| `3` – `6` | line / arrow / rectangle / ellipse |
| `7` or `e` | eraser |
| `8` or `s` | select |
| `9` | text |
| `t` | window menu, where *Always on Top* lives |
| `Del` | delete what is selected |
| `u` / `r` | undo / redo |
| `w` / `o` | write / open the session file |
| `x` | clear everything |
| `c` | next colour |
| `[` / `]` | thinner / thicker |
| `-` / `=` | less / more opaque |
| `Esc` | cancel a stroke, or leave draw mode |
| `q` | quit |

The corner of the overlay shows the mode, the colour and width you are about to
draw with, and a pip count for the selected tool, so none of that has to be
remembered.

Shapes are dragged from one corner to the other, and a drag that goes nowhere
creates nothing rather than an invisible object you could never select or
erase. Rectangles and ellipses are outlines, not fills, because an annotation
frames what is underneath rather than hiding it.

With the **select** tool, click an object to pick it up, drag it to move it,
drag a corner handle to resize it, and press Delete to remove it. While you
drag, the object fades where it still is and the preview is drawn at full
strength where it is going, so the two are not mistaken for each other. Each is one
undoable edit, and undo restores the exact geometry rather than an approximate
reverse. Clicking empty space deselects; Escape cancels a drag first, then the
selection, then draw mode.

The eraser removes whole objects its sweep touches, never parts of them, and
one sweep is one undoable action however many objects it took. It tests the
path between pointer samples rather than the samples alone, so a fast flick
does not skip over a stroke it passed straight through. `x` clears everything
and `u` brings it all back.

A highlighter's opacity applies to the completed stroke as a whole: scribbling
back and forth over one spot gives an even wash rather than a dark smear.
Drawing over it a *second* time does build up, because that is you asking for a
denser mark.

### Reaching it when something else has focus

Once pass-through is working, clicking the window underneath takes the keyboard
with it, so the overlay cannot hear you. A running overlay listens on a socket:

```sh
yappyink toggle-draw     # from any terminal, whatever has focus
yappyink pass-through
yappyink hide
yappyink emergency-hide
yappyink save
yappyink load
yappyink quit
```

Bind `yappyink toggle-draw` to a chord in **Settings → Keyboard → Custom
Shortcuts** and it works from anywhere. Wayland gives an ordinary application no
way to register a global shortcut for itself; the GlobalShortcuts portal is the
other route and is not implemented yet.

## Saving

`w` writes your annotations, `o` reads them back. One file, at
`$XDG_DATA_HOME/yappyink/session.json` or `~/.local/share/yappyink/session.json`
— there is no file picker yet, because choosing a path needs the desktop's file
portal.

Both print the full path they used, and it is worth reading. A terminal inside
a snap — VS Code's, for one — sets `XDG_DATA_HOME` to somewhere under
`~/snap/`, so a session saved from there will not be found by a yappyink
launched from anywhere else. The path is honoured deliberately; it is printed
so the surprise happens where you can see it.

The file is plain JSON holding vector objects and styles. **No pixels, ever.**
Live drawing never reads your screen, so saving annotations needs no screen
capture permission and cannot contain what was behind them.

A save writes to a temporary file alongside the target and renames it into
place, so an interrupted save leaves your previous file intact. A load validates
the whole file into objects before replacing anything: a corrupt, oversized or
newer-schema file is refused and what is on screen is untouched. Autosave is
off, and there is no autosave to turn on.

## Not built yet

A file picker, image export, text shaping for non-Latin scripts, and
multiple monitors at once. Selection
picks objects by their bounding box rather than their exact outline, and there
is no multi-select and no rotation.
**Nothing is saved when you quit.**

Undo and redo are bound to plain `u` and `r` rather than the usual Ctrl chords,
because modifier tracking is not wired up yet.

Out of scope for a first release entirely: cloud sync, accounts, AI, OCR, video
recording, and screen capture. Live drawing never reads your screen, and it
never asks for a screenshot permission. Capture and freeze are a
[separate, later feature](docs/adr/ADR-003-document-and-capture.md) with their
own permission contract.

## How this repository is built

Specification first, and evidence before claims. The rules are in
[constitution.md](constitution.md); the short version is that a task is not done
because it compiles, a mock is not evidence about a desktop, and a capability
nobody probed is reported as `unknown` rather than assumed to work.

That is why [docs/evidence/](docs/evidence/) exists. Each file records what was
run on a real machine, what happened, and what was *not* tested. Failures are
kept: [E002](docs/evidence/E002-gnome-xdg-shell-experiment.md) is mostly the
story of a route that did not work, and it is the most useful document here.

[docs/learning.md](docs/learning.md) keeps the mistakes for the same reason —
what broke, why, and what changed as a result.
[docs/handoff.md](docs/handoff.md) is the state of play: what is done, what is
next, and what to know before changing anything.

```
crates/ink-core              objects, geometry, documents — no OS, no GPU
crates/ink-app               modes, transitions, gesture rules — no platform
crates/ink-render            software rasteriser, pixel-tested headlessly
crates/ink-platform          typed capability and error contracts
crates/ink-platform-wayland  the Wayland surface and event loop
apps/yappyink                the binary: doctor, draw, control commands
experiments/                 throwaway probes; delete when their ADR closes
```

The dependency direction is one-way: `ink-core` knows nothing about a window,
`ink-app` knows nothing about Wayland, and the adapter decides nothing about
modes or documents. Everything except the adapter is testable without a display,
which is why most of the test suite runs anywhere.

Specifications: [product-spec.md](product-spec.md) for requirements,
[architecture.md](architecture.md) for the design,
[platform-matrix.md](platform-matrix.md) for what each backend must prove,
[ux-state-machine.md](ux-state-machine.md) for the interaction contract, and
[tasks.md](tasks.md) for what is done and what is not.

## Checks

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
python tools/check_specs.py     # specification cross-references only
```

The last one validates requirement, task, and scenario references. It does not
test the application, and it never will.

The Gherkin files under `tests/acceptance/` are **scenario specifications, not
executable tests**. No scenario has passed. Where a scenario is partly covered
by real tests, [traceability.json](traceability.json) says which tests and what
is still missing.

## Licence

MIT. See [LICENSE](LICENSE).
