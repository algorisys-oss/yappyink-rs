# E015: performance on the E001 machine

**Measured:** 2026-09-25, release build of 0.8.0 plus the uncommitted Wayland
resize-corner change, which does not touch painting.
**Machine:** the E001 laptop. 12th Gen Intel Core i5-1235U, 12 threads, 15 GiB,
`powersave` governor under the `performance` platform profile; Ubuntu 24.04,
GNOME 46, Wayland, output `eDP-1` at 1366×768.
**Tasks:** T024. **Requirements:** NFR-001, NFR-002, NFR-003.

**Conditions, so the numbers are not over-read.** An unrelated `ffmpeg` was
using about seven of the twelve threads throughout, and the load average was
13 to 16. Frame timings are therefore pessimistic, probably by a large factor
on the heavier scenes. The idle figures are per-process CPU time and are not
affected by other load.

## 1. Frame cost: one pointer move plus a full repaint

What every adapter does on each pointer event: the controller handles the
move, then the whole frame is repainted (clear, document, gesture preview,
chrome, selection, toolbar). Scenes are 60-point strokes of width 4, one in
five a highlighter, scattered across the surface. CPU time only; the
compositor's part and display latency are not included.

```sh
cargo test --release -p ink-ui --test frame_cost -- --ignored --nocapture --test-threads=1
```

| Surface | Scene | p50 | p95 | p99 |
|---|---|---|---|---|
| 1280×720 @1× (the Linux window) | empty | 2.0 ms | 2.6 ms | 3.0 ms |
| | 100 strokes | 11.1 ms | 15.0 ms | 15.3 ms |
| | 1,000 strokes | 96 ms | 115 ms | 120 ms |
| | 10,000 strokes (the document limit) | 1.01 s | 1.08 s | 1.09 s |
| 1920×1080 @1× | empty | 3.2 ms | 4.3 ms | 4.6 ms |
| | 100 strokes | 14.1 ms | 16.8 ms | 17.4 ms |
| | 1,000 strokes | 101 ms | 116 ms | 121 ms |
| | 10,000 strokes | 0.99 s | 1.02 s | 1.04 s |
| 1440×900 @2× (Retina, 2880×1800 px) | empty | 8.8 ms | 11.8 ms | 12.8 ms |
| | 100 strokes | 45.6 ms | 53.5 ms | 55.6 ms |
| | 1,000 strokes | 365 ms | 411 ms | 424 ms |
| | 10,000 strokes | 3.62 s | 3.71 s | 3.75 s |

Where the time goes, at 1920×1080 with 1,000 strokes and a 100-point stroke
in flight:

| Part | Cost |
|---|---|
| **the committed document** | **101.5 ms** |
| clear, with the capture floor | 1.5 ms |
| toolbar | 0.6 ms |
| gesture preview | 0.3 ms |
| frame and badge | 0.2 ms |
| one 60-point stroke, for scale | 0.09 ms |

**Finding: every pointer move re-rasterises every committed object.** Frame
cost is linear in the amount of ink, and almost all of it is ink that has not
changed. NFR-001's proposed budget is p95 ≤ 16.7 ms on a 1080p reference
machine. Under this load it is met with an empty page, missed narrowly at 100
strokes, and missed by six times at 1,000. On a Retina Mac, 100 strokes already
takes 53 ms, which is under 20 frames a second while drawing.

E006 deferred a cache of the committed scene until something was measured.
This is that measurement. A raster of the document, rebuilt only when the
document, the selection drag or the scale changes, would make a pointer move
cost roughly the empty-page figure plus the preview, whatever is on screen.

## 2. Idle cost of the live overlay

The release binary, started with no arguments, left alone. CPU time read from
`/proc/<pid>/stat` over 60 seconds in each state.

| State | CPU over 60 s | Voluntary context switches | Resident memory |
|---|---|---|---|
| Draw, visible, static | 0.01 s = **0.017 %** of one core | 189 | 359 MB |
| Hidden | 0.00 s = **0.000 %** | 0 | 359 MB |

NFR-002's target is under 1 % of one core, averaged over 60 s, while Hidden. It
is met, with the harder visible case as well. This replaces E006's 10-second
debug-build figure.

## 3. Memory and startup: the font

