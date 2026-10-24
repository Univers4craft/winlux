#!/bin/sh
# Installe Microsoft Edge (canal stable) depuis le dépôt officiel de
# Microsoft et en fait le navigateur par défaut de Winlux.
#
#   sudo scripts/install-edge.sh
#
# Edge est un navigateur Linux natif publié par Microsoft : il ne passe pas
# par la couche de compatibilité .exe de Winlux.
set -eu

if [ "$(id -u)" != 0 ]; then
    echo "lancez ce script avec sudo" >&2
    exit 1
fi
command -v apt-get >/dev/null 2>&1 || { echo "apt-get est requis (Ubuntu/Debian)" >&2; exit 1; }
for tool in curl gpg; do
    command -v "$tool" >/dev/null 2>&1 || apt-get install -y "$tool"
done

keyring=/usr/share/keyrings/microsoft-edge.gpg
curl -fsSL https://packages.microsoft.com/keys/microsoft.asc | gpg --dearmor --yes -o "$keyring"
chmod 644 "$keyring"
echo "deb [arch=amd64 signed-by=$keyring] https://packages.microsoft.com/repos/edge stable main" \
    >/etc/apt/sources.list.d/microsoft-edge.list
apt-get update
apt-get install -y microsoft-edge-stable

# Navigateur par défaut du système (x-www-browser) et des sessions XDG.
if command -v update-alternatives >/dev/null 2>&1; then
    update-alternatives --set x-www-browser /usr/bin/microsoft-edge-stable 2>/dev/null || true
    update-alternatives --set gnome-www-browser /usr/bin/microsoft-edge-stable 2>/dev/null || true
fi
mimeapps=/etc/xdg/mimeapps.list
touch "$mimeapps"
grep -q '^\[Default Applications\]' "$mimeapps" || printf '[Default Applications]\n' >>"$mimeapps"
for type in x-scheme-handler/http x-scheme-handler/https text/html application/xhtml+xml; do
    sed -i "\\|^$type=|d" "$mimeapps"
    sed -i "/^\[Default Applications\]/a $type=microsoft-edge.desktop;winlux-browser.desktop;" "$mimeapps"
done
echo "Microsoft Edge est installé et défini comme navigateur par défaut."
