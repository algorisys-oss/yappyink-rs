# What went wrong, and what changed because of it

A record of the mistakes made building this, kept for the same reason the
failure evidence in `docs/evidence/` is kept: a project that only writes down
its successes is lying by omission, and the mistakes are where the useful
information is.

Written by the coding agent that made most of them. Entries are added when
something goes wrong, not tidied up afterwards.

## 1. The same repaint bug, four times

**What happened.** Four separate features were correct in the model and absent
on screen: shapes did not appear until the mouse was released, the toolbar's
selection highlight did not move until the next stroke, the eraser's sweep
preview never drew at all, and a tooltip was tracked but never painted.

**Why.** The Wayland adapter decides when to repaint by comparing a snapshot of
what is on screen before and after handling input. Each time, the snapshot was
missing the thing that had changed, or the question it asked was a proxy for the
real one. It began as "are there freehand stroke samples?", which is not true of
a shape being dragged. Then "is a gesture in flight?", which is not true of a
button press. Then a tuple of tool, style and mode, which did not include hover.

Three of the four were found by the owner using the application. None could have
been found by the test suite, because the adapter has no tests: it needs a
compositor.

**What changed.** The snapshot is a named struct whose doc comment says that
anything drawn on screen must appear in it, so adding a field is a visible
change rather than an omission. That does not make the bug impossible, and
nothing in the suite can; what it does is put the one place to look in the one
place to look.

**The general lesson.** When a layer cannot be tested, the cost is not that bugs
appear there. It is that bugs appear there *repeatedly*, in the same shape,
because nothing pushes back. Untestable code needs its invariants written down
where they will be read.

## 2. "The surface was created" is not "a person can use this"

**What happened.** The first vertical slice was smoke-tested, mapped a surface,
reported the output it landed on, and was handed over with a checklist. It was
unusable: a fully transparent, undecorated window with nothing painted in it
cannot be located on screen, so there was nowhere to aim the pointer. The owner
reported "I get the browser, but 'd' is not drawing", which was the correct
observation of an application that could not be used.

**Why.** The smoke test asked whether the surface was created and mapped. That
is a real question with a real answer, and it is not the question that matters.

**What changed.** The overlay draws a frame and a mode badge. More usefully,
`quality-gates.md` already made this distinction — specification, unit test,
contract test, native acceptance, release certification, described as cumulative
and not substitutes — and the mistake was not reading it as binding.

## 3. Three silent no-op edits, and the bug one of them hid

**What happened.** Several source edits were applied with string replacement
without checking the match succeeded. `cargo fmt` had reformatted the anchor
text, so the replacement quietly did nothing and the build still passed.

One of these was `is_gesturing()`, which was never updated when the eraser
landed. It was missing the eraser sweep as well as both selection drags, which
means the eraser's preview would not have repainted on screen. It was found by
accident days later while writing something unrelated.

**What changed.** Replacements assert that their anchor matched. A silent no-op
is worse than a failed edit, because a failed edit is visible.

## 4. When a test fails, suspect the test

**What happened.** Twice, a failing test was the test's fault and the code was
right. The eraser's core sequence placed a rectangle across the sweep's path, so
the sweep crossed its horizontal edges and erased it too, which is correct
behaviour. A selection test asserted that `ToggleVisibility` from Draw goes to
PassThrough, when the specification says Hidden.

**Why.** A test written at the same time as the code shares its author's
assumptions. When both are wrong, they agree with each other; when only the test
is wrong, it looks like a bug in the code.

**What changed.** Nothing structural, and there may be nothing to change. It is
worth naming because the instinct on a red test is to edit the code.

## 5. A green suite that tested nothing

**What happened.** The interaction reducer's 23 tests all passed on first run.
Rather than take that as success, two rules were deliberately broken to see
whether the suite would notice. Turning a cancelled gesture into a committed one
was caught immediately. **Removing the transition-id comparison was not caught
at all.**

