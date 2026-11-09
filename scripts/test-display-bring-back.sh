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
n=97
Xvfb :$n -screen 0 2732x768x24 -nolisten tcp >/dev/null 2>&1 & pids="$pids $!"
export DISPLAY=:$n
for _ in $(seq 50); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done
HOME=$work openbox --sm-disable >/dev/null 2>&1 & pids="$pids $!"
sleep 1
xterm -name perdue -geometry 80x20+1804+178 & pids="$pids $!"
xterm -name visible -geometry 40x10+100+100 & pids="$pids $!"
for _ in $(seq 50); do
    lost=$(xdotool search --classname '^perdue$' 2>/dev/null | head -1) || true
    seen=$(xdotool search --classname '^visible$' 2>/dev/null | head -1) || true
    test -n "$lost" && test -n "$seen" && break
    sleep 0.1
done
sleep 0.5
before=$(xdotool getwindowgeometry "$seen" | sed -n 's/.*Position: \([0-9,]*\).*/\1/p')
"$WINLUX" --display bring-back 1366x768+0+0 >/dev/null
sleep 0.5
geo() { xdotool getwindowgeometry "$1" | sed -n 's/.*Position: \([0-9]*\),\([0-9]*\).*/\1 \2/p'; }
size() { xdotool getwindowgeometry "$1" | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\).*/\1 \2/p'; }
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
