# E017: live zoom on GNOME through the Shell magnifier

**Observed:** 2026-09-25, on the E001 machine (GNOME Shell 46, Wayland, output
`eDP-1` at 1366×768), with the 0.8.4 release binary.
**Tasks:** T037. **Requirements:** FR-029. **Decision tested:** ADR-008.
**Status:** four runs of `experiments/gnome-magnifier/probe.sh`; what the
owner saw, reported in words, and what the overlay logged.

## How

The probe records the user's two magnifier settings, starts the overlay,
switches the Shell magnifier on at 2× through
`org.gnome.desktop.a11y.applications screen-magnifier-enabled`, leaves it on,
switches it off, keeps the overlay up so strokes can be checked, quits it, and
restores the settings on every way out.

```sh
cargo build --release -p yappyink
LOG=/tmp/yappyink-zoom-probe4.log POST=45 \
  experiments/gnome-magnifier/probe.sh ./target/release/yappyink 60 2.0
```

## Runs

| Run | Zoom on | Objects committed | `[fault` lines | Settings afterwards |
|---|---|---|---|---|
| 1 | 45 s | 0 (nothing drawn) | 0 | restored: off, 2.0 |
| 2 | 90 s | 10 | 0 | restored: off, 2.0 |
| 3 | 90 s | 11 | 0 | restored: off, 2.0 |
| 4 | 60 s, then 45 s to check | 6 | 0 | restored: off, 2.0 |

In every run the overlay was maximized by GNOME to 1366×697 and stayed in Draw
throughout.

## What the owner reported

- Run 2: "Yes, zoom happen, I was able to draw."
- Run 4: "Yes it works. Strokes are preserved." Strokes drawn while zoomed
  stayed on the content they marked after zoom was switched off.

## What that establishes

- **Live zoom without capture works on GNOME.** The compositor magnifies the
  screen and the overlay with it; yappyink never receives a pixel and nothing
  asked for permission. ADR-008's approach holds on this platform.
- **Drawing while zoomed works**, and Mutter maps the pointer to the overlay's
  own coordinates: 27 strokes were committed across runs 2 to 4, and the owner
  confirmed after run 4 that they stay on their content once zoom is off.
  Nothing in yappyink transforms input, as ADR-008 requires.
- **The user's settings are restored reliably**, on four normal exits.
- **The magnifier does not disturb the overlay**: no faults at the maximized
  1366×697 size in any run, after the 0.8.2 buffer fix.

## The feature, same day

The zoom feature was then built (`magnifier.rs`, the `z` key, the toolbar
button, `yappyink zoom` and `zoom-off`) and driven over the control socket on
this machine, reading the settings after each step:

| Step | `screen-magnifier-enabled`, `mag-factor` | Restore file |
|---|---|---|
| before | false, 2.0 | absent |
| `yappyink zoom` | true, 2.0 | written |
| `yappyink zoom` again | true, 3.0 | present |
| `yappyink zoom-off` | false, 2.0 | removed |
| `zoom`, then `yappyink quit` | false, 2.0 | removed |
| `zoom`, then `kill -9` | true, 2.0 | left behind |
| next launch | false, 2.0 | removed |

The relaunch logged "a previous run ended while zoomed; your magnifier
settings are restored".

**By hand, after the keymap fix (23f0b52).** The first build ignored `z` on
Linux: the Wayland adapter kept its own key table (learning §22). With that
fixed, the owner pressed `z` through the levels, `0` to reset, and the toolbar's
zoom button, and reported "Yes it worked".

## Not established

- Whether a video underneath kept updating while magnified. The owner did not
  mention one, and the magnifier being a live view of the stage makes it
  likely, but it was not stated.
- Zoom factors other than 2×, and changing the factor while zoomed.
- Restoring after an abnormal exit, which the probe's trap covers for Ctrl+C
  but not SIGKILL.
- Anything about the feature itself: there is no zoom button or chord yet. This
  is the mechanism, which is what the probe was for.
