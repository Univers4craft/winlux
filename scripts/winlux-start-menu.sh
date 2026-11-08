#!/bin/sh
# Touche Windows, bouton Démarrer, Rechercher : /usr/bin/winlux-start-menu.
#
# Le menu (/usr/lib/winlux/winlux-start-menu, en Python) reste résident une
# fois lancé, et la session le précharge caché (--preload). Chaque appui
# démarrait pourtant un Python et GTK complets, seulement pour envoyer
# « ouvre-toi » au menu déjà lancé. Ici, gdbus envoie directement cette
# demande au menu ; s'il ne tourne pas encore, le menu est lancé comme avant.
menu=${WINLUX_START_MENU_PROGRAM:-/usr/lib/winlux/winlux-start-menu}
if test $# -eq 0 && command -v gdbus >/dev/null 2>&1 &&
    gdbus call --session --dest org.winlux.Start --object-path /org/winlux/Start \
        --method org.freedesktop.Application.Activate '{}' --timeout 5 >/dev/null 2>&1; then
    exit 0
fi
exec "$menu" "$@"