That comparison is the entire reason `TransitionId` exists: it stops a delayed
callback from an older mode change overwriting current state. Every test that
looked like it covered this went through `EmergencyHide`, which clears the
pending transition, so the comparison was never reached. The mechanism had zero
coverage while appearing fully tested.

**What changed.** Two tests now cover a late confirmation and a late failure
arriving while a newer transition is pending, and both fail under that mutation.

**The general lesson.** A suite that passes first time has told you nothing
about itself. Breaking the code on purpose is cheap and finds holes that
coverage percentages do not.

## 6. Writing the fixture first found a real bug immediately

**What happened.** FR-007 requires a highlighter's opacity to apply to the
completed stroke as a whole. The fixture for that was written before looking at
the renderer, and it failed at once: a highlighter at 50% opacity was rendering
at **alpha 255**, fully opaque. The rasteriser drew a stroke as a chain of
overlapping discs and blended each one, so every pixel was composited dozens of
times and the alpha saturated.

It had been invisible because the only tool was an opaque pen, where the bug has
no visible effect. Building the highlighter first and then hunting this would
have been considerably less pleasant.

**The general lesson.** This one is the specification's, not the agent's:
`tasks.md` asked for a highlighter fixture as part of the renderer task, before
the highlighter existed. Writing the test for a feature you have not built yet
is how you find out the foundation is wrong.

## 7. Documentation drifting from the code

**What happened.** The README still described a specification kit containing no
Rust application four commits after that stopped being true. On a public
repository, that left a visitor unable to tell what had been built. The owner
had to ask.

**What changed.** The README is updated in the same commit as the feature it
describes, at the owner's instruction. It is not a separate chore.

## 8. Silence is not evidence

**What happened.** The GNOME Shell extension logged nothing at all. A Shell
journal was read looking for signs of it, found only GNOME's own noise, and the
absence was almost taken as information. In fact the extension was not installed
at all, which the journal could never have shown either way.

**What changed.** The extension logs on enable, on disable, and on every window
it takes charge of, prefixed so it can be found. An extension that works and one
that was never loaded should not look identical.

## 9. The observation debt

**What happened, and is still happening.** Six features are recorded as
implemented with passing tests and `not observed on screen`: the toolbar, the
tools, the shapes, the eraser and history, selection, and half of the vertical
slice. Every bug in section 1 was found by using the application.

**Why it is recorded rather than fixed.** Only the owner has the screen. The
honest response is to mark the status accurately and keep saying so, rather than
letting a green suite stand in for evidence. `tasks.md` and `traceability.json`
both distinguish "implemented" from "verified", and the distinction is load
bearing.

## 10. Installed, and invisible: a snap redirecting XDG_DATA_HOME

**What happened.** The GNOME extension's installer honoured
`${XDG_DATA_HOME:-$HOME/.local/share}`, which is the conventional thing to do.
Run from the VS Code snap's terminal, `XDG_DATA_HOME` points at
`~/snap/code/264/.local/share`, so the extension was copied somewhere GNOME
Shell cannot read. The files existed, the copy succeeded, and
`gnome-extensions info` reported that the extension did not exist.

**Why.** The convention answers "where should this user's data go?" The actual
question was "where does GNOME Shell look?", and the Shell runs outside the
snap with its own environment. Two different questions that usually have the
same answer.

**What changed.** The installer uses `$HOME/.local/share` deliberately and says
so, and prints a note when it detects a snap. It is worth noticing that the
symptom was silence again, as in section 8: a successful copy into the wrong
place looks exactly like a successful install.

**It recurred, 2026-09-24.** The session file resolves through the same
variable, so annotations saved from the VS Code terminal land under `~/snap/`
and a yappyink launched from anywhere else finds nothing. Here, unlike the
extension, honouring the variable is the correct behaviour — the app both reads
and writes it, so it is consistent within a launch — and the fault is purely
that the user cannot tell where their file went. Save and load already print
the path, the README now warns about it, and nothing about the semantics
changed. Not every instance of a recurring mistake has the same fix: this one
needed visibility, not a different answer.

