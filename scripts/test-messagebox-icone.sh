#!/bin/sh
# MessageBox, icône du type MB_ICON* (#2854), sous Xvfb : une vraie boîte
# MessageBoxW(MB_OK | MB_ICONERROR). Sur une capture de la boîte, le disque
# rouge de l'icône d'erreur (#C42B1C, #2855) doit couvrir au moins 200
# pixels ; Entrée ferme ensuite la boîte (code 1, IDOK).
# Avant #2854, l'espace de l'icône restait vide : aucun pixel rouge.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
cc=${WINLUX_MINGW_CC:-x86_64-w64-mingw32-gcc}
for t in "$cc" xdotool xwininfo xwd convert; do
    command -v "$t" >/dev/null || { echo "$t absent : test sauté"; exit 0; }
done
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
test -n "${DISPLAY:-}" || { echo "lancer sous xvfb-run"; exit 2; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$cc" -O1 -o "$work/icone.exe" "$root/testc/boite/messagebox_icone.c" -luser32
(cd "$work" && exec "$winlux" icone.exe) >"$work/out.txt" 2>&1 &
pid=$!
n=0
until xwininfo -root -tree 2>/dev/null | grep -q '"IconeErreur"'; do
    n=$((n + 1))
    if [ "$n" -gt 150 ] || ! kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        echo "ÉCHEC : boîte jamais affichée" >&2; cat "$work/out.txt" >&2; exit 1
    fi
    sleep 0.2
done
sleep 1
win=$(xdotool search --name '^IconeErreur$' | tail -n 1)
xwd -id "$win" -silent >"$work/boite.xwd"
rouge=$(convert "$work/boite.xwd" txt:- 2>/dev/null | awk 'NR > 1 && $3 == "#C42B1C" { n++ } END { print n + 0 }')
xdotool windowfocus --sync "$win" key Return 2>/dev/null || xdotool key Return
code=0
wait "$pid" || code=$?
if [ "$rouge" -lt 200 ]; then
    echo "ÉCHEC : $rouge pixels rouges, l'icône d'erreur n'est pas peinte" >&2
    cat "$work/out.txt" >&2; exit 1
fi
if [ "$code" != 1 ]; then
    echo "ÉCHEC : code $code au lieu de 1 (IDOK)" >&2; cat "$work/out.txt" >&2; exit 1
fi
echo "MessageBox : icône MB_ICONERROR peinte ($rouge pixels) (#2854)"
