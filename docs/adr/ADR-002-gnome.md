# ADR-002: GNOME Wayland parity

Status: open; full cross-platform release gate.

## Context

The reference project documents limitations on GNOME, and the layer-shell library documents GNOME Wayland as outside its supported environments. A transparent ordinary window and a screenshot portal do not establish persistent above-app visible pass-through. [S01, S09, S17]

## Decision to make using evidence

First measure a standalone native xdg-shell compatibility route. If it fails the full contract, evaluate a minimal GNOME Shell companion. The main application remains Rust; companion code may use GJS/JavaScript and must have its own version/installation/recovery contract. [S20]

Do not assume an extension is a solved transport between Rust-rendered content and GNOME shell actors. Prototype display/input ownership and latency. Decide whether the extension only manages surfaces or needs a narrowly scoped bridge. Avoid duplicating the entire drawing engine in the extension.

## Acceptable outcomes

A verified standalone route; a verified companion route with declared supported Shell versions; or an explicitly limited preview while full GNOME parity remains blocked. The last outcome must not be described as all-Linux support.

## Explicitly rejected shortcuts

Pretending an xdg-toplevel is a layer-shell surface; treating screenshot capture as live overlay; requiring root/raw devices merely for normal annotation; forcing the user to switch to X11 while marking GNOME supported; swallowing clicks and injecting replacements; promising a companion without testing it.

## Observations so far

**2026-09-23, T002, evidence E001.** The development machine (Ubuntu 24.04.4,
GNOME Shell 46.0, Mutter 46.2, native Wayland) advertises 28 globals and
`zwlr_layer_shell_v1` is not one of them. The only shell protocol is
`xdg_wm_base` v6. This measures the assumption the ADR was written on; it does
not decide anything, because no surface was attempted.

Two globals are worth examining when the route is chosen:
`zwp_keyboard_shortcuts_inhibit_manager_v1` (relevant to FR-005 keyboard policy,
though inhibiting a shortcut is not the same as registering one) and
`zxdg_output_manager_v1` v3 (logical output geometry for FR-012). `gtk_shell1`
is a GNOME-private protocol and is not a supported integration route.

**2026-09-23, T007 step 1, evidence E002. The standalone route failed.** A
transparent xdg-shell surface was built and run on this machine. Three
constraints and one failure:

1. `set_fullscreen` produces a black background, not a transparent one; a
   surface covering a whole output is unredirected and its alpha is discarded.
2. A non-fullscreen xdg-shell surface cannot choose its output, because the
   protocol provides no positioning.
3. A maximized surface covers the work area only, leaving the top bar and dock
   unannotatable.
4. **With an empty input region, a click reaches the application underneath and
   that application is then raised above the ink.** FR-003 requires both halves
   at once. Mutter offers no client-side always-on-top, and re-raising via
   `xdg_activation_v1` would steal focus back, which the UX contract forbids.

Transparency, alpha, pass-through and clean withdrawal all worked. Stacking is
the blocker, and it is not fixable from the client side.

**Then a qualified reversal.** On a *floating* surface, Mutter's window menu
offers "Always on Top", and with the user applying it the ink stayed visible
while typing reached the application underneath. The menu entry is greyed out
for a maximized window, which is why the first attempt failed. So the pair FR-003
requires does hold on GNOME, but only via:

- a floating surface, covering part of one screen at a position the compositor
  chooses, and
- a manual user action through the window menu, on every launch, because Mutter
  exposes no client-side always-on-top request.

**Standing decision.** Outcome (a), a verified standalone route meeting the full
contract automatically, is closed: it cannot cover a chosen output, and it cannot
stay above without the user intervening. What exists instead is a narrow, honest
outcome (c): a limited GNOME preview whose overlay capability is
`NeedsUserAction`, never `Available`, and which must never be described as
all-Linux support or as parity with a layer-shell backend.

Outcome (b), the GNOME Shell companion, remains the only route to parity on this
desktop, because granting above-stacking without a manual step is exactly what an
extension can do and a client cannot. It is not yet prototyped.

Open before any GNOME claim is made: whether always-on-top survives workspace
switches, fullscreen applications, hotplug, and suspend/resume; and whether the
overlay ever reclaims focus by itself.

## Companion prototype, 2026-09-23

Outcome (b) now has code: `integrations/gnome/`, about 100 lines of GJS. It is
scoped to exactly the two things E002 and E003 proved a Wayland client cannot
do for itself, so that a failure says something specific. It makes a yappyink
window above and sticky and sizes it to its monitor, matches strictly on the
app id, touches nothing else, and undoes all three when disabled.

It declares GNOME Shell 46 only, because that is the only version available to
test on. This ADR requires declared versions to have evidence behind them.

**It has never been loaded by a running Shell.** The open question is recorded
in `docs/evidence/E007`: whether a window merely *sized* to the monitor keeps
its transparency, given that a *fullscreen* one demonstrably does not. If it
does not, full-output coverage is unreachable on GNOME by any route currently
known, and the honest ceiling is a work-area overlay.

The decision stays open until E007 has a result.

## Required record

Actual Ubuntu/GNOME/compositor versions, scenario results, route selected, user-installation steps, failure cleanup, compatibility boundaries, and reviewer approval. All are currently unverified.