```sh
cargo test --release -p ink-render --test font_cost -- --ignored --nocapture --test-threads=1
```

| Face | File | Glyphs | Resident memory |
|---|---|---|---|
| main and all five fallbacks together | | | **+345 MB** |
| `NotoSansCJK-Regular.ttc` | 18 MB | 65,535 | **+329 MB** |
| `NotoSansDevanagari-Regular.ttf` | <1 MB | 954 | +3 MB |
| Arabic, Hebrew, Thai | <1 MB each | ≤1,648 | ≈0 |

Loading them took **2.4 s**, of which the CJK face was 2.4 s.

**Finding: one fallback face is 92 % of the overlay's memory, and delays the
window by seconds.** `fontdue` expands every glyph of a face when it loads it.
All three backends load the font before showing their window, so a
double-click shows nothing for that long. The same will be true on Windows,
which loads `msyh.ttc` (E010), and on macOS, which loads `Arial Unicode.ttf`;
neither has been measured. NFR-003 asks for bounded memory; this is bounded,
but the bound is a font nobody may ever type in.

Loading fallbacks only when a character first needs one would remove both
costs for anyone who writes only in the main face's scripts.

## 4. After the two fixes, same day, same machine, same kind of load

Both findings were fixed and measured again, with `ffmpeg` still running and
the load average between 14 and 21.

**The committed ink is cached** (`ink_ui::InkLayer`). It is rasterised once,
keyed on a process-wide document revision, the size, the scale, whether the
mode shows ink, and which objects a selection drag fades. Only the rows that
hold ink are composited each frame, so an empty page costs what it did before.

| p95 | before | after |
|---|---|---|
| 1280×720, 100 strokes | 15.0 ms | 5.2 ms |
| 1280×720, 1,000 strokes | 115 ms | 7.5 ms |
| 1920×1080, empty | 4.3 ms | 4.1 ms |
| 1920×1080, 100 strokes | 16.8 ms | **7.9 ms** |
| 1920×1080, 1,000 strokes | 116 ms | **10.9 ms** |
| 1920×1080, 10,000 strokes | 1.02 s | **9.9 ms** |
| Retina, empty | 11.8 ms | 11.3 ms |
| Retina, 100 strokes | 53.5 ms | **21.5 ms** |
| Retina, 1,000 strokes | 411 ms | **30.4 ms** |

Every 1080p scene now meets NFR-001's proposed 16.7 ms, under load. Retina
does not yet: the fixed cost there is filling 5.2 million pixels with the
capture floor and compositing a dense layer, and 21–30 ms under this load is
roughly 35–45 frames a second.

The cost moved rather than vanished: the layer is rebuilt when the document
changes, which is once per committed stroke, undo or erase, and that one frame
costs what every frame used to. With 1,000 strokes at 1080p that is a single
frame of about 100 ms when the button is released. Painting only the new
object onto the layer, rather than rebuilding it, would remove that.

**Fallback fonts load when first needed**, chosen by the script of the
character being drawn, so a face for another script is never read.

| Live overlay, release | before | after |
|---|---|---|
| launch to Draw | about 2.4 s | **125 ms** |
| resident memory | 359 MB | **32 MB** |

**Correction, same day.** The first "after" run reported 146 ms and 28 MB, and
the idle run in §2 was taken the same way. Both logged one
`[fault invalid_data] the frame buffer is the wrong size`, which went unread:
GNOME had maximized the window to 1366×697, and SCTK rounds each buffer up to a
multiple of 64 bytes, which that size is not, so **no frame was ever drawn**.
The idle figures are still right in kind, since an idle overlay draws nothing
either way, but the window being measured was blank. The owner found the fault
in a run of their own. With the buffer fixed, the figures above are from a run
with no faults and a drawn window.
| `TextFont::discover` | 2,403 ms, +345 MB | 77 ms, +0 MB |

Typing a CJK character still costs the 329 MB and the load time, once, at that
moment. That is the right place for it: only someone writing CJK pays.

## Not measured

Time from a pointer event to the pixels on screen, which needs a camera.
Compositor cost. Frame pacing on screen. Allocations. Windows and macOS: the
frame harness is portable and could run there, but has not. Any of this on an
idle machine.
