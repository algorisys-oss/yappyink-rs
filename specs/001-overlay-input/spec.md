# 001: overlay lifecycle and input ownership

Status: specified, not implemented. Depends on a feasibility decision from 000.

## User story

As a presenter, I activate drawing above the current application, draw, then continue operating that application while my ink remains visible. I can also hide the layer immediately without losing committed annotations.

## Contract

Implement FR-001 through FR-006, FR-011 through FR-014, FR-018 through FR-020, and the stable-state semantics in `ux-state-machine.md`.

A native output canvas and the floating toolbar are distinct input domains. The toolbar may be implemented as a separate surface or a proven native hit-test region, but must never rely on being clickable inside a completely input-transparent parent surface.

Startup is Hidden. A transition has a desired mode, an effective mode, an ID, and a completion/failure event. The displayed status comes from the effective mode. Capture/focus/hit-test policy is applied before reporting success.

Draw mode must handle the first stroke in a fully transparent region. PassThrough must not receive local Ctrl/Cmd+Z, text input, scroll, or pointer gestures that belong to the underlying application. Only successfully registered global actions stay active.

A unsupported pass-through operation returns a typed error and a user-visible explanation. A compatibility Draw/Hide mode is explicitly named and opt-in; it does not satisfy FR-003.

## Edge cases

Switch mode with a held mouse button; press EmergencyHide during a gesture; lose the output while drawing; receive a stale native callback after another transition; lose the shortcut portal; launch when the tray is absent; fail transparency initialization; encounter a fully transparent zero-alpha hit-test region.

## Acceptance

Use the overlay Gherkin scenarios against every claimed backend. Mock tests cover reducer/effect sequencing and error rollback; native tests cover actual input, focus, and stacking. Both are required.
