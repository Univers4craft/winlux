#!/bin/sh
# MessageBox, bouton par défaut au clavier (#2754), sous Xvfb : deux vraies
# boîtes MessageBoxW (MB_DEFBUTTON2 puis MB_DEFBUTTON3) ; Entrée seul doit
# choisir « Non » puis « Ignorer » : le programme rend 75 (7 * 10 + 5).
# Avant #2754, le focus allait au premier bouton : 63 (« Oui », « Abandonner »).
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
"$cc" -O1 -o "$work/defaut.exe" "$root/testc/boite/messagebox_defaut.c" -luser32
(cd "$work" && exec "$winlux" defaut.exe) >"$work/out.txt" 2>&1 &
pid=$!
for title in Defaut2 Defaut3; do
    n=0
    until xwininfo -root -tree 2>/dev/null | grep -q "\"$title\""; do
        n=$((n + 1))
        if [ "$n" -gt 150 ] || ! kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            echo "ÉCHEC : boîte « $title » jamais affichée" >&2; cat "$work/out.txt" >&2; exit 1
        fi
        sleep 0.2
    done
    sleep 0.5
    win=$(xdotool search --name "^$title\$" | tail -n 1)
    xdotool windowfocus --sync "$win" key Return 2>/dev/null || xdotool key Return
done
code=0
wait "$pid" || code=$?
if [ "$code" != 75 ]; then
    echo "ÉCHEC : code $code au lieu de 75 (Entrée n'a pas choisi le bouton par défaut)" >&2
    cat "$work/out.txt" >&2; exit 1
fi
echo "MessageBox : Entrée choisit le bouton MB_DEFBUTTON2/3 (#2754)"
