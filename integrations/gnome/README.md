# yappyink GNOME Shell extension

**Status: prototype, unverified.** It has never been loaded by a running
GNOME Shell. Everything below is what it is meant to do.

## Why this exists

[ADR-002](../../docs/adr/ADR-002-gnome.md) outcome (b). Measured in
[E002](../../docs/evidence/E002-gnome-xdg-shell-experiment.md) and
[E003](../../docs/evidence/E003-gnome-interaction-probe.md), a plain Wayland
client on Mutter can already do almost everything an annotation overlay needs:
draw, take pointer input over fully transparent pixels, hand input through with
an empty input region, and withdraw cleanly. Two things it cannot do, and no
protocol offers:

1. **Stay above other windows.** Mutter exposes no client-side always-on-top,
   so today the user applies it by hand from the window menu, once per launch.
2. **Choose where it appears.** xdg-shell has no positioning at all, so the
   overlay lands on whichever monitor the compositor picks.

This extension does those two things for windows belonging to yappyink, and
nothing else. It is about 100 lines, because every line has to keep working
across Shell releases, and the drawing engine stays in Rust where it can be
tested.

## What it does

When a yappyink window appears, the extension:

- marks it **above** other windows,
- marks it **sticky**, so annotations survive a workspace switch,
- **sizes it to the monitor** it is on.

It matches strictly on the app id `dev.yappyink.Overlay` and touches no other
application's windows. Disabling it undoes all three.

Note it sizes the window to the monitor rather than making it fullscreen. E002
finding 1 measured that a fullscreen surface on Mutter is unredirected and loses
its transparency, which is fatal for an overlay. **Whether an ordinary window
merely sized to the monitor keeps its transparency is the open question this
prototype exists to answer.**

## Install

```sh
./integrations/gnome/install.sh
```

Per user, no root. To check and remove:

```sh
gnome-extensions info yappyink@algorisys-oss.github.io
gnome-extensions disable yappyink@algorisys-oss.github.io
rm -rf ~/.local/share/gnome-shell/extensions/yappyink@algorisys-oss.github.io
```

A newly installed extension can usually be enabled without logging out.
**Editing it afterwards cannot**: GNOME Shell will not reload an extension it
has already loaded, and on Wayland the Shell cannot be restarted, so every code
change means logging out and back in. Budget for that.

Watch what it does with:

```sh
journalctl --user -f -o cat /usr/bin/gnome-shell
```

## Supported versions

`metadata.json` declares GNOME Shell **46** only, because that is the only
version available to test on. ADR-002 requires declared versions to have
evidence behind them. Widening the range without testing would be the kind of
claim this project does not make; adding a version means running the scenarios
on it.

## What it still does not fix

- **The top bar and dock.** The window is sized to the monitor, but Shell
  chrome is stacked above ordinary windows, so annotating over the panel is
  likely still impossible. Untested.
- **Which monitor.** It uses the monitor the compositor chose. Directing it to a
  particular one needs the app to tell the extension which, and there is no
  channel between them yet.
- **Everything else.** Drawing, input, pass-through and withdrawal are the
  application's job and already work without this.
