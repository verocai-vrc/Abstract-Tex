#!/usr/bin/env bash
# Run the app on a virtual display and leave it running, so it can be driven and photographed with
# scripts/xvfb-ui.py. Needs Xvfb (`apt install xvfb`); everything else is already there.
#
# Usage: scripts/xvfb-app.sh [project folder]      (restarts the app if it is already running)
#        DISPLAY_NUMBER=98 SCREEN=1700x1100x24 scripts/xvfb-app.sh fixtures/paper
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

# `scripts/dev.sh` cleans the VS Code snap's GTK variables, which break the app's startup.
if [ -n "${1:-}" ]; then ABSTRACT_TEX_OPEN="$(cd "$1" && pwd)"; export ABSTRACT_TEX_OPEN; fi
nohup scripts/dev.sh >"$log" 2>&1 &

# The window title is "Abstract-Tex"; the page needs a few more seconds after it maps.
timeout 180 bash -c "until xwininfo -root -tree 2>/dev/null | grep -q '\"Abstract-Tex\"'; do sleep 2; done" \
  && sleep 8 && echo "app is up on $DISPLAY (log: $log)"
