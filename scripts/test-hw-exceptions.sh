#!/bin/sh
# Fautes matérielles en exceptions SEH (#269) : compile testc/msvc/hwexc_msvc.c
# avec clang (cible MSVC) si besoin, puis vérifie sous Winlux
#   - que les douze cas (__try autour d'un accès à NULL, division par zéro,
#     int3, ud2, pas à pas, page de garde, débordement de pile (thread secondaire et
#     thread principal, deux fois de suite), VEH qui
#     corrige le contexte…) sont rattrapés par le programme ;
#   - qu'un débordement de pile non rattrapé, sur le thread principal comme
#     sur un thread secondaire, finit au rapport EXCEPTION_STACK_OVERFLOW
#     (code 0xC00000FD, octet bas 253) et non en faute dans Winlux (#15) ;
#   - qu'un accès à NULL non rattrapé finit bien au rapport de plantage
#     (code 0xC0000005, dont Linux garde l'octet bas : 5).
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/hwexc_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/hwexc_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

set +e
WINLUX_GUI=headless timeout 60 "$runtime" "$exe" >"$work/ok.txt" 2>&1
status=$?
WINLUX_GUI=headless WINLUX_CRASH_DIR="$work/crash" timeout 60 "$runtime" "$exe" crash >"$work/crash.txt" 2>&1
crash=$?
set -e

test "$status" -eq 0 || {
    cat "$work/ok.txt"
    echo "hwexc_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "hwexc_msvc : 12 fautes matérielles rattrapées par __try et VEH"
test "$crash" -eq 5 || {
    cat "$work/crash.txt"
    echo "hwexc_msvc.exe crash : code $crash, 5 attendu (0xC0000005)" >&2
    exit 1
}
grep -q 'EXCEPTION_ACCESS_VIOLATION' "$work/crash.txt" || {
    cat "$work/crash.txt"
    echo "hwexc_msvc.exe crash : rapport de plantage absent" >&2
    exit 1
}
echo "hwexc_msvc : faute non rattrapée -> rapport de plantage"
for where in overflow overflow-thread; do
    set +e
    WINLUX_GUI=headless WINLUX_CRASH_DIR="$work/crash-$where" timeout 60 "$runtime" "$exe" "$where" >"$work/$where.txt" 2>&1
    code=$?
    set -e
    if test "$code" -ne 253 || ! grep -q 'EXCEPTION_STACK_OVERFLOW' "$work/$where.txt"; then
        cat "$work/$where.txt"
        echo "hwexc_msvc.exe $where : code $code, 253 (0xC00000FD) et EXCEPTION_STACK_OVERFLOW attendus" >&2
        exit 1
    fi
done
echo "hwexc_msvc : débordement de pile non rattrapé (thread principal et secondaire) -> rapport EXCEPTION_STACK_OVERFLOW"
