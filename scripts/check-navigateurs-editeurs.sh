#!/bin/sh
# Relevé réel chez les éditeurs (réseau des runners de la CI, #1716) : pour
# chaque navigateur proposé, l'index officiel est lu et le paquet retenu est
# affiché (version, adresse, SHA-256). Tor Browser : liste SHA-256 signée et
# clé épinglée vérifiées. Rien n'est téléchargé d'autre que les index.
set -u
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
status=0
for id in brave chrome edge firefox opera operagx tor vivaldi; do
    echo "== $id"
    if ! python3 -I -B "$project/scripts/prepare-browser-packages.py" --info "$id"; then
        echo "ÉCHEC : $id introuvable chez son éditeur" >&2
        status=1
    fi
done
exit "$status"
