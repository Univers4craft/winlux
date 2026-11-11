#!/bin/sh
# Fenêtre restée sur un écran débranché (#1412) : avec openbox sur un écran
# X de 2732x768, une fenêtre placée sur la moitié droite (le « second
# écran ») est ramenée sur l'écran restant 1366x768+0+0 par
# `winlux --display bring-back`, une fenêtre déjà visible ne bouge pas.
set -eu
WINLUX=${WINLUX:-target/debug/winlux}
for tool in Xvfb openbox xterm xdotool; do
    command -v "$tool" >/dev/null || { echo "SKIP : $tool absent"; exit 0; }
done
test -x "$WINLUX" || { echo "ÉCHEC : $WINLUX absent"; exit 1; }
work=$(mktemp -d)
cleanup() { kill $pids 2>/dev/null || true; rm -rf "$work"; }
pids=
trap cleanup EXIT
# Numéro d'affichage libre choisi par Xvfb : jamais un serveur d'un passage
# précédent encore en train de s'arrêter.
Xvfb -displayfd 3 -screen 0 2732x768x24 -nolisten tcp 3>"$work/display" >/dev/null 2>&1 & pids="$pids $!"
for _ in $(seq 100); do test -s "$work/display" && break; sleep 0.1; done
export DISPLAY=:$(head -1 "$work/display")
for _ in $(seq 50); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done
HOME=$work openbox --sm-disable >/dev/null 2>&1 & pids="$pids $!"
# Attendre qu'openbox gère l'écran avant d'ouvrir les fenêtres.
for _ in $(seq 300); do xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q "window id" && break; sleep 0.1; done
xterm -name perdue -geometry 80x20+1804+178 & pids="$pids $!"
xterm -name visible -geometry 40x10+100+100 & pids="$pids $!"
for _ in $(seq 300); do
    lost=$(xdotool search --classname '^perdue$' 2>/dev/null | head -1) || true
    seen=$(xdotool search --classname '^visible$' 2>/dev/null | head -1) || true
    test -n "$lost" && test -n "$seen" && break
    sleep 0.1
done
# Attendre qu'openbox ait pris les deux fenêtres en charge (liste des
# clients et bordures publiées).
managed() {
    list=$(xprop -root _NET_CLIENT_LIST 2>/dev/null)
    for w in "$lost" "$seen"; do
        echo "$list" | grep -qi "$(printf '0x%x' "$w")" || return 1
        xprop -id "$w" _NET_FRAME_EXTENTS 2>/dev/null | grep -q "=" || return 1
    done
}
for _ in $(seq 300); do managed && break; sleep 0.1; done
managed || { echo "ÉCHEC : openbox n'a pas pris les fenêtres en charge en 30 s"; exit 1; }
geo() { xdotool getwindowgeometry "$1" | sed -n 's/.*Position: \([0-9]*\),\([0-9]*\).*/\1 \2/p'; }
size() { xdotool getwindowgeometry "$1" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\).*/\1 \2/p'; }
# xterm peut encore redemander sa taille et sa position (police de repli) :
# attendre une géométrie stable avant de « débrancher » l'écran (#1429).
prev=
for _ in $(seq 30); do
    now="$(geo "$lost") $(size "$lost")"
    [ "$now" = "$prev" ] && break
    prev=$now
    sleep 0.3
done
before=$(xdotool getwindowgeometry "$seen" | sed -n 's/.*Position: \([0-9,]*\).*/\1/p')
moved=$("$WINLUX" --display bring-back 1366x768+0+0)
echo "fenêtres déplacées : $moved"
# Le gestionnaire de fenêtres applique la demande de façon asynchrone.
for _ in $(seq 50); do
    set -- $(geo "$lost") $(size "$lost")
    [ "$1" -ge 0 ] && [ $(($1 + $3)) -le 1366 ] && break
    sleep 0.1
done
set -- $(geo "$lost") $(size "$lost")
x=$1 y=$2 w=$3 h=$4
echo "fenêtre perdue ramenée en $x,$y ($w x $h)"
if [ "$x" -lt 0 ] || [ $((x + w)) -gt 1366 ] || [ "$y" -lt 0 ] || [ $((y + h)) -gt 768 ]; then
    echo "ÉCHEC : la fenêtre dépasse encore de l'écran restant"; exit 1
fi
after=$(xdotool getwindowgeometry "$seen" | sed -n 's/.*Position: \([0-9,]*\).*/\1/p')
if [ "$before" != "$after" ]; then
    echo "ÉCHEC : la fenêtre visible a bougé ($before → $after)"; exit 1
fi
echo "OK : fenêtre ramenée sur l'écran restant, fenêtre visible laissée en place"
