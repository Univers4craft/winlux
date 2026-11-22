#!/bin/sh
# #2992 : un contrôle enfant mis en hauteur 0 sous un vrai serveur X (Xvfb).
# La fenêtre X ne descend pas sous 1x1 ; GetClientRect doit pourtant rendre
# la hauteur 0 demandée, et la fenêtre ne doit recevoir aucun WM_PAINT à
# zone client vide. À lancer sous xvfb-run.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_RUNTIME:-$root/target/release/winlux}
exe=$root/target/x86_64-pc-windows-msvc/release/libelle_vide_suite.exe
test -n "${DISPLAY:-}" || { echo "DISPLAY absent : à lancer sous xvfb-run" >&2; exit 1; }
NO_AT_BRIDGE=1 timeout 60 "$runtime" "$exe"
