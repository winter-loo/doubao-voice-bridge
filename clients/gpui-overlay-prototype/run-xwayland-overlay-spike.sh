#!/usr/bin/env bash

# PROTOTYPE: force GPUI through XWayland to test notification-window placement,
# stacking, focus preservation, and click-to-close behavior on GNOME.
set -euo pipefail

unset WAYLAND_DISPLAY
export DISPLAY="${DISPLAY:-:0}"
export DOUBAO_XWAYLAND_OVERLAY_SPIKE=1

# Mutter keeps its XWayland cookie in the user's runtime directory. A shell
# launched inside GNOME already has XAUTHORITY; plain SSH sessions usually do not.
if [[ -z "${XAUTHORITY:-}" ]]; then
    runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    for candidate in "$runtime_dir"/.mutter-Xwaylandauth.*; do
        if [[ -r "$candidate" ]]; then
            export XAUTHORITY="$candidate"
            break
        fi
    done
fi

exec cargo run --bin DoubaoVoiceClient
