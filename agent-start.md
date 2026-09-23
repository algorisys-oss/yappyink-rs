# First instruction to the coding agent

Copy the following into the coding agent after placing this kit in the implementation repository:

```text
Read AGENTS.md, constitution.md, product-spec.md, architecture.md,
platform-matrix.md, and specs/000-platform-feasibility/.

Begin specification-driven development of ScreenInk, a Rust screen
annotation app for Linux, Windows, and macOS.

Start with T001 and T002. Inspect the actual development environment and
repository. Create the smallest compatible Rust workspace and a diagnostic
entry point, and record the exact toolchain/dependency choices. Do not
build the full app or tool palette yet.

Then implement the first native feasibility slice for the actual Linux
session. It must show transparent ink above a live app, accept drawing
on completely blank transparent areas, switch to genuine visible
pass-through, and withdraw safely. Use tests/fixtures/underlay.html.

Keep a minimal shared model and explicit native adapters. Do not assume
winit alone provides Wayland stacking. Do not replace live overlay with
a screenshot, inject clicks, or force a switch to X11 while calling it
GNOME support.

Record platform experiments as pass/fail/blocked/not_tested. Windows and
macOS must receive their own native evidence; local Linux compilation
cannot certify them. Resolve the GNOME route through ADR-002 before
claiming full Linux parity.

For each change, provide requirement IDs, scenario IDs, exact commands
run, observed results, changed files, and remaining limitations. Follow
task dependencies. Stop broad feature expansion until the platform
feasibility decision is explicit.
```
