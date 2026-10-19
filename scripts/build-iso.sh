#!/bin/sh
set -eu

# Produit une ISO de démonstration sans modifier le système hôte.
# Le répertoire staging doit contenir un système Linux bootable minimal et
# une copie de target/release/winlux sous usr/bin/winlux.
out=${1:-build/winlux-test.iso}
stage=${WINLUX_ISO_STAGE:-build/iso-root}

command -v xorriso >/dev/null 2>&1 || { echo "xorriso est requis" >&2; exit 1; }
test -d "$stage" || { echo "répertoire ISO absent : $stage" >&2; exit 1; }
mkdir -p "$(dirname "$out")"
xorriso -as mkisofs -o "$out" -V WINLUX_TEST "$stage"
echo "$out"
