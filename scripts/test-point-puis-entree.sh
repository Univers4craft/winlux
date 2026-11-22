#!/bin/sh
# #3072, sous Xvfb : « a. », Entrée, « q » arrivés ensemble.
# La zone lit Entrée au WM_KEYDOWN et les caractères au WM_CHAR (Scintilla).
# Attendu « a.| » (code 7). Avant #3072, le WM_CHAR du « . » passait derrière
# Entrée déjà en file : « a|. » (code 3), le point de Notepad++ après Entrée.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
cc=${WINLUX_MINGW_CC:-x86_64-w64-mingw32-gcc}
for t in "$cc" xdotool xwininfo; do
    command -v "$t" >/dev/null || { echo "$t absent : test sauté"; exit 0; }
done
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
test -n "${DISPLAY:-}" || { echo "lancer sous xvfb-run"; exit 2; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$cc" -O1 -mwindows -o "$work/point.exe" "$root/testc/frappe/point_puis_entree.c" -luser32
(cd "$work" && exec "$winlux" point.exe) >"$work/out.txt" 2>&1 &
pid=$!
n=0
until xwininfo -root -tree 2>/dev/null | grep -q '"PointEntree3072"'; do
    n=$((n + 1))
    if [ "$n" -gt 150 ] || ! kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        echo "ÉCHEC : fenêtre jamais affichée" >&2; cat "$work/out.txt" >&2; exit 1
    fi
    sleep 0.2
done
sleep 0.5
win=$(xdotool search --name '^PointEntree3072$' | tail -n 1)
xdotool windowfocus --sync "$win" 2>/dev/null || true
xdotool type --delay 30 a.
xdotool key Return
xdotool type q
code=0
n=0
while kill -0 "$pid" 2>/dev/null; do
    n=$((n + 1))
    if [ "$n" -gt 50 ]; then
        kill "$pid" 2>/dev/null || true
        echo "ÉCHEC : « q » jamais reçu ; ordre « $(cat "$work/ordre.txt" 2>/dev/null) »" >&2; cat "$work/out.txt" >&2; exit 1
    fi
    sleep 0.2
done
wait "$pid" || code=$?
if [ "$code" != 7 ]; then
    echo "ordre reçu : « $(cat "$work/ordre.txt" 2>/dev/null) »" >&2
    echo "ÉCHEC : code $code au lieu de 7 (le « . » arrive après Entrée)" >&2
    cat "$work/out.txt" >&2; exit 1
fi
echo "« a. » puis Entrée déjà en file : « a.| », le point passe avant Entrée (#3072)"
