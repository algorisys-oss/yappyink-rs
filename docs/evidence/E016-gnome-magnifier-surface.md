# E016: what GNOME offers for live zoom, probed read-only

**Observed:** 2026-09-25, on the E001 machine (GNOME Shell 46.0, Wayland).
**Tasks:** T037. **Requirements:** FR-029.
**Status:** a read-only probe of what exists. Nothing was switched on, and
nothing about zoom behaviour is established.

## Commands and results

```sh
gdbus introspect --session --dest org.gnome.Magnifier --object-path /org/gnome/Magnifier
# GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown:
# The name org.gnome.Magnifier was not provided by any .service files

gsettings list-keys org.gnome.desktop.a11y.magnifier
# brightness-* caret-tracking color-saturation contrast-* cross-hairs-*
# focus-tracking invert-lightness lens-mode mag-factor mouse-tracking
# screen-position scroll-at-edges show-cross-hairs

gsettings get org.gnome.desktop.a11y.applications screen-magnifier-enabled   # false
gsettings get org.gnome.desktop.a11y.magnifier mag-factor                    # 2.0
gsettings get org.gnome.desktop.a11y.magnifier mouse-tracking                # 'proportional'
```

## What it establishes

- The Shell magnifier exists on this machine and is driven by settings. It is
  off, at a factor of 2 and proportional pointer tracking, which are the
  defaults; nobody here has changed them.
- The D-Bus interface that Orca-era documentation describes is not reachable
  here, so ADR-008 uses the settings.

## Not established

That switching it on magnifies the overlay with everything else, that drawing
while zoomed lands under the pointer, that a video stays live, and how quickly
a factor change takes effect. Those are T037's probe.
