# Toolchain and dependency record

Requirements: NFR-008 (dependency and release hygiene), NFR-004 (testable core).
Constitution §10 requires recorded, verified versions rather than remembered ones.

## Naming

The working repository is `yappyink-rs`. The specification kit was written under
the name **ScreenInk**, and the spec files still use it; the owner renamed the
work on 2026-09-23. The binary and workspace therefore use `yappyink`, and the
CLI command named in `platform-matrix.md` as `screenink toggle-draw` becomes
`yappyink toggle-draw`. Crate names from `architecture.md` (`ink-core`, and the
later `ink-app`, `ink-render`, `ink-ui`, `ink-platform*`, `ink-storage`) are
unchanged. This is a name change only; no requirement or acceptance criterion is
affected. The spec prose has not been rewritten, so treat "ScreenInk" in the
`.md` files as the same product.

## Licence

MIT, chosen by the owner on 2026-09-23 and recorded in `LICENSE`. The workspace
manifests carry `license = "MIT"` to match. An earlier draft of this file said
`MIT OR Apache-2.0`; that was a placeholder written before the owner decided,
and it was wrong to state it as settled.

## Toolchain actually used, 2026-09-23

Read from the development machine, not chosen from memory:

| Item | Value | How it was obtained |
|---|---|---|
| rustc | 1.95.0 (59807616e 2026-04-14) | `rustc --version` |
| cargo | 1.95.0 (f2d3ce0bd 2026-03-21) | `cargo --version` |
| host triple | x86_64-unknown-linux-gnu | rustup channel sync output |
| edition | 2024 | workspace choice, supported by 1.95 |
| resolver | 3 | workspace choice, supported by 1.95 |

`rust-toolchain.toml` pins channel `1.95.0` with `rustfmt` and `clippy`. The pin
records the version that is tested here. It is not a claim about a minimum
supported version; `rust-version = "1.95"` says the same thing and will only be
lowered with evidence from an actual older-toolchain build.

## Dependencies

Added by T002: **`wayland-client` 0.31.15** (MIT), for the session probe. The
alternative was hand-rolling the Wayland wire protocol to list registry globals,
which would have risked reporting a protocol as absent because of a parsing
mistake. `platform-matrix.md` already names this crate as the Wayland route, and
T006 will add `smithay-client-toolkit` (0.21.1 at the time of writing) on top of
it for the layer surface.

Default features only, which means the **pure-Rust backend**: the probe does not
link `libwayland-client.so`, so a missing system library cannot be mistaken for a
missing compositor. `system` and `dlopen` stay off until something needs them.

The resulting graph, from `cargo tree`:

| Crate | Version | Role |
|---|---|---|
| wayland-client | 0.31.15 | protocol client |
| wayland-backend | 0.3.17 | wire protocol |
| wayland-scanner | 0.31.11 | build-time protocol codegen (proc-macro) |
| wayland-sys | 0.31.11 | present in the graph; no C library is linked in this configuration |
| rustix, linux-raw-sys, bitflags, smallvec, downcast-rs, memchr, quick-xml, proc-macro2, quote, unicode-ident | see Cargo.lock | transitive |

Exact versions are pinned by `Cargo.lock`, which is the authoritative record;
the manifests carry caret requirements so a security update is not blocked.

A licence audit of this graph has not been run yet (NFR-008). `wayland-client`
is MIT; the rest were not checked individually, and that check belongs to T031.
`ink-core`, `ink-platform`, and `ink-platform-wayland` all set
`#![forbid(unsafe_code)]`; the dependencies contain unsafe code that has not been
reviewed.

Added by the text tool: **`fontdue` 0.9.4** (MIT/Apache-2.0), for glyph
rasterisation. Chosen over a shaping stack such as cosmic-text because it
rasterises straight to a coverage bitmap, which is the form `ink-render`
already composites in, and because its dependency tree is small. What it does
not do is shaping, bidirectional layout or font fallback, which is why the text
tool is Latin-only and why input-method support is recorded as outstanding
rather than nearly done.

No font is bundled. A sans-serif face is found by probing known system paths;
if none is found the text tool draws nothing and the application says so, which
is better than shipping a font nobody asked for or drawing placeholder boxes.

Added by input-method support: **`wayland-protocols` 0.32.13** with the
`client` and `unstable` features, for `zwp_text_input_v3`. Unstable is where
that protocol lives and it is what Mutter advertises. No new transitive tree:
`wayland-client` was already a dependency.

Still open, to be pinned by the task that needs them:

- `smithay-client-toolkit` for the layer surface (T006)
- a D-Bus client for the `GlobalShortcuts` portal (T012). This is why the
  doctor report currently says the portal was not probed.
- winit, wgpu, and egui as a mutually compatible set (T003-T005, T014)
- the global-hotkey route and its main-thread constraints (T012)

Each is pinned when its task lands, with compatibility verified by an actual
build rather than by reading version numbers.

## Checks run on 2026-09-23

```sh
cargo fmt --all --check      # clean
cargo clippy --workspace --all-targets -- -D warnings   # clean
cargo test --workspace       # 28 tests pass
cargo build --workspace --locked --offline   # in a clean copy of the tree
```

The clean-copy build ran from a `tar` copy of the repository with `target/`
excluded, confirming the exit criterion that a fresh checkout builds with the
committed lockfile.

## What these checks do not establish

They are compilation and headless-unit evidence only. No overlay, window,
surface, or input behaviour has been exercised on any platform, and Windows and
macOS have had no contact with this code at all. The one live measurement so far
is the T002 protocol probe recorded in `docs/evidence/E001-ubuntu-gnome-wayland.md`.
