#!/bin/sh
# Openbox de Winlux (#3064) : la source d'Ubuntu 26.04, vérifiée par son
# SHA-256, ses correctifs Debian, puis les nôtres
# (integration/native/openbox/*.patch). Seul le programme openbox change :
# il se lie aux bibliothèques libobrender32 et libobt2 du même paquet source
# Ubuntu. Le binaire va dans /usr/lib/winlux/openbox/openbox, jamais dans
# /usr/bin : /usr/bin/openbox reste celui d'Ubuntu, à qui dpkg et apt
# l'attribuent, et la session revient à lui si le nôtre ne démarre pas.
#
#     sh scripts/build-openbox.sh DOSSIER    → DOSSIER/openbox
#
# Dépendances de construction (paquets Ubuntu) : autoconf automake libtool
# gettext autopoint pkg-config libglib2.0-dev libxml2-dev libpango1.0-dev
# libx11-dev libxft-dev libxrandr-dev libxinerama-dev libxcursor-dev
# libstartup-notification0-dev libsm-dev libxext-dev libimlib2-dev
# librsvg2-dev libxi-dev.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out=${1:?usage : build-openbox.sh DOSSIER}
version=3.6.1-12ubuntu3
pool=https://archive.ubuntu.com/ubuntu/pool/universe/o/openbox
# Sommes de l'index Sources de resolute/universe (Checksums-Sha256).
sums="8b4ac0760018c77c0044fab06a4f0c510ba87eae934d9983b10878483bde7ef7  openbox_3.6.1.orig.tar.gz
1e20d9a4fd8fd1256f62e0836e1bff9fbcb2781e346a093d96c93e55b0c28471  openbox_${version}.debian.tar.xz"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/dl" "$work/src" "$work/dest"
# Fichiers téléchargés : dossier vide à eux, vérifiés avant toute lecture.
printf '%s\n' "$sums" | while read -r _ name; do
    curl -fsSL --retry 3 -o "$work/dl/$name" "$pool/$name"
done
(cd "$work/dl" && printf '%s\n' "$sums" | sha256sum -c --quiet -) || {
    echo "ÉCHEC : source openbox $version différente de celle attendue" >&2; exit 1; }
tar -xzf "$work/dl/openbox_3.6.1.orig.tar.gz" -C "$work/src"
src="$work/src/openbox-3.6.1"
tar -xJf "$work/dl/openbox_${version}.debian.tar.xz" -C "$src"
# Correctifs Debian dans l'ordre de leur série (lignes commentées sautées).
grep -v '^[[:space:]]*#' "$src/debian/patches/series" | while read -r p _; do
    test -n "$p" || continue
    patch -s -p1 -d "$src" <"$src/debian/patches/$p"
done
for p in "$root"/integration/native/openbox/*.patch; do
    patch -s -p1 -d "$src" <"$p"
done
(
    cd "$src"
    autoreconf -fi >/dev/null 2>&1
    ./configure --prefix=/usr --sysconfdir=/etc --disable-static >"$work/configure.log" 2>&1 || {
        tail -30 "$work/configure.log" >&2; exit 1; }
    make -j"${WINLUX_OPENBOX_JOBS:-4}" openbox/openbox >"$work/make.log" 2>&1 || {
        tail -30 "$work/make.log" >&2; exit 1; }
    # Installé sans le chemin de construction (libtool relie le programme).
    make DESTDIR="$work/dest" install-binPROGRAMS >"$work/install.log" 2>&1 || {
        tail -30 "$work/install.log" >&2; exit 1; }
)
mkdir -p "$out"
install -m 755 "$work/dest/usr/bin/openbox" "$out/openbox"
strip "$out/openbox"
if readelf -d "$out/openbox" | grep -Eq 'RPATH|RUNPATH'; then
    echo "ÉCHEC : openbox garde un chemin de bibliothèques de construction" >&2; exit 1
fi
"$out/openbox" --version | head -1
sha256sum "$out/openbox"
