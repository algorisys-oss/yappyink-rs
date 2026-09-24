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
