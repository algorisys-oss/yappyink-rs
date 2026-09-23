# E001: Ubuntu 24.04 GNOME Wayland, session and protocol probe

Task: T002. Requirements: FR-013, FR-020, NFR-005.
Scenario coverage: partial AC-FR-013 and AC-NFR-005 (see "What this does not
establish"). AC-FR-020 is not covered: no settings file exists yet.

## Environment record

```text
Environment ID:            E001 (the owner's development machine)
App commit and profile:    yappyink 0.0.0, cargo dev profile, uncommitted tree
Date and tester:           2026-09-23, Rajesh Pillai's machine, probe run by the coding agent
OS / architecture:         Ubuntu 24.04.4 LTS, Linux 7.0.0-31-generic, x86_64
Desktop / compositor:      GNOME Shell 46.0, mutter-common 46.2-1ubuntu0.24.04.16
Protocol versions:         see the advertised-globals list in E001-doctor-output.txt
GPU / driver:              not recorded (no renderer exists yet; required from T014)
Output layout / scaling:   eDP-1 1366x768 @ 60.059 Hz, HDMI-1 1920x1080 @ 100.000 Hz,
                           both integer scale 1, both transform Normal
Native or nested session:  native (XDG_SESSION_TYPE=wayland, socket /run/user/1001/wayland-0)
Backend / permissions:     wayland; no permission was requested and none was needed
Scenario IDs executed:     partial AC-FR-013, partial AC-NFR-005
Result:                    pass for what was probed; everything else not tested
```

## Commands

```sh
cargo run -p yappyink -- doctor        # full output in E001-doctor-output.txt
gnome-shell --version
dpkg -s mutter-common | grep -i Version
lsb_release -ds && uname -srm
```

## The finding that matters

**Mutter advertises 28 globals and `zwlr_layer_shell_v1` is not among them.**

The advertised shell protocol is `xdg_wm_base` v6. This is the first direct
measurement of the risk that `platform-matrix.md` and ADR-002 were written
around, on the machine the product is being built on. It confirms that the
layer-shell route from the Wayland row of the backend plan does not exist here,
and that T007's standalone xdg-shell experiment is the next real question, not an
optional branch.

It does **not** show that a GNOME overlay is impossible. Nothing was attempted.
It narrows which route is worth attempting first.

Globals that may matter later, recorded now so the T006/T007 work does not have
to rediscover them:

| Interface | Version | Why it was noted |
|---|---|---|
| `xdg_wm_base` | 6 | the only shell protocol available; the T007 experiment's starting point |
| `wp_fractional_scale_manager_v1` | 1 | fractional scale is per surface; FR-012 and T018 need it |
| `wp_viewporter` | 1 | usually paired with fractional scaling |
| `zwp_keyboard_shortcuts_inhibit_manager_v1` | 1 | worth examining for FR-005 keyboard policy; inhibiting is not the same as a global shortcut |
| `zxdg_output_manager_v1` | 3 | logical output geometry, which `wl_output` alone does not give |
| `gtk_shell1` | 5 | a GNOME-specific private protocol; not a supported integration route |
| `wl_output` | 4 | version 4 is why output names arrived at all |

## What this does not establish

Nothing about drawing. No surface was created, so this run says nothing about
overlay stacking, transparency, hit testing on blank pixels, pass-through,
keyboard release, or withdrawal. Every one of those is `unknown` in the report,
each with the task that would settle it.

Also untested: the GlobalShortcuts portal (no D-Bus client is chosen yet), X11
and XWayland (the native socket answered first), Windows and macOS (this code has
never run there), fractional scaling in practice, hotplug, and capture.

Runtime observation is not certification. `output_enumeration` is `available`
because two outputs were actually enumerated over the protocol on this machine
today; that is a fact about this session, not a supported-platform claim.
