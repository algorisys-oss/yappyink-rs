// yappyink overlay support.
//
// ADR-002 outcome (b): the GNOME Shell companion. Measured in
// docs/evidence/E002 and E003, a plain Wayland client on Mutter can already
// draw, take input over transparent pixels, pass input through with an empty
// input region, and withdraw cleanly. What it cannot do is stay above other
// windows or choose where it appears, and no protocol lets it.
//
// So this extension does exactly those two things and nothing else. It is
// deliberately small: every line here is a line that has to keep working
// across GNOME Shell releases, and the drawing engine stays in Rust where it
// can be tested.
//
// It touches only windows belonging to yappyink, and it undoes what it did
// when disabled.

import Meta from 'gi://Meta';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

/// The app id the overlay sets on its xdg-toplevel. Mutter surfaces it as the
/// window's wm_class for Wayland clients.
const APP_ID = 'dev.yappyink.Overlay';

/// Fallback match. Only used when wm_class is not yet set, which happens for a
/// moment after a window is created.
const TITLE_PREFIX = 'yappyink';

/// Everything this extension logs is prefixed, so it can be told apart from
/// GNOME Shell's own considerable output:
///
///     journalctl --user -f -o cat /usr/bin/gnome-shell | grep yappyink
///
/// It logs on enable, on disable, and on every window it takes charge of.
/// Silence would be ambiguous: an extension that did nothing and one that was
/// never loaded look identical in a log.
const LOG = '[yappyink]';

export default class YappyinkOverlaySupport extends Extension {
    enable() {
        console.log(`${LOG} enabled, watching for windows with app id ${APP_ID}`);

        // Windows we have changed, so disable() can put them back. A Set of
        // Meta.Window; entries are dropped when the window is unmanaged.
        this._managed = new Set();
        this._pending = new Map();

        this._createdId = global.display.connect('window-created', (_display, window) => {
            this._adopt(window);
        });

        // Windows that already exist when the extension is enabled, which is
        // the normal case when enabling it while the overlay is running.
        for (const actor of global.get_window_actors())
            this._adopt(actor.meta_window);
    }

    disable() {
        if (this._createdId) {
            global.display.disconnect(this._createdId);
            this._createdId = null;
        }

        for (const [window, id] of this._pending)
            window.disconnect(id);
        this._pending.clear();

        // Put back what we changed. An extension that leaves windows altered
        // after being disabled is indistinguishable from a bug.
        const count = this._managed.size;
        for (const window of this._managed) {
            try {
                window.unmake_above();
                window.unstick();
            } catch (_error) {
                // The window is already gone. Nothing to restore.
            }
        }
        console.log(`${LOG} disabled, ${count} window(s) put back`);
        this._managed.clear();
    }

    _isOverlay(window) {
        if (!window || window.get_window_type() !== Meta.WindowType.NORMAL)
            return false;

        const wmClass = window.get_wm_class();
        if (wmClass === APP_ID)
            return true;

        // wm_class can be null for an instant after creation. The title is a
        // weaker signal, so it is only trusted while wm_class is absent.
        return !wmClass && (window.get_title() ?? '').startsWith(TITLE_PREFIX);
    }

    _adopt(window) {
        if (!window || this._managed.has(window) || this._pending.has(window))
            return;
        if (window.get_window_type() !== Meta.WindowType.NORMAL)
            return;

        // A window that has already said who it is and is not the overlay is
        // left alone at once.
        const identified = !!window.get_wm_class();
        if (identified && !this._isOverlay(window))
            return;

        if (identified && window.get_compositor_private()) {
            this._apply(window);
            return;
        }

        // Otherwise wait until it is shown, and decide then. For a Wayland
        // client `window-created` fires when the toplevel is created, before
        // its app id and title requests are processed, so both are empty at
        // that instant. The first version decided there and then, saw an
        // anonymous window, and never looked again: in hours of being enabled
        // on the E001 machine it never took charge of a single overlay.
        // Waiting for 'shown' also avoids a window that flickers into place.
        const id = window.connect('shown', () => {
            window.disconnect(id);
            this._pending.delete(window);
            if (this._isOverlay(window))
                this._apply(window);
        });
        this._pending.set(window, id);
    }

    _apply(window) {
        // Above every ordinary window. This is the whole reason the extension
        // exists: Mutter offers a client no way to ask for it, and without it
        // the ink is covered the moment the user clicks anything.
        window.make_above();

        // On every workspace. An annotation over a screen should not vanish
        // because the presenter switched desktops.
        window.stick();

        // Fill the monitor the window is on. Not `fullscreen`: E002 finding 1
        // measured that a fullscreen surface on Mutter is unredirected and
        // loses its transparency, which is fatal for an overlay. An ordinary
        // window merely sized to the monitor is the thing being tested here.
        const monitor = window.get_monitor();
        const geometry = global.display.get_monitor_geometry(monitor);
        window.move_resize_frame(
            false,
            geometry.x,
            geometry.y,
            geometry.width,
            geometry.height
        );

        this._managed.add(window);
        console.log(
            `${LOG} took charge of "${window.get_title()}" (wm_class ${window.get_wm_class()}): ` +
            `above, sticky, and sized to monitor ${monitor} at ` +
            `${geometry.width}x${geometry.height}+${geometry.x}+${geometry.y}`
        );

        const unmanagedId = window.connect('unmanaged', () => {
            window.disconnect(unmanagedId);
            this._managed.delete(window);
        });
    }
}
