#!/bin/sh
# Héritage des handles entre processus .exe (crashpad de Chromium et
# d'Electron) : compile testc/msvc/inherit_msvc.c avec clang (cible MSVC) si
# besoin, puis le lance sous Winlux. Le programme se relance lui-même avec
# des handles hérités (événements, première instance d'un tube nommé, tube
# anonyme, handle vers le parent), restreints par
# PROC_THREAD_ATTRIBUTE_HANDLE_LIST ; les deux processus se signalent, se
# connectent par le nom du tube et échangent des données. Vérifie aussi que
# le dossier des rendez-vous de tubes n'est pas laissé plein.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/inherit_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/inherit_msvc.c" -nt "$exe"; then
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
    echo "heritage : échec, code $rc (voir testc/msvc/inherit_msvc.c)" >&2
    exit 1
fi
left=$(find "$work/run/winlux-tubes" -type s 2>/dev/null | wc -l)
test "$left" -eq 0 || { echo "heritage : $left rendez-vous de tube laissés" >&2; exit 1; }
echo "heritage : événements, tube nommé et tube anonyme hérités, connexion entre processus"
