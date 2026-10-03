#!/usr/bin/env bash
# Starts `pnpm tauri dev` from a shell that was opened inside the VS Code snap.
#
# Why this exists: the snap sets GTK_PATH, GIO_MODULE_DIR, GDK_PIXBUF_* and friends so *its own*
# GTK finds *its own* modules. A child process inherits them, and the app's GTK then loads the
# snap's older libc-era modules and dies at startup with
#   symbol lookup error: /snap/core20/.../libpthread.so.0: undefined symbol: __libc_pthread_init
# (bugs-issues-fixes.md, "pnpm tauri dev fails"). A terminal opened outside the snap has none of
# this and does not need the script; outside a snap it changes nothing and just runs the command.
#
# Usage: scripts/dev.sh [extra args for `pnpm tauri dev`]
#        ABSTRACT_TEX_OPEN="$PWD/fixtures/paper" scripts/dev.sh

# Variables the snap points into its own tree. Unsetting them makes GTK fall back to the system's.
SNAP_GTK_VARIABLES=(
  GTK_PATH
  GTK_EXE_PREFIX
  GTK_IM_MODULE_FILE
  GIO_MODULE_DIR
  GDK_PIXBUF_MODULEDIR
  GDK_PIXBUF_MODULE_FILE
  GSETTINGS_SCHEMA_DIR
  LOCPATH
)

# Variables the snap *rewrote* rather than added, and kept the original of under this suffix.
SNAP_ORIGINAL_SUFFIX="_VSCODE_SNAP_ORIG"
SNAP_REWRITTEN_VARIABLES=(XDG_DATA_DIRS XDG_CONFIG_DIRS)

clean_snap_environment() {
  # `SNAP_NAME` is set for everything started from a snap, and for nothing else.
  [ -n "${SNAP_NAME:-}" ] || return 0

  local variable original
  for variable in "${SNAP_GTK_VARIABLES[@]}"; do
    unset "$variable"
  done
  for variable in "${SNAP_REWRITTEN_VARIABLES[@]}"; do
    original="${variable}${SNAP_ORIGINAL_SUFFIX}"
    # `${!original}` reads the variable whose *name* is held in `original`.
    if [ -n "${!original:-}" ]; then
      export "$variable=${!original}"
    fi
    unset "$original"
  done
}

# Run only when executed, not when sourced, so the cleaning can be tried on its own:
#   source scripts/dev.sh && clean_snap_environment && env | grep -c snap
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  # Work from the repository root whichever folder this was started in.
  cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
  clean_snap_environment
  exec pnpm tauri dev "$@"
fi
