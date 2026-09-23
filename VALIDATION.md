# Validation record

Prepared on 22 September 2026, when the repository was a specification kit only.
Implementation began on 23 September 2026; the sections below describe the kit's
own reference checks, which still pass and still certify nothing about the
application. What the Rust code has actually been checked against is recorded in
`docs/toolchain-and-dependencies.md` and `docs/evidence/`.

## Artifact checks

The included specification-reference validator checks unique IDs, task dependency cycles, complete requirement-to-task-to-scenario mappings, scenario file/declaration existence, source identifiers, and balanced Markdown code fences.

Run from the kit directory:

```sh
python tools/check_specs.py
```

The actual run output is recorded in `validation-output.txt`. These are documentation integrity checks only.

## Not implemented or verified

As of 23 September 2026 a minimal Rust workspace and a `doctor` capability probe exist (T001, T002). No native overlay, renderer, shortcut integration, GNOME companion, capture implementation, installer, or executable acceptance-test runner is included. The 36 Gherkin scenarios are requirements examples awaiting step definitions/implementation and native execution.

No claim is made that Windows, macOS, X11, layer-shell Wayland, or GNOME overlay behavior has been tested for this app: no surface has ever been created. No application performance measurements exist. Every task except T001 and T002 remains not_started, and no scenario has passed.

The underlay HTML page is a manual future test aid, not an implementation of screen annotation. Its existence does not establish any native app capability.
