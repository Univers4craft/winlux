#!/bin/sh
# Exécute sous Winlux les tests C compilés par scripts/build-mingw-tests.sh.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
dir=${WINLUX_MINGW_DIR:-$root/target/mingw}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
test -f "$dir/mingw_suite.exe" || { echo "lancez d'abord scripts/build-mingw-tests.sh" >&2; exit 1; }
cd "$work"
set +e
WINLUX_GUI=headless "$runtime" "$dir/mingw_suite.exe" un "deux mots" >"$work/sortie.txt" 2>&1
status=$?
set -e
cat "$work/sortie.txt"
test "$status" -eq 0 || { echo "mingw_suite.exe : code $status" >&2; exit 1; }
grep -q '^atexit exécuté$' "$work/sortie.txt" || { echo "les fonctions atexit n'ont pas été exécutées" >&2; exit 1; }
echo "Tests MinGW réussis."
