# E006: rendering conventions and idle cost

Task: T014. Requirements: FR-001, FR-007, NFR-002, NFR-003.
Scenarios: AC-FR-007 (partial), AC-NFR-002.

## Finding 1: a 50% highlighter was rendering fully opaque

The first rasteriser drew a stroke as a chain of discs, blending each one onto
the canvas as it went. Consecutive samples of a stroke overlap almost entirely,
so the blend repeated dozens of times per pixel and the alpha saturated.

Measured before the fix: a highlighter at opacity 0.5 produced **alpha 255**
where the user asked for 128. The stroke was opaque.

FR-007 requires the opacity of a highlighter to apply to the completed stroke as
a whole, and this is exactly the failure that rule exists to prevent. It was
invisible in earlier runs because the only tool was an opaque pen, where the bug
has no visible effect.

**Fix:** coverage is accumulated for a whole object into a mask first, then
composited onto the canvas once. Coverage saturates rather than accumulating, so
samples within one object cover a pixel instead of darkening it.

The distinction FR-007 draws is preserved: two *separate* strokes still build up,
because drawing twice is the user asking for a denser mark. Both halves have
tests.

Fixtures in `crates/ink-render/tests/painting.rs`:

- a straight highlighter stroke carries exactly the requested alpha along its
  whole length
- a highlighter stroke crossing its own path is the same colour at the crossing
  as elsewhere
- two separate strokes are denser than one

## Finding 2: an idle overlay now costs nothing

The event loop used to poll every 8 ms, because the control channel was a
`std::sync::mpsc` receiver and a blocking Wayland dispatch would not wake for
it. That is 125 wakeups a second doing nothing, which NFR-002 forbids.

Replaced with `calloop`: the Wayland queue and the control channel are both
event sources, and the loop blocks indefinitely until one of them has something.

Measured on the E001 machine, 2026-09-23, debug build, overlay in **Draw** mode
with a surface mapped and a static scene:

```sh
./target/debug/yappyink draw &
# read utime and stime from /proc/<pid>/stat, wait 10s, read again
```

| Measurement | Result |
|---|---|
| CPU ticks used over 10 idle seconds | **0** of 1000 available |
| Voluntary context switches, whole run | 8 |
| Involuntary context switches | 0 |

NFR-002's stated target is under 1% of one logical core averaged over 60
seconds while Hidden. This measures 0% over 10 seconds while *visible*, which is
the harder case, since Hidden has no surface at all.

Caveats, so this is not over-read: a debug build, ten seconds not sixty, one
machine, and an empty document. It shows the polling loop is gone. It is not a
performance benchmark.

## Not measured

**NFR-001**, the p95 input-dispatch-to-render-submit budget of 16.7 ms, is
untouched. No timing instrumentation exists and no benchmark scene has been
built. T024 owns it, and nothing here says anything about drawing latency.

## Deferred deliberately: geometry caching

T014's work statement asks for scene caches. The painter reuses its coverage
buffer across frames, which removes a per-frame allocation the size of the
canvas. A cached raster of the committed scene, so that an unchanged document is
not re-rasterised when only the preview moves, is **not** implemented.

That is a deliberate deferral, not an oversight. `architecture.md` says not to
implement two renderers before the first one is profiled, and the same reasoning
applies to caches: there is no measurement showing re-rasterisation costs
anything at the document sizes reachable today. T024 measures first.

## Still the software renderer

No GPU path exists. `architecture.md` permits starting with a simple renderer
and warns against building a second one before profiling the first. The
convention that a GPU renderer would have to keep is pinned by the pixel tests:
premultiplied alpha, ARGB8888, a transparent pixel being four zero bytes, and
whole-object compositing for opacity.
