#!/bin/sh
# #3170, sous Xvfb : MsgWaitForMultipleObjects dort jusqu'au signal.
# Pendant 3 s d'attente sans rien, le fil ne doit presque pas se réveiller
# (avant #3170 : ~98 réveils/s, des tranches de 10 ms). Puis un événement et
# un message postés depuis un autre fil réveillent l'attente à temps.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
cc=${WINLUX_MINGW_CC:-x86_64-w64-mingw32-gcc}
command -v "$cc" >/dev/null || { echo "$cc absent : test sauté"; exit 0; }
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
test -n "${DISPLAY:-}" || { echo "lancer sous xvfb-run"; exit 2; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$cc" -O1 -mwindows -o "$work/attente.exe" "$root/testc/attente/msgwait_repos.c" -luser32
(cd "$work" && exec "$winlux" attente.exe) >"$work/out.txt" 2>&1 &
pid=$!
reveils() { cat /proc/$pid/task/*/status 2>/dev/null | awk '/^voluntary_ctxt_switches/{s+=$2} END{print s+0}'; }
sleep 1
a=$(reveils)
sleep 1.5
b=$(reveils)
par_s=$(( (b - a) * 2 / 3 ))
code=0
wait "$pid" || code=$?
echo "attente sans rien : $par_s réveils/s ; code de sortie $code"
if [ "$par_s" -gt 20 ]; then
    echo "ÉCHEC : $par_s réveils par seconde pendant une attente sans rien (attendu au plus 20)" >&2
    cat "$work/out.txt" >&2; exit 1
fi
if [ "$code" != 0 ]; then
    echo "ÉCHEC : étape $code (1 délai, 2 événement, 3 message ; 1x = réveil en retard)" >&2
    cat "$work/out.txt" >&2; exit 1
fi
echo "MsgWaitForMultipleObjects dort jusqu'au signal, événement et message réveillent à temps (#3170)"
