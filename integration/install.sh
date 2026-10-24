#!/bin/sh
# Installe Winlux et l'associe aux fichiers .exe (double-clic et binfmt).
#
#   sudo integration/install.sh            # installe dans /usr
#   sudo integration/install.sh /usr/local
#   DESTDIR=/tmp/racine integration/install.sh   # pour une image ou un paquet
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
prefix=${1:-/usr}
destdir=${DESTDIR:-}
bin="$destdir$prefix/bin"
share="$destdir$prefix/share"
etc="$destdir/etc"
version=${WINLUX_VERSION:-$(git -C "$root" describe --tags --always 2>/dev/null || printf 'development')}
mime_types="application/x-ms-dos-executable application/x-msdownload application/vnd.microsoft.portable-executable application/x-dosexec"
web_types="x-scheme-handler/http x-scheme-handler/https text/html application/xhtml+xml"

test -x "$root/target/release/winlux" || {
    echo "target/release/winlux absent : lancez d'abord cargo build --release -p winlux" >&2
    exit 1
}

install -Dm755 "$root/target/release/winlux" "$bin/winlux"
install -Dm755 "$root/integration/winlux-open" "$bin/winlux-open"
install -Dm755 "$root/integration/winlux-browser" "$bin/winlux-browser"
install -Dm755 "$root/scripts/install-edge.sh" "$share/winlux/install-edge.sh"
install -Dm755 "$root/scripts/winlux-update.sh" "$bin/winlux-update"
install -Dm755 "$root/integration/uninstall.sh" "$share/winlux/uninstall.sh"
printf '%s\n' "$version" | install -Dm644 /dev/stdin "$share/winlux/version"
sed "s|/usr/bin/winlux-open|$prefix/bin/winlux-open|" "$root/integration/winlux.desktop" \
    | install -Dm644 /dev/stdin "$share/applications/winlux.desktop"
sed "s|/usr/bin/winlux-browser|$prefix/bin/winlux-browser|" "$root/integration/winlux-browser.desktop" \
    | install -Dm644 /dev/stdin "$share/applications/winlux-browser.desktop"
install -Dm644 "$root/integration/x-ms-dos-executable.xml" "$share/mime/packages/winlux.xml"
sed "s|/usr/bin/winlux|$prefix/bin/winlux|" "$root/integration/winlux-binfmt.conf" \
    | install -Dm644 /dev/stdin "$etc/binfmt.d/winlux.conf"

# Application par défaut des .exe pour tous les utilisateurs; uninstall.sh
# retire exactement ces lignes.
mimeapps="$etc/xdg/mimeapps.list"
mkdir -p "$etc/xdg"
touch "$mimeapps"
if ! grep -q '^\[Default Applications\]' "$mimeapps"; then
    printf '[Default Applications]\n' >>"$mimeapps"
fi
# Liens web : Microsoft Edge (via winlux-browser), le navigateur par défaut.
for type in $web_types; do
    if grep -q "^$type=.*winlux-browser\.desktop" "$mimeapps"; then
        continue
    elif grep -q "^$type=" "$mimeapps"; then
        sed -i "s|^$type=|$type=winlux-browser.desktop;|" "$mimeapps"
    else
        sed -i "/^\[Default Applications\]/a $type=winlux-browser.desktop;" "$mimeapps"
    fi
done
for type in $mime_types; do
    if grep -q "^$type=.*winlux\.desktop" "$mimeapps"; then
        continue
    elif grep -q "^$type=" "$mimeapps"; then
        # Une autre application était choisie : Winlux passe en premier.
        sed -i "s|^$type=|$type=winlux.desktop;|" "$mimeapps"
    else
        sed -i "/^\[Default Applications\]/a $type=winlux.desktop;" "$mimeapps"
    fi
done

if [ -z "$destdir" ]; then
    update-mime-database "$share/mime" 2>/dev/null || true
    update-desktop-database "$share/applications" 2>/dev/null || true
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
    if [ -d /proc/sys/fs/binfmt_misc ]; then
        systemctl restart systemd-binfmt.service 2>/dev/null || true
    fi
fi
echo "Winlux $version installé dans $prefix/bin (double-clic : $prefix/bin/winlux-open)"
