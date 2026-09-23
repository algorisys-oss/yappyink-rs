# Development constitution

Status: proposed project rules, 22 September 2026.

## 1. The specification is the behavioral authority

User behavior is defined in requirement IDs and acceptance scenarios before implementation. Architecture is allowed to change; behavior is not silently changed to suit a library. A discovered contradiction is recorded as an ADR/spec amendment, not hidden in code.

## 2. Prove the platform before polishing the tools

A working transparent canvas inside an ordinary window is not proof of a desktop overlay. Each supported backend must prove stacking, drawing on empty transparent areas, input isolation, visible pass-through, shortcut activation, and safe withdrawal.

## 3. Keep platform policy out of the drawing engine

The domain uses document objects, logical coordinates, commands, and deterministic state transitions. OS handles, wgpu objects, egui events, Wayland objects, and Objective-C types remain in adapters.

## 4. Model unsupported capabilities explicitly

Use Unknown, Available, NeedsUserAction, and Unavailable states with reasons. Runtime detection is not the same as release certification. Neither a platform name nor a successful build is a capability test.

## 5. Input ownership is a safety requirement

Draw owns its annotation pointer gestures; PassThrough does not. Never forward clicks, synthesize underlying keystrokes, or require raw-device access merely to emulate pass-through. Keyboard focus is its own policy, not a side effect of mouse transparency.

## 6. Minimal privileges and local-first behavior

Basic drawing does not require reading the desktop. Capture is an optional user-initiated capability, never a dependency of live ink. No automatic content upload, analytics, screen retention, or root requirement. Logging excludes screen pixels, annotation text, and raw key streams.

## 7. Honest implementation status

Every task begins not_started. Work progresses through in_progress, implemented, and verified. A scenario text file is not an automated test. A mock backend test is not native desktop evidence. Missing hardware or desktop access is reported as not tested.

## 8. Small vertical slices

One change should connect an accepted behavior to tests and implementation. Reject broad unreviewed changes that introduce every tool or backend at once. No universal event loop, capture framework, network backend, database server, plugin system, or cloud account before a requirement needs it.

## 9. Rust and FFI discipline

Prefer typed errors, explicit ownership, and RAII cleanup. Keep unsafe code inside small platform modules with documented lifetime/thread invariants. No panics for permission denial, unsupported protocols, or ordinary environment failures. Native event-loop ownership is respected.

## 10. Dependency evidence

Choose a mutually compatible toolchain and dependency set during the feasibility implementation. Pin and record them; do not guess versions from memory. New dependencies need an explicit purpose and maintenance/license check.
