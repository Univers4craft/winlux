#!/bin/sh
# System.IO.Compression (#2819) : le témoin bcl/temoins/Compression.cs contre
# une grosse archive écrite par le module zipfile de Python. Elle contient du
# texte de plusieurs centaines de Kio (codes dynamiques, distances jusqu'à
# 32 Kio, fenêtre qui tourne), des octets aléatoires (blocs stockés dans
# Deflate) et une entrée vide ; chaque entrée est comparée octet pour octet.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
temoins=$root/build/bcl-temoins
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
test -f "$temoins/Compression.exe" || { echo "lancer scripts/build-bcl.sh --temoins d'abord : test sauté"; exit 0; }
command -v python3 >/dev/null || { echo "python3 absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat >"$work/ecrire.py" <<'PY'
import os, random, sys, zipfile

archive = sys.argv[1]
dossier = archive + ".d"
os.makedirs(dossier)
r = random.Random(2819)
mots = ["winlux", "paint", "net", "fenêtre", "pinceau", "calque", "couleur", "image", "zip", "deflate"]
entrees = {
    "texte.txt": " ".join(r.choice(mots) for _ in range(60000)).encode("utf-8"),
    "aleatoire.bin": bytes(r.getrandbits(8) for _ in range(100000)),
    "vide.txt": b"",
}
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
    for nom, donnees in entrees.items():
        z.writestr(nom, donnees)
        with open(os.path.join(dossier, nom), "wb") as f:
            f.write(donnees)
PY
python3 -I "$work/ecrire.py" "$work/paquet.zip"

set +e
(cd "$temoins" && WINLUX_BCL_DIR="$root/build/bcl" WINLUX_TEMOIN_ZIP="$work/paquet.zip" \
    timeout 300 "$winlux" Compression.exe)
code=$?
set -e
if [ "$code" != 200 ]; then
    echo "ÉCHEC : témoin Compression, code $code (200 attendu ; 9 = grosse archive)"
    exit 1
fi
echo "ok : ZipArchive et DeflateStream sur une archive de $(wc -c <"$work/paquet.zip") octets (#2819)"
