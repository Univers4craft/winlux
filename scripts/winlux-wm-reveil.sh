#!/bin/sh
# Réveil du gestionnaire de fenêtres pendant son démarrage (#3190).
#
# Openbox 3.6 (celui d'Ubuntu, repli de la session) lit les événements X dans
# sa propre file pendant qu'il démarre, et sa boucle ne regarde ensuite que
# la file de Xlib : une fenêtre affichée entre la prise de l'écran (ce que
# guette « wmctrl -m ») et la fin de son démarrage n'est traitée qu'au
# prochain événement X qu'il reçoit, jamais sur un écran au repos. Ce script,
# lancé avec le gestionnaire, réécrit une propriété de la fenêtre racine
# toutes les 50 ms : chaque écriture est un événement pour lui, et une
# fenêtre ainsi retenue s'affiche en moins de 50 ms. Il s'arrête 3 s après
# que « wmctrl -m » a répondu (30 s au plus), puis retire la propriété.
# Indépendant du gestionnaire : sans effet sur un gestionnaire sans ce défaut.
#
#     winlux-wm-reveil [PID du gestionnaire]
wm_pid=${1:-}
apres=0
i=0
while test "$i" -lt 600 && test "$apres" -lt 60; do
    if test -n "$wm_pid" && ! kill -0 "$wm_pid" 2>/dev/null; then break; fi
    xprop -root -f _WINLUX_REVEIL_WM 32c -set _WINLUX_REVEIL_WM "$i" 2>/dev/null || break
    if test "$apres" -gt 0 || wmctrl -m >/dev/null 2>&1; then apres=$((apres + 1)); fi
    sleep 0.05
    i=$((i + 1))
done
xprop -root -remove _WINLUX_REVEIL_WM 2>/dev/null || true
