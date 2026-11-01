#!/bin/sh
# Construit le paquet winlux-usb-creator_<version>_all.deb.
# Usage : installation/packaging/build-deb.sh [dossier de sortie]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$(dirname "$here")
root=$(dirname "$src")
out=${1:-"$root/build/installation"}
# WINLUX_USB_VERSION : version imposée (publication pour une étiquette donnée).
version=$(printf '%s' "${WINLUX_USB_VERSION:-$(cat "$root/VERSION")}" | sed 's/^v//')
case "$version" in [0-9]*) ;; *) echo "VERSION invalide : $version" >&2; exit 1 ;; esac
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

lib="$stage/usr/lib/winlux-usb-creator"
mkdir -p "$lib/winlux_usb" "$stage/usr/bin" "$stage/usr/share/applications" \
    "$stage/usr/share/polkit-1/actions" "$stage/usr/share/doc/winlux-usb-creator" "$stage/DEBIAN"
cp "$src"/winlux_usb/*.py "$lib/winlux_usb/"
rm -f "$lib/winlux_usb/disks_windows.py"
printf 'VERSION = "v%s"\n' "$version" >"$lib/winlux_usb/_version.py"
cp "$root/assets/brand/winlux-logo-64.png" "$lib/winlux_usb/logo.png"
cp "$src/winlux-usb-creator.py" "$lib/winlux-usb-creator"
sed -i '1s|.*|#!/usr/bin/python3|' "$lib/winlux-usb-creator"
ln -s ../lib/winlux-usb-creator/winlux-usb-creator "$stage/usr/bin/winlux-usb-creator"
cp "$here/winlux-usb-creator.desktop" "$stage/usr/share/applications/"
cp "$here/org.winlux.usb-creator.policy" "$stage/usr/share/polkit-1/actions/"
for size in 48 64 128 256; do
    mkdir -p "$stage/usr/share/icons/hicolor/${size}x${size}/apps"
    cp "$root/assets/brand/winlux-logo-$size.png" "$stage/usr/share/icons/hicolor/${size}x${size}/apps/winlux-usb-creator.png"
done
cp "$src/README.md" "$stage/usr/share/doc/winlux-usb-creator/README.md"
cat >"$stage/usr/share/doc/winlux-usb-creator/copyright" <<COPY
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: winlux-usb-creator
Source: https://github.com/Univers4craft/winlux

Files: *
License: voir LICENSE dans le dépôt Winlux
COPY
cat >"$stage/DEBIAN/prerm" <<'PRERM'
#!/bin/sh
set -e
rm -rf /usr/lib/winlux-usb-creator/winlux_usb/__pycache__
PRERM
chmod 755 "$stage/DEBIAN/prerm"
find "$stage" -type d -exec chmod 755 {} +
find "$stage/usr" -type f -exec chmod 644 {} +
chmod 755 "$lib/winlux-usb-creator"
size_kb=$(du -sk "$stage/usr" | cut -f1)
cat >"$stage/DEBIAN/control" <<CONTROL
Package: winlux-usb-creator
Version: $version
Section: admin
Priority: optional
Architecture: all
Depends: python3 (>= 3.8), python3-tk
Recommends: pkexec | policykit-1
Installed-Size: $size_kb
Maintainer: Winlux <winlux@users.noreply.github.com>
Homepage: https://github.com/Univers4craft/winlux
Description: Winlux USB stick creator
 Small graphical tool that writes the Winlux installation image to a USB
 stick (UEFI and BIOS bootable). Boot a PC from the stick and choose
 "Installer Winlux sur le disque" to install Winlux permanently.
CONTROL
mkdir -p "$out"
deb="$out/winlux-usb-creator_${version}_all.deb"
dpkg-deb --root-owner-group -Zxz --build "$stage" "$deb" >/dev/null
echo "$deb"
