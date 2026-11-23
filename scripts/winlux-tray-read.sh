#!/bin/sh
# Greffons genmon de la barre des tâches (/usr/bin/winlux-tray KIND).
#
# Chaque greffon relançait un Python toutes les 2 à 5 s (environ 80 par
# minute ; le clavier tactile, osk, jusqu'au #3419). Le service résident `winlux-tray serve` (/usr/lib/winlux/winlux-tray)
# calcule ces lignes dans un seul processus et les dépose dans
# ~/.cache/winlux/tray-out ; ce lecteur les recopie avec les seules commandes
# internes du shell. Service absent ou sortie pas encore calculée : il le
# lance et répond par un calcul direct, comme avant.
tray=${WINLUX_TRAY_PROGRAM:-/usr/lib/winlux/winlux-tray}
out=${XDG_CACHE_HOME:-$HOME/.cache}/winlux/tray-out
case "${1:-}" in
    balance|status|search|notifications|osk)
        pid= comm= panel= stat= caller= caller_comm=
        { read -r pid <"$out/serve.pid"; } 2>/dev/null
        # Une barre ne reçoit que les sorties calculées pour elle (#2467) :
        # juste après une relance de la barre (autre ordre de greffons, bouton
        # de l'assistant ajouté), l'ancienne vit encore un instant et son
        # contrepoids, faux pour la nouvelle, était servi. La barre qui
        # demande est le parent du wrapper du greffon ; un appel hors de la
        # barre (terminal, mesure-barre-taches.sh) est servi comme avant.
        { read -r panel <"$out/serve.panel"; } 2>/dev/null
        if { read -r stat <"/proc/$PPID/stat"; } 2>/dev/null; then
            stat=${stat##*) }; stat=${stat#* }; caller=${stat%% *}
        fi
        { read -r caller_comm <"/proc/$caller/comm"; } 2>/dev/null
        if test -n "$pid" && { read -r comm <"/proc/$pid/comm"; } 2>/dev/null && test "$comm" = winlux-tray &&
            { test "$caller_comm" != xfce4-panel || test "$panel" = "$caller"; } && test -s "$out/out-$1"; then
            # Plusieurs lignes possibles (infobulle réseau + son + batterie).
            { while IFS= read -r line; do printf '%s\n' "$line"; done <"$out/out-$1"; } 2>/dev/null && exit 0
        fi
        if test "${WINLUX_TRAY_NO_SERVE:-0}" != 1; then
            setsid -f "$tray" serve </dev/null >/dev/null 2>&1
        fi
        ;;
esac
exec "$tray" "$@"
