#!/bin/sh
# Régénère les paquets MSI de test à partir de nos propres sources.
#
# Origine : sample.msi, tables.msi et licence.msi sont produits ici par msitools
# (wixl et msibuild, logiciel libre du projet GNOME, LGPL), à partir des
# descriptions sample.wxs / tables.wxs et des tables tables/*.idt écrites
# par l'équipe Winlux. Les fichiers embarqués sont des textes de test
# créés par ce script (aucun binaire tiers, aucun fichier Microsoft).
#
# Usage : sh runtime/src/msi/testdata/generer.sh   (paquets : wixl msitools)
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Charge utile : la même que celle qu'attendent les tests (msi/tests.rs).
printf 'MZ-winlux-test' > "$work/app.exe"
printf 'Paquet de test de Winlux.\r\n' > "$work/lisezmoi.txt"
i=0
while [ "$i" -lt 2500 ]; do
    printf 'ligne %d : Winlux installe ce paquet\n' $(( i * 7919 % 10007 ))
    i=$(( i + 1 ))
done > "$work/donnees.bin"

cd "$work"
wixl -a x64 -o "$here/sample.msi" "$here/sample.wxs"
wixl -a x64 -o "$here/tables.msi" "$here/tables.wxs"
# wixl n'écrit pas ces tables : on les ajoute au paquet.
for t in "$here"/tables/*.idt; do
    msibuild "$here/tables.msi" -i "$t"
done
# licence.msi (#2123) : sample.msi plus une table Control avec une boîte de
# licence (texte RTF écrit par l'équipe Winlux) et un lisez-moi.
cp "$here/sample.msi" "$here/licence.msi"
msibuild "$here/licence.msi" -i "$here/licence/Control.idt"
echo "Paquets régénérés dans $here"
