#!/usr/bin/env bash
# Run the app on a virtual display and leave it running, so it can be driven and photographed with
# scripts/xvfb-ui.py. Needs Xvfb (`apt install xvfb`); everything else is already there.
#
# Usage: scripts/xvfb-app.sh [project folder]      (restarts the app if it is already running)
#        DISPLAY_NUMBER=98 SCREEN=1700x1100x24 scripts/xvfb-app.sh fixtures/paper
#
# The walk never touches your own settings or documents: the app's config and data go under
# $TMPDIR/abstract-tex-walk, and so do Documents, Desktop, Downloads and the rest. That folder is kept
# between runs, so that a second launch finds the recent documents of the first; FRESH=1 empties it.
# XDG_CONFIG_HOME, XDG_DATA_HOME and XDG_CACHE_HOME can be set to go elsewhere; the cache (the TeX
# packages already fetched) is otherwise your own.
#
# Copy a fixture first if the walk will edit it: the app saves into the folder it opens.
# Log: $TMPDIR/abstract-tex-xvfb.log. Stop with: pkill -f 'target/debug/abstract-te[x]'; pkill Xvfb

set -u
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
number="${DISPLAY_NUMBER:-99}"
export DISPLAY=":$number" # for the app and Xvfb only; scripts/xvfb-ui.py reads XVFB_DISPLAY
log="${TMPDIR:-/tmp}/abstract-tex-xvfb.log"

pgrep -f "Xvfb :$number" >/dev/null || { Xvfb ":$number" -screen 0 "${SCREEN:-1600x1100x24}" >/dev/null 2>&1 & sleep 1; }

pkill -f "target/debug/abstract-te[x]"; pkill -f "tauri de[v]"; pkill -f "node.*vit[e]"; sleep 1

walk="${TMPDIR:-/tmp}/abstract-tex-walk"
if [ "${FRESH:-}" = 1 ]; then rm -rf "$walk"; fi
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$walk/config}" XDG_DATA_HOME="${XDG_DATA_HOME:-$walk/data}"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
# Where the system says Documents and the others are. Without this file the app would make new documents
# in the real home folder. Written only into a config folder of the walk's own.
if [ "$XDG_CONFIG_HOME" = "$walk/config" ]; then
  : >"$XDG_CONFIG_HOME/user-dirs.dirs"
  for folder in Documents Desktop Download Pictures Music Videos Publicshare; do
    mkdir -p "$walk/home/$folder"
    echo "XDG_$(echo "$folder" | tr '[:lower:]' '[:upper:]')_DIR=\"$walk/home/$folder\"" >>"$XDG_CONFIG_HOME/user-dirs.dirs"
  done
fi

# `scripts/dev.sh` cleans the VS Code snap's GTK variables, which break the app's startup.
if [ -n "${1:-}" ]; then ABSTRACT_TEX_OPEN="$(cd "$1" && pwd)"; export ABSTRACT_TEX_OPEN; fi
# The window must open on the virtual display, never on a real Wayland desktop.
unset WAYLAND_DISPLAY; export GDK_BACKEND=x11
nohup scripts/dev.sh >"$log" 2>&1 &

# The window title is "Abstract-Tex", or ends in it once a document is open; the page needs a few more
# seconds after it maps.
timeout 180 bash -c "until xwininfo -root -tree 2>/dev/null | grep -q 'Abstract-Tex\"'; do sleep 2; done" \
  && sleep 8 && echo "app is up on $DISPLAY (log: $log)"
