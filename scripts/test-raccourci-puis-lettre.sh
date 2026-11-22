#!/bin/sh
# #2870, sous Xvfb : Ctrl+N pris par TranslateAccelerator, puis « nnnq ».
# Le programme rend 10 × (« n » reçus) + (commandes Ctrl+N) : 31 attendu.
# Avant #2870, le « sans caractère » de Ctrl+N restait en file et volait le
# caractère du premier « n » : 21 (la lettre était perdue, Notepad++).
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
"$cc" -O1 -mwindows -o "$work/raccourci.exe" "$root/testc/frappe/raccourci_puis_lettre.c" -luser32
(cd "$work" && exec "$winlux" raccourci.exe) >"$work/out.txt" 2>&1 &
pid=$!
n=0
until xwininfo -root -tree 2>/dev/null | grep -q '"Raccourci2870"'; do
    n=$((n + 1))
    if [ "$n" -gt 150 ] || ! kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        echo "ÉCHEC : fenêtre jamais affichée" >&2; cat "$work/out.txt" >&2; exit 1
    fi
    sleep 0.2
done
sleep 0.5
win=$(xdotool search --name '^Raccourci2870$' | tail -n 1)
xdotool windowfocus --sync "$win" 2>/dev/null || true
xdotool key ctrl+n
sleep 0.3
xdotool type --delay 30 nnnq
code=0
n=0
while kill -0 "$pid" 2>/dev/null; do
    n=$((n + 1))
    if [ "$n" -gt 50 ]; then
        kill "$pid" 2>/dev/null || true
        echo "ÉCHEC : « q » jamais reçu" >&2; cat "$work/out.txt" >&2; exit 1
    fi
    sleep 0.2
done
wait "$pid" || code=$?
if [ "$code" != 31 ]; then
    echo "ÉCHEC : code $code au lieu de 31 (une lettre perdue après le raccourci Ctrl+N)" >&2
    cat "$work/out.txt" >&2; exit 1
fi
echo "Ctrl+N pris par un accélérateur, puis « nnn » : aucune lettre perdue (#2870)"
