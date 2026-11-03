#!/bin/sh
# Installe le navigateur choisi par l'utilisateur, téléchargé chez son éditeur.
#
#   sudo install-browser.sh chrome|edge|firefox
#
# Paquet d'intégration (Winlux installé sur une autre distribution) : même
# règle que l'ISO (docs/BROWSERS.md). Aucun navigateur n'est fourni ni
# favorisé, les trois sont proposés dans l'ordre alphabétique, et le paquet
# officiel vient du dépôt de son éditeur, vérifié par la somme SHA-256 de
# l'index de ce dépôt (prepare-browser-packages.py). Le navigateur installé
# devient celui du système (x-www-browser).
set -eu
here=$(cd "$(dirname -- "$0")" && pwd)
case "${1:-}" in
    chrome|edge|firefox) browser=$1 ;;
    *) echo "usage : sudo $0 chrome|edge|firefox (Google Chrome, Microsoft Edge, Mozilla Firefox)" >&2; exit 2 ;;
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
case "$browser" in
    chrome) command=google-chrome-stable ;;
    edge) command=microsoft-edge-stable ;;
    firefox) command=firefox ;;
esac
if command -v update-alternatives >/dev/null 2>&1 && path=$(command -v "$command"); then
    update-alternatives --set x-www-browser "$path" 2>/dev/null || true
fi
echo "$command est installé ; les liens des programmes Windows l'ouvrent via winlux-browser."
