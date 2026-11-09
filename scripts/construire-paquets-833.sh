#!/bin/sh
# Construit depuis le dépôt les paquets MSI de la fiche de banc 833
# (actions personnalisées EXE, DLL et script, #833 et #973) :
#   actions-dll-exe.msi  (scripts/test-msi-actions.sh)
#   actions-script.msi   (scripts/test-msi-scripts.sh)
# Outils libres seulement : wixl et msitools (msibuild, msiinfo), nsis,
# gcc-mingw-w64. Aucun binaire préfabriqué. La CI publie le résultat dans
# l'artefact « paquets-833 ».
# Usage : sh scripts/construire-paquets-833.sh <dossier>
set -eu
[ $# -eq 1 ] || { echo "usage : $0 <dossier>" >&2; exit 2; }
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WINLUX_MSI_OUT=$1 sh "$here/test-msi-actions.sh" >/dev/null
WINLUX_MSI_OUT=$1 sh "$here/test-msi-scripts.sh" >/dev/null
for f in actions-dll-exe.msi actions-script.msi; do
    test -s "$1/$f" || { echo "$f non construit" >&2; exit 1; }
done
(cd "$1" && sha256sum actions-dll-exe.msi actions-script.msi | tee SHA256SUMS)
