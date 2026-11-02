#!/bin/sh
# Entrée brute (#271) : compile testc/msvc/rawinput_msvc.c avec clang (cible
# MSVC) si besoin, puis le lance sous Winlux avec une fausse souris et un faux
# clavier (deux FIFO à la place de /dev/input/event*) :
#   - RegisterRawInputDevices sur une fenêtre, liste et noms des appareils ;
#   - déplacement relatif + bouton, molette, touche W : un WM_INPUT chacun,
#     lu par GetRawInputData ;
#   - plus rien après RIDEV_REMOVE.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/rawinput_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/rawinput_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkfifo "$work/souris.fifo" "$work/clavier.fifo"
cd "$work"
set +e
WINLUX_GUI=headless WINLUX_RAWINPUT_DEVICES="mouse=$work/souris.fifo;keyboard=$work/clavier.fifo" \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "rawinput_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "rawinput_msvc : souris relative, molette et clavier reçus en WM_INPUT"