## 11. The same silent edit, twice more, and the fix that should have been first

**What happened.** Section 3 recorded three source edits that silently did
nothing because the text they were anchored to had been reformatted, and said
the fix was to assert the anchor matched. That fix was applied to some edits and
not others.

Two more slipped through. When T028 was split into T028 and T036, `tasks.json`
was updated and `tasks.md` was not, so for a day the prose said T028 was
`not_started` and described work that had already shipped, and T036 did not
appear in it at all. Both looked exactly like the work not having been done.

Then the same thing happened again with the text tool, and this time the
assertion caught it, which is how the older drift was noticed at all.

**What changed.** Asserting on each edit is a discipline, and disciplines lapse.
`tools/check_specs.py` now checks that every task in `tasks.json` has a heading
in `tasks.md` and that the two agree on its status, and that `tasks.md` contains
no task the JSON does not. Both failure modes were reproduced deliberately to
confirm the check catches them.

**The general lesson.** When a mistake recurs, the useful response is rarely to
try harder. Section 1's fix was a struct the compiler forces you to fill in;
this one is a validator that fails. Both replace an intention with something
mechanical, which is the only kind of fix that survives being tired.

## 12. The fifth repaint bug, wearing a cursor

**What happened.** The cursor was computed only inside the pointer-event
handler. Changing tool by pressing a key or clicking a toolbar button does not
move the pointer, so no pointer event arrived and the cursor stayed as it was.
Selecting the text tool and clicking straight away showed the old pointer for
that first click.

**Why it is the same bug as section 1.** Both are "the state changed and the
screen did not", and both came from attaching an update to the wrong trigger.
Section 1's fix was to compare a snapshot of everything drawn; this is the same
shape of mistake in a thing that is *not* drawn by us, so the snapshot did not
cover it.

**What changed.** The last pointer position is kept, and the cursor is
re-evaluated every loop iteration rather than only on pointer events. Applying
it is a no-op when it has not changed, so the cost is a comparison.

**The general lesson.** "Recompute it every iteration and make the write cheap"
beats "work out exactly when it can change" for anything small enough to afford
it. The clever version is what produced five bugs.

## 13. The text was never the problem; the font was

**What happened.** `zwp_text_input_v3` was implemented, the handshake was
confirmed in a log — `[ime] focused`, `[ime] enabled for the open editor` — and
Hindi still did not appear. The next instinct was to keep reading protocol code.

`fc-query` settled it in one command: **DejaVuSans has zero Devanagari
coverage**, and DejaVuSans is first in the renderer's candidate list and is what
this machine loads. `lookup_glyph_index` answers 0 for a character a face does
not have, and rasterising that gives an empty bitmap. So the text could have
been arriving perfectly correct the entire time and the screen would have looked
identical to the text never arriving at all.

**Why it was nearly missed.** Two layers can each turn text into nothing, and
they produce the same symptom. The one recently worked on is not the more likely
one; it is just the one in mind. The cheap discriminating question — *can the
font this machine loaded even draw this character?* — was not asked for a day,
and it costs one command.

**What changed.** The renderer keeps fallback faces and picks per character.
Every loaded face is logged at startup, and each key arriving at an open editor
is logged with its keysym and UTF-8 bytes, so a single run now separates "the
keyboard is still producing Latin" from "the characters are right and we failed
to draw them". This is section 12's lesson again: make the state visible rather
than deduce it.

## 14. A skip guard that asked the code under test

**What happened.** Two tests were written for the new font fallback, and both
passed first time. Following section 5, the fallback was deliberately broken to
see whether they would notice. **Both still passed.**

They each began `if !font.can_draw(c) { return; }` — a reasonable guard, since a
machine with no Devanagari font should skip rather than fail. But `can_draw`
resolves through the very lookup that was mutated. With fallback removed the
guard answered false, both tests returned early, and the suite reported success
for a feature that had been deleted.

