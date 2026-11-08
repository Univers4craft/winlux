#!/bin/sh
# Greffons genmon de la barre des tâches (/usr/bin/winlux-tray KIND).
#
# Chaque greffon relançait un Python toutes les 2 à 5 s (environ 80 par
# minute). Le service résident `winlux-tray serve` (/usr/lib/winlux/winlux-tray)
# calcule ces lignes dans un seul processus et les dépose dans
# ~/.cache/winlux/tray-out ; ce lecteur les recopie avec les seules commandes
# internes du shell. Service absent ou sortie pas encore calculée : il le
# lance et répond par un calcul direct, comme avant.
tray=${WINLUX_TRAY_PROGRAM:-/usr/lib/winlux/winlux-tray}
out=${XDG_CACHE_HOME:-$HOME/.cache}/winlux/tray-out
case "${1:-}" in
    balance|status|search|notifications)
        pid= comm=
        { read -r pid <"$out/serve.pid"; } 2>/dev/null
        if test -n "$pid" && { read -r comm <"/proc/$pid/comm"; } 2>/dev/null && test "$comm" = winlux-tray &&
            test -s "$out/out-$1"; then
            # Plusieurs lignes possibles (infobulle réseau + son + batterie).
            { while IFS= read -r line; do printf '%s\n' "$line"; done <"$out/out-$1"; } 2>/dev/null && exit 0
        fi
        if test "${WINLUX_TRAY_NO_SERVE:-0}" != 1; then
            setsid -f "$tray" serve </dev/null >/dev/null 2>&1
        fi
        ;;
esac
exec "$tray" "$@"
