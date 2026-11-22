#!/bin/sh
# MessageBox.Show de WinForms dans le moteur .NET maison (#2753), sous Xvfb :
# le témoin bcl/temoins/BoiteMessage.exe ouvre de vraies boîtes. Les trois
# premières (bouton par défaut : le premier) doivent apparaître, la vraie
# fenêtre de l'appli « BoiteMessage », et Entrée choisit OK, OK puis Oui : le
# témoin continue alors jusqu'à la quatrième, qui doit apparaître aussi.
# Les valeurs rendues pour toutes les surcharges sont vérifiées sans
# affichage par le témoin lui-même (scripts/test-bcl.sh).
# Prérequis : scripts/build-bcl.sh --temoins et le runtime.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
exe=$root/build/bcl-temoins/BoiteMessage.exe
for t in xdotool xwininfo; do
    command -v "$t" >/dev/null || { echo "$t absent : test sauté"; exit 0; }
done
test -x "$winlux" && test -f "$exe" || { echo "runtime ou témoin absent : test sauté"; exit 0; }
test -n "${DISPLAY:-}" || { echo "lancer sous xvfb-run"; exit 2; }
out=$(mktemp)
trap 'rm -f "$out"' EXIT
(cd "$root/build/bcl-temoins" && WINLUX_BCL_DIR="$root/build/bcl" exec "$winlux" BoiteMessage.exe) >"$out" 2>&1 &
pid=$!
i=0
while [ "$i" -lt 4 ]; do
    n=0
    until xwininfo -root -tree 2>/dev/null | grep -q '("BoiteMessage" "BoiteMessage")'; do
        n=$((n + 1))
        if [ "$n" -gt 150 ] || ! kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            code=0; wait "$pid" || code=$?; echo "ÉCHEC : boîte $((i + 1)) jamais affichée (code $code ; 1 à 3 : mauvaise réponse rendue)" >&2; cat "$out" >&2; exit 1
        fi
        sleep 0.2
    done
    i=$((i + 1))
    # Quatrième boîte affichée : les trois Entrée ont été rendus à l'appli.
    [ "$i" -lt 4 ] || break
    win=$(xdotool search --classname BoiteMessage | tail -n 1)
    sleep 0.5
    xdotool windowfocus --sync "$win" key Return 2>/dev/null || xdotool key Return
    # La boîte se ferme avant que la suivante s'ouvre.
    n=0
    while xdotool search --classname BoiteMessage >/dev/null 2>&1 && [ "$n" -lt 25 ]; do
        n=$((n + 1)); sleep 0.2
    done
done
kill "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true
echo "MessageBox.Show (WinForms) : boîtes affichées, OK et Oui rendus à l'appli (#2753)"