**What changed.** The guard asks the filesystem whether the font file exists,
which is independent of the code under test, and the test then *asserts* that a
loaded face can draw the character rather than skipping when it cannot. One of
the two then caught the mutation; the other still did not, because it compared
the advance width against a fixed threshold and DejaVu's `.notdef` is a wide box
that cleared it. It now compares against the width the fallback face itself
reports. Both fail under the mutation.

**The general lesson.** A conditional skip is a silent pass, and it inherits
every assumption in its condition. If the guard and the assertion consult the
same machinery, the test cannot distinguish "not applicable here" from "broken
everywhere" — and it will choose the reassuring one.

## 15. `is_absolute` asks the host, not the protocol

**What happened.** The first CI run failed on Windows, on its first attempt, in
`ink-platform` — the crate with no dependencies and no platform code in it at
all. `WAYLAND_DISPLAY=/custom/wl.sock` was parsed with `Path::is_absolute`,
which answers by the *host's* rules: Windows wants a drive letter, so it called
that path relative, fell through to joining `XDG_RUNTIME_DIR`, and panicked
because there is no such variable there.

**Why it matters more than the bug.** The value is a POSIX path by the Wayland
protocol's definition. It means the same thing whichever machine parses the
string, so the parse had to mean the same thing too, and `is_absolute` quietly
substituted a different question. The crate was portable in the sense of
compiling everywhere and not portable in the sense of *behaving the same*, and
only one of those is worth having.

**What changed.** A leading-slash test, which is the protocol's own rule.

**The general lesson.** This is the first thing the Windows and macOS CI jobs
did, and it is the argument for them: not that the application works there — it
does not — but that "portable" degrades into "compiles" the moment nothing
checks it. It is also the fourth time a standard-library convenience answered a
nearby question instead of the intended one, after `lines()` on a trailing
newline and `XDG_DATA_HOME` in a snap, twice.

## 16. Five hundred lines that could not be tested, and did not have to be

**What happened.** Every bug in section 1 was in the same place: the chrome —
the frame, the mode badge, the toolbar, the selection handles. All of it lived
in private functions inside the Wayland adapter, so reaching it needed a
compositor, so none of it had a single test. Four of those bugs were found by
the owner using the application.

The assumption underneath was that this code was *inherently* untestable
because it was in the adapter. It was not. `paint_toolbar` takes a `Canvas`, a
`Toolbar`, a `Tool`, a colour and a scale, and returns nothing; a `Canvas` is a
slice of bytes. There was never anything platform-specific about it. It was
untestable because of where it had been put, and nowhere else.

**What changed.** A second backend forced the question — the alternative was
copying it — and it moved to a new `ink-ui` crate that depends on `ink-core`,
`ink-app` and `ink-render` and nothing else. Eight tests exist now, written
against the failures that actually happened rather than against the
implementation: the overlay that could not be seen, the mode that looked
identical in both states, the toolbar highlight that did not follow the
selected tool. Removing the highlight makes the last one fail, which was
checked rather than assumed.

**The general lesson, and it is uncomfortable.** "This layer cannot be tested"
was true of the layer and false of the code in it. The honest version was "this
code is in a place where it cannot be tested", which invites a different
question. It took needing the code twice to ask it, and the tests that came out
in an afternoon would have caught four bugs that shipped.

Worth setting against section 1's conclusion, which was that untestable code
needs its invariants written where they will be read. That was a reasonable
response to the problem as posed. It was also an accommodation, and the better
move was available the whole time.

## 17. Two releases that described code that no longer existed

**What happened.** The Windows backend landed in 0.5.0 and the macOS one in
0.6.0. The release notes for both said "there is no overlay on Windows or
macOS... neither has been started", because the notes are fixed text in
`release.yml`, written at 0.3.0, and nobody changed them. The owner found out
from the release page. Worse, the CI step that asserted `yappyink draw` fails
off Linux was still there. Once `draw` had a backend it opened a real overlay
on a headless runner and waited for a person who was never coming. Both
portable jobs hit the six-hour limit, and were cancelled, on five pushes in a
row. The release workflow does not run that step, so every release still went
green, and a cancelled CI run is easy to read as noise.

