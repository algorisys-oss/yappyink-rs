# E008: an input-method run that produced Latin only

**Observed:** 2026-09-24, on the E001 machine (Ubuntu 24.04, GNOME Shell 46,
Wayland, ibus).
**Task:** T028. **Requirement:** FR-023.
**Status:** inconclusive, parked. The fault has not been located.

This is parked deliberately while cross-platform packaging is done first. It is
written down at the point of stopping so the next attempt starts from the
evidence rather than from memory.

## What was done

`zwp_text_input_v3` was implemented: enable/disable tied to the editor and to
protocol focus, composition kept apart from committed text, events accumulated
and applied on `done` in the specification's order, caret rectangle reported.

Font fallback was added after `fc-query` showed that DejaVuSans — first in the
renderer's candidate list, and the face this machine loads — has **zero
Devanagari coverage**. Before that, any Hindi that did arrive would have
rasterised to an empty bitmap.

## What was observed

The overlay was run, the text tool selected, the editor opened, and text typed.
The relevant log, trimmed:

```
[ime] input methods available through zwp_text_input_v3
[ime] focused
[cursor] Text
[ime] enabled for the open editor
[key] keysym XK_Shift_L utf8 None
[key] keysym XK_N utf8 Some("N")
[key] keysym XK_o utf8 Some("o")
[key] keysym XK_t utf8 Some("t")
...
[ime] disabled
```

Three facts, and the third is the useful one:

1. The protocol is advertised, the surface takes text-input focus, and `enable`
   is sent while focused. The handshake completes.
2. Every key arrived through `wl_keyboard` as a plain keysym with its Latin
   UTF-8 already attached.
3. **The input method sent nothing at all** — no preedit, no commit, no delete.

An engine that is engaged intercepts the keystrokes and hands back finished
text; it does not let them through as keysyms. So for this run the engine was
not composing for our surface.

## What is not known

Whether the engine was switched to Hindi at all during this run. The text typed
was English, so the run is equally consistent with "the user did not switch" and
with "the switch did not take effect for our window". **That ambiguity is the
reason this is inconclusive, and it is cheap to remove.**

Beyond it, three candidates remain, in the order they should be tested:

- the engine was never switched, and there is no bug here;
- ibus is running but not offering `zwp_text_input_v3` to this client, in which
  case the ibus side is where to look, not ours;
- something in our enable/commit sequence is wrong in a way the handshake log
  does not reveal.

## What was put in place to settle it

Every event from the input method is now logged, **including empty ones**: an
engine clears a composition by sending an empty preedit, so silence and
emptiness must be distinguishable or the next run answers nothing either.
Every key reaching an open editor logs its keysym and UTF-8 bytes.

## The next run

Open the text editor, switch the engine to Hindi *while it is open*, and type.

| What the log shows | Where the fault is |
|---|---|
| `[ime] preedit ...` lines appear | ours: the text arrives and we fail to draw or commit it |
| `[key]` lines with Devanagari UTF-8 | ours: the keyboard layout is delivering it and the editor mishandles it |
| `[key]` lines still Latin, no `[ime]` events | outside this codebase: the engine is not engaged for our surface |

## What will still be wrong when it works

Fallback is per glyph and `fontdue` does no shaping. Devanagari matras will sit
after their consonant instead of around it and conjuncts will not form; Arabic
will not join. Correct text for those scripts needs a shaping engine, which is a
new dependency and therefore an ADR. **Do not read "the characters appeared" as
"the script renders correctly."**
