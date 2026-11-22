#!/bin/bash
# Lance une suite de tests avec une borne de durée (#2839, voir #2225).
#
# Un gel rare de la suite unitaire a bloqué la CI 47 minutes, puis le run a
# été annulé à la main sans laisser de pile : la cause n'a jamais pu être
# lue. Ce lanceur exécute la commande donnée (par défaut `cargo test -p
# winlux`) dans son propre groupe de processus. Si elle dépasse la borne, il
# relève les piles de tous les fils de chaque processus du groupe avec gdb,
# arrête le groupe et sort avec le code 124 (celui de timeout(1)).
#
#   BORNE_TESTS  borne en secondes (600 par défaut)
#   usage        scripts/tests-bornes.sh [commande arguments…]
#
# gdb s'attache par sudo quand on n'est pas root : Ubuntu limite ptrace aux
# descendants directs (yama ptrace_scope = 1). Sans gdb ni sudo, les piles
# manquent mais la borne tient.
set -u
borne=${BORNE_TESTS:-600}
[ $# -gt 0 ] || set -- cargo test -p winlux

setsid "$@" &
chef=$!
debut=$(date +%s)
while kill -0 "$chef" 2>/dev/null; do
  if [ $(( $(date +%s) - debut )) -ge "$borne" ]; then
    echo "::error::Suite figée : plus de $borne s. Piles de chaque processus ci-dessous." >&2
    if [ "$(id -u)" = 0 ]; then sudo=""; else sudo="sudo -n"; fi
    for pid in $(pgrep -g "$chef"); do
      [ "$pid" = "$chef" ] && continue
      echo "===== $pid : $(tr '\0' ' ' < /proc/$pid/cmdline 2>/dev/null)" >&2
      if command -v gdb >/dev/null; then
        $sudo gdb -p "$pid" -batch -nx -ex "set pagination off" \
          -ex "thread apply all bt" >&2 2>&1 </dev/null
      else
        for t in /proc/$pid/task/*; do
          echo "fil ${t##*/} : $(cat "$t/comm" 2>/dev/null) attend dans $(cat "$t/wchan" 2>/dev/null)" >&2
        done
      fi
    done
    kill -KILL -- "-$chef" 2>/dev/null
    wait "$chef" 2>/dev/null
    exit 124
  fi
  sleep 2
done
wait "$chef"
