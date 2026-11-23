#!/bin/sh
# Lance le gestionnaire de fenêtres de la session (#3064) : l'openbox de
# Winlux (boutons Réduire et Agrandir selon le style Win32), ou celui d'Ubuntu
# s'il manque ou ne démarre plus, par exemple si une mise à jour d'Ubuntu
# changeait ses bibliothèques. Une session sans gestionnaire n'est jamais
# possible. Les arguments passent tels quels à openbox.
winlux=${WINLUX_OPENBOX_WINLUX:-/usr/lib/winlux/openbox/openbox}
ubuntu=${WINLUX_OPENBOX_UBUNTU:-/usr/bin/openbox}
if test -x "$winlux" && "$winlux" --version >/dev/null 2>&1; then
    exec "$winlux" "$@"
fi
echo "openbox de Winlux indisponible : openbox d'Ubuntu" >&2
exec "$ubuntu" "$@"
