#!/usr/bin/env bash
# T037 probe: does the GNOME Shell magnifier give yappyink live zoom?
#
# Throwaway, like the other experiments. It changes two of the user's own
# accessibility settings, so it records them first and restores them on every
# way out, including Ctrl+C. If it is killed with SIGKILL, Alt+Super+8 turns the
# magnifier off.
#
# Usage: experiments/gnome-magnifier/probe.sh [binary] [zoom seconds] [factor]
# Questions it exists to answer are in README.md next to it.
set -euo pipefail

BIN=${1:-./target/release/yappyink}
SECS=${2:-45}
FACTOR=${3:-2.0}
LOG=${LOG:-/tmp/yappyink-zoom-probe.log}
# Seconds the overlay stays up after zoom turns off, to check where strokes are.
POST=${POST:-20}
A=org.gnome.desktop.a11y.applications
M=org.gnome.desktop.a11y.magnifier

was_on=$(gsettings get $A screen-magnifier-enabled)
was_factor=$(gsettings get $M mag-factor)
echo "[probe] saved: screen-magnifier-enabled=$was_on mag-factor=$was_factor"

pid=
restore() {
    gsettings set $A screen-magnifier-enabled "$was_on"
    gsettings set $M mag-factor "$was_factor"
    echo "[probe] restored: $(gsettings get $A screen-magnifier-enabled) $(gsettings get $M mag-factor)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        "$BIN" quit >/dev/null 2>&1 || kill "$pid"
    fi
}
trap restore EXIT INT TERM

say() { echo "[probe] $1"; notify-send -t 8000 "yappyink zoom probe" "$1" || true; }

"$BIN" >"$LOG" 2>&1 &
pid=$!
say "Overlay starting. Zoom turns on in 8 seconds."
sleep 8

started=$(date +%s.%N)
gsettings set $M mag-factor "$FACTOR"
gsettings set $A screen-magnifier-enabled true
say "Zoom ON at ${FACTOR}x for ${SECS}s. Move the pointer, and DRAW inside the overlay."
sleep "$SECS"

gsettings set $A screen-magnifier-enabled "$was_on"
say "Zoom OFF. Check your strokes are still on what they marked. Quitting in ${POST}s."
sleep "$POST"

"$BIN" quit >/dev/null 2>&1 || true
wait "$pid" || true
pid=
echo "[probe] zoom was on from $started for ${SECS}s; overlay log: $LOG"
echo "[probe] faults in the log: $(grep -c '^\[fault' "$LOG" || true)"
