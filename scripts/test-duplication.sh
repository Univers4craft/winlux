#!/bin/sh
# DuplicateHandle et mémoire entre processus .exe (bac à sable de Chromium,
# Mojo d'Electron, #236) : compile testc/msvc/dupx_msvc.c avec clang (cible
# MSVC) si besoin, puis le lance sous Winlux. Le programme se relance
# lui-même suspendu, lit le PEB et l'image de l'enfant, y copie un mapping,
# écrit dans ses variables, puis l'enfant reprend un événement du parent.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/dupx_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/dupx_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home" "$work/run"
chmod 700 "$work/run"
set +e
HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_GUI=headless \
XDG_RUNTIME_DIR="$work/run" timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
rc=$?
set -e
if test "$rc" -ne 0; then
    cat "$work/out.txt"
    echo "duplication : échec, code $rc (voir testc/msvc/dupx_msvc.c)" >&2
    exit 1
fi
left=$(find "$work/run/winlux-handles" -type s 2>/dev/null | wc -l)
test "$left" -eq 0 || { echo "duplication : $left guichets de handles laissés" >&2; exit 1; }
echo "duplication : enfant suspendu lu et écrit, mapping et événement copiés entre processus"
