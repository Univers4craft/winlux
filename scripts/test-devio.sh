#!/bin/sh
# DeviceIoControl (#271) : compile testc/msvc/devio_msvc.c avec clang (cible
# MSVC) si besoin, puis vérifie sous Winlux, sur le vrai système :
#   - \\.\C: : taille, géométrie, fiche du disque (en-tête puis complète),
#     SSD ou non, numéro de disque, verrouillage refusé, code inconnu ;
#   - un fichier creux : FSCTL_SET_SPARSE, QUERY_ALLOCATED_RANGES, SET_ZERO_DATA ;
#   - \\.\PhysicalDrive0 quand la machine a un disque.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/devio_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/devio_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
set +e
WINLUX_GUI=headless timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "devio_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "devio_msvc : volume, disque et fichier creux interrogés par DeviceIoControl"
