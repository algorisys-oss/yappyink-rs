# 000: feasibility execution plan

Create the implementation repository from this specification kit. Pin actual compatible dependencies and record the toolchain; do not paste an unverified set of latest-version numbers.

Build a deterministic underlying test fixture with a click counter, editable text, a scrolling area, and an animation. Keep the fixture independent of the annotation program.

Implement a minimal diagnostic CLI that records the actual session, capabilities, output scale, and known limitations. Runtime probes should detect Wayland globals/portal interfaces or X11/compositor support, not infer them from a marketing desktop name.

Evaluate native adapters separately. Reuse the basic scene and reducer only after their contracts are explicit. For Wayland, use the layer-shell example as a protocol starting point, not an attempt to re-role a winit toplevel.

For Windows, explicitly test input on fully transparent blank areas. For macOS, separately test pointer pass-through and focus/Spaces behavior. For X11, test with a real compositor and more than one relevant WM configuration. For GNOME, test stock-session behavior before choosing any companion design.

Record proof as reproducible steps, logs, exact versions, and optional video/screenshot references made by the tester. Do not put users' real desktop content into public evidence. Failure evidence is valuable and must remain in the repository.

Update ADR-001 and ADR-002 with observed results. An accepted decision must state whether the target is full release or a limited preview. No hard-coded `supported = true` flags can replace this record.
