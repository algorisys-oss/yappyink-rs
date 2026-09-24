# ADR-005: Win32 bindings for the Windows overlay

Status: accepted, 2026-09-24. Adds one dependency, on the Windows target only.
No requirement or acceptance criterion changes.

## Context

T003 asks for native Windows evidence for FR-001, FR-002, FR-003, FR-005 and
FR-018. Nothing can be measured without calling Win32, and `AGENTS.md` forbids
adding a dependency without a requirement or an ADR justifying it. This is that
justification.

The overlay contract maps onto Win32 more directly than onto anything else we
have looked at:

| What we need | Win32 |
|---|---|
| per-pixel alpha above other windows | `WS_EX_LAYERED` + `UpdateLayeredWindow` |
| stay on top | `WS_EX_TOPMOST` |
| pass-through without forwarding input | `WS_EX_TRANSPARENT` |
| never steal focus | `WS_EX_NOACTIVATE` |
| choose a monitor | ordinary window placement |
| a global shortcut | `RegisterHotKey` |

The last two are worth dwelling on, because they are the two capabilities GNOME
measurably cannot provide (`docs/evidence/E002`, ADR-002). If Windows does
supply them, then Windows — not the development machine — is where the product
first works as specified, and that changes what the README can honestly say.

## Decision

**Use `windows-sys`, pinned at 0.61.2, as a target-gated dependency.**

```toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = [...] }
```

Target-gated, so Linux builds never see it and the workspace still builds on the
development machine.

### Why `windows-sys` and not `windows`

Both are Microsoft's, generated from the same metadata. `windows` adds RAII
wrappers, `Result`, and COM plumbing; `windows-sys` is raw `extern` declarations
and constants.

We want the raw one. Everything here is GDI and user32 — no COM — so the
wrappers would buy nothing, and they cost compile time and a much larger
surface. More to the point, this is a layer whose whole job is to be an honest
description of what the OS does. A binding that returns `Result` invites the
adapter to treat an error as ordinary, when the interesting cases are precisely
the ones where Windows refuses something and we have to classify it as
unsupported, denied or conflicted (`ink-platform`'s typed errors).

### Why not a windowing crate

`winit` and friends abstract over exactly the details being tested. The
platform matrix already warns against assuming a generic stack behaves; the
GNOME work found that a plain xdg-shell surface could not do what was needed,
and that was discoverable only because the experiment spoke the protocol
directly. Using a cross-platform window crate here would mean measuring the
crate.

That is a judgement about the *probe*. If the eventual adapter wants a windowing
crate it can propose one, with evidence.

## Consequences

- One dependency, on one target, with a pinned version in `Cargo.lock`.
- `unsafe` appears in this repository for the first time in real quantity. It is
  confined to `experiments/windows-layered` for now, and the eventual adapter
  should confine it to the adapter. It must never reach `ink-core`.
- The experiment is throwaway, exactly like `experiments/gnome-xdg-shell`, and
  should be deleted once T003 is settled. It is not the adapter and nothing in
  `ink-*` may import it.
- **`windows-sys` is not evidence.** It makes the calls possible. Whether a
  layered window actually behaves is a question for a real Windows machine, and
  until one has run this, every Windows capability stays `unknown`.

## What would reverse this

If `UpdateLayeredWindow` proves too slow for full-screen repainting at an
acceptable frame rate — a real risk, since it copies the whole surface every
time — the answer is likely Direct Composition or a swap chain, which does mean
COM and therefore the `windows` crate. That would be a new ADR with the
measurement attached, not a quiet swap.