**Why.** The claim "Windows cannot draw" was written down in five places: the
notes, a CI assertion, the README, `docs/ship-it.md` and the usage text. The
code change made all five false, and the commit changed none of them. A test
that asserts an absence is a statement about the current state of the world,
and it goes on asserting it after the world changes.

**What changed.** The CI step now checks what is true in both states, that
`doctor` reports the backend as compiled in and never run, and every portable
job has a 30-minute limit. The notes describe what each download does rather
than what it lacks.

## 18. Reading the code found what the compiler never could

Reading the Windows and macOS adapters line by line, to close their listed
gaps, turned up seven defects that `clippy -D warnings` and every test had
passed over. Each one would have shown up on the first real run:

- **Draw would not have caught clicks on empty canvas** (Windows, and likely
  macOS). Both window systems let a click through wherever alpha is zero, and
  the canvas was cleared to zero. That is the mistake `AGENTS.md` warns about,
  made from the other side: transparent pixels are not capturing pixels.
- **Both overlays started Hidden**, because the controller does and neither
  adapter asked for Draw, as the Wayland one does at startup.
- **Hiding or quitting on Windows would probably have aborted.**
  `ShowWindow` sends `WM_KILLFOCUS` back into the window procedure before it
  returns, while the state was borrowed. A second borrow panics, and a panic in
  an `extern "system"` callback aborts the process.
- **Typed text was invisible on both** until Return, because each adapter had
  its own copy of the preview code and both skipped text.
- **A dragged selection would have shown twice at full strength** on both, for
  the same reason: the fade lived only in the Wayland copy.
- **macOS never received `mouseMoved:`**, because nothing turned it on.
- **The Windows frame was sent to (0, 0) on every repaint**, which only works
  on the primary monitor.

While this was being written, the first real macOS launch was reported (E009):
the screen appeared to freeze. The startup-Hidden defect, the missing chord,
and a keyboard that never reached the window explain it, and a fourth problem
turned up that reading had missed: the only macOS font path was one a stock Mac
does not have. One user's run found in minutes what the compiler never will.

The first Windows run (E010) then added another of the same kind: drawing
worked, and saving failed, because the session path knew only `XDG_DATA_HOME`
and `HOME`. It was written on Linux, tested on Linux, and nothing about it
looked platform-specific until a platform without `HOME` ran it. Its
resolution now takes the platform and environment as parameters, so the
Windows branch is tested here.

And one on Wayland, found while comparing: nothing ever calls
`Controller::set_surface_size`, so the resize corner the controller offers can
never appear there. `surface::scale_for_dpi` in 0.5.0 was the same shape of bug
(section 16): tested code that nothing calls is not tested behaviour.

**Why.** Three copies of the same painting drifted, which is the cost
`ink-ui`'s own documentation warns about. The preview and document painting
were left behind when the chrome was lifted out, because at the time they did
not look like chrome.

**What changed.** `paint_document`, `paint_preview` and `clear` are now in
`ink-ui` with pixel tests. The text-preview test was checked against the old
behaviour, put back temporarily, and fails there; the others were not
mutation-checked. Windows messages go through
a queue so no handler can re-enter. The Wayland resize corner is recorded and
not fixed here: it can be tested on this machine, and it deserves its own
change.

## 19. A deferral that was right, and then was not

E006 deferred caching the rendered document until something was measured,
following `architecture.md`. That was the right call when it was made. It then
stayed deferred through three backends while the ink a user could draw grew,
and the first measurement (E015) found 101 of 104 ms per frame going to ink
that had not changed. The same run found one fallback font holding 92 % of the
process's memory, loaded for scripts nobody had typed.

