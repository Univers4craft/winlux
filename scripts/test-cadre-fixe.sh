#!/bin/sh
# Fenêtre à cadre fixe sous openbox réel (#3011) : une fenêtre de premier
# niveau à barre de titre sans WS_THICKFRAME ni WS_MAXIMIZEBOX (comme une
# boîte de message) n'offre ni « Agrandir » ni « Redimensionner » dans
# _NET_WM_ALLOWED_ACTIONS, que publie openbox ; une fenêtre
# WS_OVERLAPPEDWINDOW lancée par le même programme garde les deux.
# « Réduire » reste : openbox 3.6 n'a aucun moyen de le retirer par
# fenêtre (voir #2856).
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
cc=${WINLUX_MINGW_CC:-x86_64-w64-mingw32-gcc}
for t in "$cc" Xvfb openbox xdotool xprop; do
    command -v "$t" >/dev/null || { echo "$t absent : test sauté"; exit 0; }
done
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
work=$(mktemp -d)
pids=
cleanup() { kill $pids 2>/dev/null || true; rm -rf "$work"; }
trap cleanup EXIT
"$cc" -O1 -o "$work/cadre.exe" "$root/testc/boite/cadre_fixe.c" -luser32
Xvfb -displayfd 3 -screen 0 1024x768x24 -nolisten tcp 3>"$work/display" >/dev/null 2>&1 & pids="$pids $!"
for _ in $(seq 100); do test -s "$work/display" && break; sleep 0.1; done
export DISPLAY=:$(head -1 "$work/display")
for _ in $(seq 50); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done
HOME=$work openbox --sm-disable >/dev/null 2>&1 & pids="$pids $!"
for _ in $(seq 300); do xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q "window id" && break; sleep 0.1; done
(cd "$work" && exec "$winlux" cadre.exe) >"$work/out.txt" 2>&1 & pids="$pids $!"
# Actions publiées par openbox pour la fenêtre gérée qui porte ce titre.
actions() {
    for id in $(xprop -root _NET_CLIENT_LIST 2>/dev/null | grep -o '0x[0-9a-f]*'); do
        if xprop -id "$id" _NET_WM_NAME WM_NAME 2>/dev/null | grep -q "\"$1\""; then
            xprop -id "$id" _NET_WM_ALLOWED_ACTIONS 2>/dev/null
            return 0
        fi
    done
    return 1
}
fixe= normale=
for _ in $(seq 150); do
    fixe=$(actions CadreFixe) || true
    normale=$(actions CadreNormal) || true
    case "$fixe$normale" in *ALLOWED_ACTIONS*ALLOWED_ACTIONS*) break ;; esac
    sleep 0.2
done
echo "CadreFixe   : $fixe"
echo "CadreNormal : $normale"
fail=0
case "$normale" in
    *_NET_WM_ACTION_MAXIMIZE_VERT*) ;;
    *) echo "ÉCHEC : la fenêtre ordinaire n'offre pas Agrandir (openbox absent ?)" >&2; fail=1 ;;
esac
case "$fixe" in
    *ALLOWED_ACTIONS*) ;;
    *) echo "ÉCHEC : fenêtre à cadre fixe jamais gérée par openbox" >&2; fail=1 ;;
esac
case "$fixe" in
    *_NET_WM_ACTION_MAXIMIZE*|*_NET_WM_ACTION_RESIZE*)
        echo "ÉCHEC : la fenêtre à cadre fixe offre encore Agrandir ou Redimensionner" >&2; fail=1 ;;
esac
if [ "$fail" != 0 ]; then cat "$work/out.txt" >&2; exit 1; fi
echo "Cadre fixe : ni Agrandir ni Redimensionner, fenêtre ordinaire intacte (#3011)"
