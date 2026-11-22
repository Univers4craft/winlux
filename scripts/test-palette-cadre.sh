#!/bin/sh
# #2984, #2983 : une palette flottante (WS_CAPTION + WS_EX_TOOLWINDOW) sous
# un vrai gestionnaire de fenêtres (openbox, Xvfb). Son rectangle Win32
# comprend le cadre du gestionnaire : calée sur le coin bas droit de la zone
# de travail, son cadre réel ne dépasse pas de l'écran, et la replacer au
# même endroit ne la déplace pas. À lancer sous xvfb-run.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_RUNTIME:-$root/target/release/winlux}
exe=$root/target/x86_64-pc-windows-msvc/release/palette_suite.exe
test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
test -f "$exe" || { echo "PE absent : $exe" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'kill "$wm" 2>/dev/null || true; rm -rf "$tmp"' EXIT
openbox >/dev/null 2>&1 &
wm=$!
sleep 1
NO_AT_BRIDGE=1 timeout 60 "$runtime" "$exe" >"$tmp/out" 2>"$tmp/err" &
pe=$!
id=
i=0
while [ -z "$id" ] && [ $i -lt 100 ] && kill -0 "$pe" 2>/dev/null; do
	sleep 0.1
	i=$((i + 1))
	id=$(xwininfo -root -tree | awk '/"Palette"/ {print $1; exit}')
done
[ -n "$id" ] || { kill "$pe" 2>/dev/null; cat "$tmp/out" "$tmp/err" >&2; echo "fenêtre « Palette » introuvable" >&2; exit 1; }
sleep 1
geom=$(xwininfo -id "$id")
ext=$(xprop -id "$id" _NET_FRAME_EXTENTS | sed 's/.*= //; s/,//g')
wait "$pe" && rc=0 || rc=$?
cat "$tmp/out"
x=$(printf '%s\n' "$geom" | awk '/Absolute upper-left X/ {print $4}')
y=$(printf '%s\n' "$geom" | awk '/Absolute upper-left Y/ {print $4}')
w=$(printf '%s\n' "$geom" | awk '/Width:/ {print $2}')
h=$(printf '%s\n' "$geom" | awk '/Height:/ {print $2}')
set -- $ext
right=$((x + w + $2))
bottom=$((y + h + $4))
sw=$(xwininfo -root | awk '/Width:/ {print $2}')
sh=$(xwininfo -root | awk '/Height:/ {print $2}')
echo "cadre réel : $((x - $1)),$((y - $3)) -> $right,$bottom (écran ${sw}x${sh}, cadre $ext)"
test "$rc" -eq 0 || { echo "palette_suite a échoué ($rc)" >&2; exit 1; }
test "$right" -le "$sw" && test "$bottom" -le "$sh" || { echo "le cadre de la palette dépasse de l'écran" >&2; exit 1; }
test $(($1 + $2 + $3 + $4)) -gt 0 || { echo "openbox n'a posé aucun cadre : test sans objet" >&2; exit 1; }
echo "OK : palette dans l'écran, cadre compris (#2984)"
