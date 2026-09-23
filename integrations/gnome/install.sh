#!/usr/bin/env bash
# Installs the yappyink GNOME Shell extension for the current user.
#
# It copies the extension into the per-user extensions directory and enables
# it. Nothing is installed system-wide and nothing needs root.
set -euo pipefail

UUID="yappyink@algorisys-oss.github.io"
SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$UUID"
TARGET="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"

if [ ! -d "$SOURCE" ]; then
    echo "error: $SOURCE is missing" >&2
    exit 1
fi

echo "installing $UUID"
mkdir -p "$(dirname "$TARGET")"
rm -rf "$TARGET"
cp -r "$SOURCE" "$TARGET"

if ! command -v gnome-extensions >/dev/null; then
    echo "gnome-extensions is not on PATH; enable it yourself with the Extensions app" >&2
    exit 0
fi

# A newly installed extension can usually be enabled without logging out.
# Changes to one that GNOME Shell has already loaded cannot: on Wayland the
# Shell cannot be restarted, so editing this code means logging out and back in.
gnome-extensions enable "$UUID" 2>/dev/null || {
    echo
    echo "could not enable it yet. Log out and back in, then run:"
    echo "    gnome-extensions enable $UUID"
    exit 0
}

echo "enabled. Check it with:"
echo "    gnome-extensions info $UUID"
