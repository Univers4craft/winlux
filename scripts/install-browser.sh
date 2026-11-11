#!/bin/sh
# Installe le navigateur choisi par l'utilisateur, téléchargé chez son éditeur.
#
#   sudo install-browser.sh brave|chrome|edge|firefox|opera|operagx|vivaldi
#
# Paquet d'intégration (Winlux installé sur une autre distribution) : même
# règle que l'ISO (docs/BROWSERS.md). Aucun navigateur n'est fourni ni
# favorisé, ils sont proposés dans l'ordre alphabétique, et le paquet
# officiel vient du dépôt de son éditeur, vérifié par la somme SHA-256 de
# l'index de ce dépôt (prepare-browser-packages.py). Le navigateur installé
# devient celui du système (x-www-browser). Tor Browser n'a pas de paquet
# .deb (archive signée) : il s'installe avec winlux-browsers sur Winlux, ou
# avec le torbrowser-launcher de la distribution.
set -eu
here=$(cd "$(dirname -- "$0")" && pwd)
case "${1:-}" in
    brave|chrome|edge|firefox|opera|operagx|vivaldi) browser=$1 ;;
    tor) echo "Tor Browser n'a pas de paquet .deb : installez torbrowser-launcher depuis votre distribution." >&2; exit 2 ;;
    *) echo "usage : sudo $0 brave|chrome|edge|firefox|opera|operagx|vivaldi (Brave, Google Chrome, Microsoft Edge, Mozilla Firefox, Opera, Opera GX, Vivaldi)" >&2; exit 2 ;;
esac
if [ "$(id -u)" != 0 ]; then
    echo "lancez ce script avec sudo" >&2
    exit 1
fi
command -v apt-get >/dev/null 2>&1 || { echo "apt-get est requis (Ubuntu/Debian)" >&2; exit 1; }
fetch=$here/winlux-browser-fetch
test -f "$fetch" || fetch=$here/prepare-browser-packages.py
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
python3 "$fetch" --only "$browser" "$work"
apt-get install -y "$work/$browser.deb"
# Commande posée par le paquet de l'éditeur dans /usr/bin (google-chrome-stable, opera…).
package=$(dpkg-deb -f "$work/$browser.deb" Package)
command=$(dpkg -L "$package" 2>/dev/null | grep '^/usr/bin/' | head -n 1)
command=${command##*/}
test -n "$command" || command=$package
if command -v update-alternatives >/dev/null 2>&1 && path=$(command -v "$command"); then
    update-alternatives --set x-www-browser "$path" 2>/dev/null || true
fi
echo "$command est installé ; les liens des programmes Windows l'ouvrent via winlux-browser."
