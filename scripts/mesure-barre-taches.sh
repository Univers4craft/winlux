#!/bin/sh
# Mesure, dans une vraie session Winlux, ce que coûte la barre des tâches au repos.
# À lancer par le testeur, dans un terminal de la session, sans argument :
#   sh /usr/lib/winlux/mesure-barre-taches.sh   (ou scripts/mesure-barre-taches.sh)
# Affiche : le service résident vivant ou non, le temps moyen d'une lecture par
# le lecteur (/usr/bin/winlux-tray), celui d'un calcul direct en Python, et le
# temps processeur consommé par le service sur 60 secondes.
set -u
lecteur=${WINLUX_TRAY_READER:-/usr/bin/winlux-tray}
calcul=${WINLUX_TRAY_PROGRAM:-/usr/lib/winlux/winlux-tray}
sortie=${XDG_CACHE_HOME:-$HOME/.cache}/winlux/tray-out
n=${MESURE_TOURS:-50}

ms() { date +%s%N | cut -c1-13; }

pid=$(cat "$sortie/serve.pid" 2>/dev/null || true)
if [ -n "$pid" ] && [ "$(cat /proc/"$pid"/comm 2>/dev/null)" = winlux-tray ]; then
    echo "Service résident : vivant (pid $pid)"
else
    echo "Service résident : absent, les lectures retombent sur le calcul direct"
    pid=
fi

debut=$(ms); i=0
while [ "$i" -lt "$n" ]; do "$lecteur" status >/dev/null 2>&1; i=$((i + 1)); done
lecture=$(( ($(ms) - debut) * 1000 / n ))
debut=$(ms); i=0
while [ "$i" -lt 10 ]; do WINLUX_TRAY_NO_SERVE=1 "$calcul" status >/dev/null 2>&1; i=$((i + 1)); done
direct=$(( ($(ms) - debut) * 1000 / 10 ))
echo "Lecture par le lecteur : $((lecture / 1000)),$(printf %03d $((lecture % 1000))) ms ($n tours)"
echo "Calcul direct en Python : $((direct / 1000)),$(printf %03d $((direct % 1000))) ms (10 tours)"

if [ -n "$pid" ]; then
    hz=$(getconf CLK_TCK)
    avant=$(awk '{print $14 + $15}' /proc/"$pid"/stat)
    sleep 60
    apres=$(awk '{print $14 + $15}' /proc/"$pid"/stat 2>/dev/null || echo "$avant")
    echo "Service : $(( (apres - avant) * 1000 / hz )) ms de processeur en 60 s"
fi
