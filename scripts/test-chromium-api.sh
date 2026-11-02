#!/bin/sh
# Fonctions que Chromium et Electron résolvent au démarrage (#236) :
# compile testc/msvc/chromium_msvc.c avec clang (cible MSVC) si besoin, puis
# le lance sous Winlux. Atténuations, descripteurs de sécurité, veille,
# contextes de DPI, shcore par son contrat d'API, contrat downlevel de
# shell32 et oleaut32 par ordinal (#162).
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/chromium_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/chromium_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
set +e
HOME="$work" WINLUX_HOME="$work" WINLUX_DRIVE_C="$work" WINLUX_GUI=headless \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
rc=$?
set -e
if test "$rc" -ne 0; then
    cat "$work/out.txt"
    echo "api-chromium : échec, code $rc (voir testc/msvc/chromium_msvc.c)" >&2
    exit 1
fi
echo "api-chromium : atténuations, sécurité, DPI, shcore, oleaut32 #162"
