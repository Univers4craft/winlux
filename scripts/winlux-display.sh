#!/bin/sh
# Écrans : la politique (disposition, échelle par écran, branchement à
# chaud, capot) est dans le runtime Winlux (runtime/src/display) ; xrandr
# reste l'outil de plomberie. Arguments validés ici puis par le runtime.
set -eu
umask 077
export DISPLAY=${DISPLAY:-:0}
for arg in "$@"; do
    case "$arg" in *[!A-Za-z0-9._:x-]*|'') echo "Argument invalide : $arg" >&2; exit 2 ;; esac
done
exec winlux --display "$@"
