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

As of 23 September 2026 the repository contains a working Rust workspace: the document model, the interaction reducer, a software renderer, a Wayland overlay adapter, and a local control socket (T001, T002, T009, T010, and partially T007, T011, T012). No GNOME companion, capture implementation, installer, or executable acceptance-test runner is included, and there is no X11, Windows, or macOS backend. The 36 Gherkin scenarios are requirements examples awaiting step definitions/implementation and native execution.

No claim is made that Windows, macOS, X11, or layer-shell Wayland behavior has been tested: no surface has ever been created on any of them. GNOME Wayland behavior is recorded in `docs/evidence/`, including what was measured and what was not. No application performance measurements exist. No acceptance scenario has passed; several are partially covered by unit and contract tests, which `traceability.json` records individually.

The underlay HTML page is a manual test aid. Opening it establishes nothing about the application.

Current status per task is in `tasks.md` and `tasks.json`; per requirement and scenario in `traceability.json`; per environment in `docs/evidence/`. Those four are the authority, not this file, which records only the specification kit's own reference checks.
