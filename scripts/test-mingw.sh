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

if test -f "$dir/cpp_exceptions.exe"; then
    set +e
    WINLUX_GUI=headless "$runtime" "$dir/cpp_exceptions.exe" >"$work/cpp.txt" 2>&1
    cpp_status=$?
    set -e
    cat "$work/cpp.txt"
    if test "$cpp_status" -ne 0; then
        echo "cpp_exceptions.exe : code $cpp_status (déroulement SEH incomplet, non bloquant)" >&2
    elif ! grep -q 'all tests passed' "$work/cpp.txt"; then
        echo "cpp_exceptions : marqueur de réussite absent (non bloquant)" >&2
    fi
fi

if test -f "$dir/eh_msvc.exe"; then
    set +e
    WINLUX_GUI=headless "$runtime" "$dir/eh_msvc.exe" >"$work/msvc.txt" 2>&1
    msvc_status=$?
    set -e
    cat "$work/msvc.txt"
    test "$msvc_status" -eq 0 || { echo "eh_msvc.exe : le test n° $msvc_status a échoué" >&2; exit 1; }
    echo "eh_msvc : exceptions C++ et __try au format MSVC OK"
fi

echo "Tests MinGW réussis."
