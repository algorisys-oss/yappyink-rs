# Verified source notes

Checked on 22 September 2026. Documentation may evolve. These sources support platform facts; they do not certify this unimplemented application. Numerical product targets and architecture choices are proposals.

## S01: Wayscriber requirements and GNOME limitations

Shows current project scope: Wayland, layer-shell environments, and limited GNOME fallback; not evidence that our app works.

URL: `https://wayscriber.com/docs/`

## S02: Wayscriber source and platform matrix

Reference implementation to study; not a promise of portable backend reuse.

URL: `https://github.com/devmobasa/wayscriber`

## S03: winit WindowLevel

Window levels are unsupported by the generic winit Wayland backend.

URL: `https://docs.rs/winit/latest/winit/window/enum.WindowLevel.html`

## S04: winit Window

Transparency/hit-test APIs and platform limits; native behavior still requires validation.

URL: `https://docs.rs/winit/latest/winit/window/struct.Window.html`

## S05: Microsoft Window Features

Layered-window alpha hit testing and WS_EX_TRANSPARENT behavior; zero-alpha Draw-mode capture is a specific integration risk.

URL: `https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features`

## S06: Apple NSWindow ignoresMouseEvents

Native pointer pass-through property.

URL: `https://developer.apple.com/documentation/appkit/nswindow/ignoresmouseevents`

## S07: Apple NSWindow fullScreenAuxiliary

One of the collection-behavior primitives to evaluate; not a universal fullscreen guarantee.

URL: `https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct/fullscreenauxiliary`

## S08: Wayland wlr-layer-shell protocol

Protocol text for layer surfaces, keyboard interactivity, and pointer input-region behavior.

URL: `https://wayland.app/protocols/wlr-layer-shell-unstable-v1`

## S09: GTK4 layer-shell supported desktops

Documents GNOME Wayland as not supporting this layer-shell integration.

URL: `https://github.com/wmww/gtk4-layer-shell`

## S10: global-hotkey Rust crate

Windows/macOS/X11 support and event-loop restrictions; not a Wayland shortcut solution.

URL: `https://docs.rs/global-hotkey`

## S11: XDG GlobalShortcuts portal

Session-based global shortcut registration and activation independent of focused app.

URL: `https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html`

## S12: Smithay client-toolkit layer example

Concrete client-side layer-shell starting point. Use client-toolkit, not a compositor server.

URL: `https://github.com/Smithay/client-toolkit/blob/master/examples/simple_layer.rs`

## S13: egui project and integration documentation

UI library and custom integration model; does not provide universal desktop overlay privileges.

URL: `https://github.com/emilk/egui`

## S14: wgpu project

Portable graphics API proposal; GPU rendering does not grant window stacking rights.

URL: `https://wgpu.rs/`

## S15: EWMH application window properties

X11 window-manager hints including stacking-related state.

URL: `https://specifications.freedesktop.org/wm/latest/ar01s05.html`

## S16: XFixes protocol

Region and input-shape operations relevant to X11 pass-through.

URL: `https://xorg.freedesktop.org/archive/current/doc/fixesproto/fixesproto.txt`

## S17: XDG Screenshot portal

Optional later screenshot capability, distinct from overlay rendering.

URL: `https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html`

## S18: XDG ScreenCast portal

Optional later stream capture with its own session and selection lifecycle.

URL: `https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html`

## S19: Apple ScreenCaptureKit

Optional later capture API; verify deployment target and permission UX when implementing.

URL: `https://developer.apple.com/documentation/screencapturekit`

## S20: GNOME Shell extension development

A potential GNOME integration requires a separately maintained Shell extension, conventionally JavaScript/GJS.

URL: `https://gjs.guide/extensions/`
