#!/bin/sh
# Runs a command with a display to open its window on. A terminal started by a
# daemon or over SSH has neither WAYLAND_DISPLAY nor DISPLAY, and GPUI then runs
# headless: no window and no error. Values already set are left alone.
# Usage: scripts/with-display.sh COMMAND [ARGS...]
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
    for socket in "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"/wayland-*; do
        if [ -S "$socket" ]; then
            WAYLAND_DISPLAY=${socket##*/}
            export WAYLAND_DISPLAY
            break
        fi
    done
fi
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
    for socket in /tmp/.X11-unix/X*; do
        if [ -S "$socket" ]; then
            DISPLAY=:${socket##*/X}
            export DISPLAY
            break
        fi
    done
fi
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
    echo "No display found: set WAYLAND_DISPLAY or DISPLAY." >&2
    exit 1
fi
exec "$@"
