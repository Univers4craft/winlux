#!/bin/sh
# Retire Winlux et ses associations installés par integration/install.sh.
#
#   sudo integration/uninstall.sh            # même préfixe que l'installation
#   sudo integration/uninstall.sh /usr/local
# Le registre et les données des programmes (~/.local/share/winlux) sont
# conservés; ajoutez --purge-user pour supprimer ceux de l'utilisateur courant.
set -eu

prefix=/usr
purge_user=
for arg in "$@"; do
    case "$arg" in
        --purge-user) purge_user=1 ;;
        -*) echo "option inconnue : $arg" >&2; exit 2 ;;
        *) prefix=$arg ;;
    esac
done
destdir=${DESTDIR:-}
bin="$destdir$prefix/bin"
share="$destdir$prefix/share"
etc="$destdir/etc"
mime_types="application/x-ms-dos-executable application/x-msdownload application/vnd.microsoft.portable-executable application/x-dosexec"
web_types="x-scheme-handler/http x-scheme-handler/https text/html application/xhtml+xml"

# Désenregistre d'abord binfmt pour que le noyau ne pointe plus vers un
# interpréteur supprimé.
if [ -z "$destdir" ] && [ -w /proc/sys/fs/binfmt_misc/winlux ]; then
    echo -1 >/proc/sys/fs/binfmt_misc/winlux || true
fi
rm -f "$etc/binfmt.d/winlux.conf"
rm -f "$bin/winlux" "$bin/winlux-open" "$bin/winlux-update" "$bin/winlux-browser" "$bin/winlux-report"
rm -f "$share/applications/winlux.desktop" "$share/applications/winlux-browser.desktop" "$share/mime/packages/winlux.xml"
rm -f "$share/winlux/version" "$share/winlux/uninstall.sh" "$share/winlux/install-edge.sh"
rmdir "$share/winlux" 2>/dev/null || true

mimeapps="$etc/xdg/mimeapps.list"
if [ -f "$mimeapps" ]; then
    for type in $mime_types; do
        sed -i -e "s|^\($type=.*\)winlux\.desktop;|\1|" -e "\\|^$type=\$|d" "$mimeapps"
    done
    for type in $web_types; do
        sed -i -e "s|^\($type=.*\)winlux-browser\.desktop;|\1|" -e "\\|^$type=\$|d" "$mimeapps"
    done
    # Supprime le fichier s'il ne reste que l'en-tête ajouté par l'installation.
    if ! grep -qv '^\[Default Applications\]$' "$mimeapps"; then
        rm -f "$mimeapps"
    fi
fi

if [ -n "$purge_user" ]; then
    rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/winlux" "${XDG_STATE_HOME:-$HOME/.local/state}/winlux"
fi

if [ -z "$destdir" ]; then
    update-mime-database "$share/mime" 2>/dev/null || true
    update-desktop-database "$share/applications" 2>/dev/null || true
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
fi
echo "Winlux désinstallé de $prefix"
