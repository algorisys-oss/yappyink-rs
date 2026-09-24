# ADR-006: AppKit bindings for the macOS overlay

Status: accepted, 2026-09-24. Adds three dependencies, on the macOS target
only. No requirement or acceptance criterion changes.

## Context

T004 asks for native macOS evidence for FR-001, FR-002, FR-003, FR-005 and
FR-018. The overlay is an `NSWindow`, so AppKit has to be called, and `AGENTS.md`
forbids a dependency without an ADR. This is that justification, and it is the
weakest of the platform ADRs for a reason given at the end.

The mapping is less clean than Windows':

| What we need | AppKit | Confidence |
|---|---|---|
| a transparent window above other apps | borderless `NSWindow`, `setOpaque:NO`, clear background, level 1000 | documented, untested |
| pass-through without forwarding | `setIgnoresMouseEvents:` | documented, untested |
| not stealing focus | `NSApplicationActivationPolicy::Accessory` | partial — see below |
| choosing a screen | `NSScreen` | documented, untested |
| surviving Spaces and fullscreen | `NSWindowCollectionBehavior` | **genuinely uncertain** |
| a global shortcut | nothing equivalent | **no good answer** |

## Decision

**Use `objc2` 0.6 with `objc2-app-kit` 0.3 and `objc2-foundation` 0.3,
target-gated to macOS.**

`objc2` is the maintained successor to the older `objc`/`cocoa` crates, and it
is the only current option that models Objective-C ownership in the type system
rather than leaving every message send to hand-written `unsafe`. Its
`define_class!` is what makes a custom `NSView` with `drawRect:` expressible at
all without writing a runtime class by hand.

The alternative, raw `objc_msgSend` through `extern "C"`, was rejected: every
call becomes unsafe, every retain/release becomes manual, and the first mistake
is a use-after-free rather than a compile error. On a platform nobody here can
run, losing compile-time checking is the worst possible trade.

`winit` was rejected for the same reason as in ADR-005: it abstracts over
exactly the behaviour being measured.

## Two things this ADR does not settle

**The focus tension.** `Accessory` keeps the application out of the Dock and
stops it activating on launch, which is the nearest macOS has to Windows'
`WS_EX_NOACTIVATE`. But a window that accepts key presses must be able to become
key, and becoming key takes focus from the application being annotated. Windows
resolves this by having the overlay never take keyboard input at all and putting
every control on a global hot key. macOS has no free equivalent, so the probe
uses ordinary key presses and the tension is recorded rather than resolved.

**The global shortcut, FR-005.** The options are Carbon's
`RegisterEventHotKey`, which is ancient but works without permission, or an
`NSEvent` global monitor, which requires the user to grant accessibility
permission. That is a product decision with a permission prompt attached, not an
implementation detail, and it needs its own ADR once the basic overlay is known
to work.

## Consequences

- Three dependencies, one target, pinned in `Cargo.lock`.
- The probe is throwaway and must be deleted once T004 is settled, like the
  other two experiments. Nothing in `ink-*` may import it.
- **This is the least verified code in the repository, and the gap is not
  closeable by us.** The Windows probe at least has an owner with a Windows
  machine. Nobody on this project has a Mac. It type-checks for
  `aarch64-apple-darwin` and CI compiles it on a macOS runner, and that is the
  entire extent of what is known. A compiler cannot tell you whether a window
  appears.
- Every macOS capability stays `unknown`, and T004 stays `in_progress`, until
  somebody runs it and reports. If that never happens, the honest outcome is to
  say macOS is unsupported, not to ship it quietly.

## What would reverse this

Evidence that a window at screen-saver level cannot coexist with fullscreen
applications or Spaces in the way FR-001 requires. That is Q7 in the probe, it
is the most likely of the questions to fail, and if it does the answer is
probably a private API or a different window server mechanism — which would be a
new ADR, with the measurement attached.
