# Releasing

What "Ship it" means, what the version number claims, and what the artefacts
are and are not.

## The version scheme

Started at **0.3.0** on 2026-09-24. The project was unversioned before that;
everything up to then was `0.0.0`. **0.3.0 was never tagged**: the
cross-platform work landed before the first release was cut, so the first
published version is 0.4.0.

`0.` — and it stays there. The leading zero is not modesty, it is the platform
matrix: one operating system has a backend, and on that one the overlay is a
limited preview that cannot choose its monitor or raise itself without help.
**1.0 is not a quality claim to be earned by polish; it needs a second real
backend.** See [ADR-002](adr/ADR-002-gnome.md).

Why `.3` rather than `.1`: the minor number counts milestones delivered, and
three had been by the time versioning started — M1 the overlay and drawing, M2
the tools, history and saving, and M3 begun with text. Numbering the first
release 0.1.0 would have implied the first two did not happen.

From here:

| Change | Bump |
|---|---|
| A user-visible feature | minor — `0.3.0` → `0.4.0` |
| A fix, or anything invisible from outside | patch — `0.3.0` → `0.3.1` |
| A second platform with a working overlay | that is the 1.0 conversation |

The version lives in one place, `[workspace.package]` in the root
`Cargo.toml`, and every crate inherits it.

## "Ship it"

When the owner says **Ship it**:

1. Run the checks below and stop if any fail.
2. Bump the version in the root `Cargo.toml`, choosing the increment from the
   table and saying which was chosen and why.
3. Update `README.md`, `docs/handoff.md` and `tasks.md` as the standing
   conventions require.
4. Commit and push.
5. Tag `v<version>` and push the tag. That is what starts the release build;
   pushing to a branch alone never publishes anything.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 tools/check_specs.py

git commit -am "..." && git push
git tag v0.6.0 && git push origin v0.6.0
```

The tag is the trigger and the tag is the record. Do not move one that has been
pushed: a download and a commit have to keep meaning the same thing.

## What CI checks, and on what

| Job | Runs on | What it proves |
|---|---|---|
| `linux` | `ubuntu-latest` | format, lint, the whole suite, the spec validator, and a release build of everything including the Wayland adapter |
| `portable` | `windows-latest`, `macos-latest` | that the domain, the controller, the renderer, storage and the platform contracts compile, lint and pass their tests off Linux |

The split is the honest one. `ink-platform-wayland` is Linux-only by
construction and the workspace does not build without it elsewhere, so the
other two runners build the crates that are meant to be portable and the
binary, and nothing more.

**What the Windows and macOS jobs do not prove:** that yappyink works there.
Both carry an overlay backend (T003, T004), and those runners build it, link
it, run its unit tests, and check that `yappyink doctor` still reports it as
compiled in and never run. They do **not** run `yappyink draw`. Until 0.6.0 they
did, expecting it to fail because there was no backend; once the backends
landed it opened a real overlay on a runner with nobody to close it, and both
jobs hung to the six-hour limit on five pushes in a row. Each job now has a
30-minute limit as well. A green tick on three platforms is not three working
products.

The Linux job installs `libxkbcommon-dev`, which the overlay links for keysym
handling. Wayland itself needs nothing installed: the pure-Rust backend is
used, so `libwayland-client` is never linked.

## The artefacts

A tag builds three and attaches them to the GitHub release:

- `yappyink-x86_64-linux` — the whole thing, the only one that draws.
- `yappyink-x86_64-windows.exe` and `yappyink-aarch64-macos` — `doctor` and
  `version` only. They are published so the port has somewhere to land and so
  the portable core is provably buildable, not because they are useful yet.
  The release notes say so.

Nothing is signed or notarised, so macOS will refuse the binary until it is
cleared by hand. That is expected and is not worth working around before there
is something on macOS worth running.
