# ADR-004: the platform-free core does not wait for the full platform matrix

Status: accepted, 2026-09-23. Amends the task dependency graph only. No
requirement or acceptance criterion changes.

## Context

`tasks.json` has T009 (the document model) and T010 (the interaction reducer)
depend on T008, "ratify architecture and support intent". T008 in turn depends
on T003, T004, T005, T006, and T007: native evidence from Windows, macOS, X11,
layer-shell Wayland, and GNOME.

The reasoning is constitution §2, prove the platform before polishing the tools,
and it is sound. A team that builds a tool palette on an unproven overlay has
built nothing.

The problem is practical. Development is happening on a single Ubuntu GNOME
Wayland machine. T003 and T004 need Windows and macOS hardware that does not
exist here. T006 needs a wlroots compositor that is not installed. Read
literally, the graph means nothing after T008 can begin until hardware is
acquired, and the only work available is more feasibility probing on a platform
whose questions T007 has now largely answered.

## Decision

**T009 and T010 depend on T007 instead of T008.**

T008 is unchanged. It remains the gate for *support claims*, and it still
requires evidence from every environment before any of them can be advertised
as supported. Nothing about this amendment lets an untested platform be called
supported.

## Why this is safe

Constitution §3 already requires `ink-core` to contain no OS handle, window,
GPU, egui, portal, or Objective-C type, and NFR-004 requires its tests to run
headlessly. A crate that cannot name a platform type cannot be invalidated by a
platform decision.

Today's GNOME work demonstrates this rather than merely asserting it. E002 and
E003 changed what is known about Mutter's stacking, transparency, output
placement, and input regions. None of it says anything about how a stroke is
represented, when a gesture becomes an object, how undo groups an eraser drag,
or which mode transitions are legal. Those are the whole content of T009 and
T010.

The platform work also now informs the core rather than being blocked behind it.
E003 established what the adapter boundary has to carry: surface-local pointer
coordinates, mode changes that must cancel an in-flight gesture, and an input
region that switches per mode. That is a better basis for designing the reducer
than a guess would have been, which is the opposite of the risk §2 warns about.

## What this does not permit

- No renderer, toolbar, tool palette, or capture work moves earlier. Those touch
  platform surfaces and stay behind their own dependencies.
- No environment is described as supported without its own native evidence.
- T011, integrating the production overlay lifecycle, still needs a real
  adapter, and on GNOME that adapter is limited in the ways E002 and E003
  record.

## Consequences

Work can proceed on the part of the product that every backend needs, on the
hardware that is actually available. The cost is that the core is designed
against one compositor's evidence rather than five. That is a real risk, and the
mitigation is the dependency rule itself: if a later platform forces a change to
the domain, the change belongs in `ink-core` only if it is still free of
platform types. If it is not, it belongs in an adapter, and the pressure to put
it in the core is a signal that the boundary is being violated.

## Revisit when

Windows, macOS, X11, or layer-shell evidence contradicts an assumption in the
document model or the reducer. Record the contradiction as a spec amendment
rather than reshaping the domain around one platform.