**What changed.** The measurement exists now, as ignored tests that can be run
again, and both fixes were checked against it rather than assumed: the first
cache made an empty page twice as slow, which only the numbers showed, and it
was fixed by compositing only the rows with ink. A deferral should carry the
measurement that would end it, and the measurement should be cheap to run.

## 20. A fault in the log, twice, unread

**What happened.** On Wayland every frame at 1366×697 failed with "the frame
buffer is the wrong size", so the overlay drew nothing. SCTK rounds each shared
memory slot up to 64 bytes, and `Canvas` rightly refuses a buffer that is not
exactly the frame. 1280×720 is a multiple, so the default size always worked;
GNOME auto-maximizing the window on a 1366-wide screen, and the newly working
resize corner, produced sizes that are not. The bug had been there since the
Wayland adapter was written.

Two of my measurement runs that day logged this fault, and I reported startup
time and memory from them without reading the log. The owner found it.

**What changed.** The adapter hands `Canvas` exactly the frame's bytes. And the
rule for any live run from now on: grep the log for `fault` and `failed`
before quoting a number from it. A figure from a run that faulted is a figure
about something else.

## 21. The Windows adapter decided when to repaint, and decided wrong

**What happened.** Asked whether tooltips show on hover, reading the Windows
adapter showed that a pointer move repainted only when the text tool was
selected. A hovered button changes controller state and produces no effect, so
no tooltip ever appeared; and a stroke being dragged produces no effect either,
so **it was not drawn until the button came up**. E010's screenshot shows
finished shapes, which is exactly what this bug still allows.

**Why.** The same mistake as §12, made again in a new adapter: a hand-written
list of the events that might change the screen, which was incomplete. macOS
asks for a redraw after every event and does not have the bug.

**What changed.** Every pointer event repaints on Windows too. The cached ink
layer (§19) is what makes that cheap enough not to be clever about.

## 22. "One place" that was three places, minus one

**What happened.** Zoom was bound to `z` in `ink_app::keymap`, tested there,
and did nothing on Linux, the only platform where zoom works. The Wayland
adapter matched keysyms against its own hand-written table of actions. The
commit that "put the key bindings in one place" (785613c) moved Windows and
macOS onto the shared keymap and left Wayland on its copy, and nothing
noticed because every key that existed then was in both.

**Why.** A refactor described as complete, and tests that checked the shared
table rather than whether each adapter consulted it. The first new key exposed
the gap.

**What changed.** The Wayland adapter now names the key (`keys::key_for`) and
asks the shared keymap what it means, as the others do. A test replays every
key the old table bound, so the move is checked to have lost nothing.

## 23. Two plausible designs, both wrong, found in two runs

Remembering the window size looked simple. The first version opened at the
remembered 1366x697 and relied on asking GNOME to un-maximize: GNOME
maximized it, and when asked to float, chose 1024x522 by itself, which was
then saved as the size to remember. The second grew the window on the first
configure after its first frame: GNOME sent no such configure, so it never
grew. The version that works grows right after the first frame is committed.

Neither mistake was visible in the code or the unit tests, which were right
about the arithmetic. Each took one live run to find. That is the argument for
running the thing before calling it done, made twice in ten minutes.

## What has held up well

Worth recording too, since the point is to learn rather than to flagellate.

**Keeping failure evidence.** `docs/evidence/E002` is mostly the story of a
route that did not work, and it is the most useful document in the repository.
It is what made the GNOME extension small: by the time it was written, exactly
two capabilities were known to be missing, so the extension supplies two things
and nothing else.

**Turning a specification into a test verbatim.** `specs/002-drawing-history`
writes out a sequence of edits and undos. Implementing it literally, with the
specification's own wording as the comments, gave a test that is obviously
correct against the requirement rather than against the implementation.

**Refusing to guess.** Capability states carry a reason and distinguish
"unknown" from "unavailable"; `yappyink doctor` says what it did not probe.
Several times that has been the difference between a useful report and a
confident wrong one.
