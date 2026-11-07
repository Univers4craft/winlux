#!/usr/bin/env python3
"""Types de fichiers traduits dans les quatre langues (#591) et icônes des .exe (#592).

Construit une base MIME jetable avec freedesktop.org.xml et le paquet
Winlux (integration/x-ms-dos-executable.xml, livré en
/usr/share/mime/packages/winlux.xml), puis vérifie que les types que
Thunar affiche le plus ont un libellé en français, allemand et espagnol,
et que chaque type du paquet Winlux a son libellé français.
"""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = ROOT / "integration/x-ms-dos-executable.xml"
UPSTREAM = Path("/usr/share/mime/packages/freedesktop.org.xml")
NS = "{http://www.freedesktop.org/standards/shared-mime-info}"
LANG = "{http://www.w3.org/XML/1998/namespace}lang"
COMMON = ["inode/directory", "text/plain", "application/x-shellscript", "application/vnd.efi.iso",
          "application/vnd.microsoft.portable-executable", "application/x-ms-dos-executable",
          "application/x-msdownload", "application/zip", "image/png", "application/pdf"]
failures = []


def comments(element):
    return {c.get(LANG): (c.text or "").strip() for c in element.findall(NS + "comment")}


own = ET.parse(PACKAGE).getroot()
for mime in own.findall(NS + "mime-type"):
    if not comments(mime).get("fr"):
        failures.append(f"{mime.get('type')} : pas de libellé français dans le paquet Winlux")

if not UPSTREAM.is_file() or not shutil.which("update-mime-database"):
    print("shared-mime-info absent : seule la vérification du paquet Winlux est faite")
else:
    with tempfile.TemporaryDirectory() as tmp:
        packages = Path(tmp, "packages")
        packages.mkdir()
        shutil.copy(UPSTREAM, packages / UPSTREAM.name)
        shutil.copy(PACKAGE, packages / "winlux.xml")
        subprocess.run(["update-mime-database", tmp], check=True, capture_output=True)
        for kind in COMMON:
            built = Path(tmp, kind + ".xml")
            if not built.is_file():
                failures.append(f"{kind} : absent de la base construite")
                continue
            labels = comments(ET.parse(built).getroot())
            for lang in (None, "fr", "de", "es"):
                if not labels.get(lang):
                    failures.append(f"{kind} : pas de libellé {lang or 'anglais'}")
        folder = comments(ET.parse(Path(tmp, "inode/directory.xml")).getroot())
        if folder.get("fr") != "Dossier":
            failures.append(f"inode/directory en français : {folder.get('fr')!r}")

# Icône du thème Winlux pour chaque type qu'un .exe peut recevoir (#592) :
# sans elle, Thunar et le bureau prennent l'icône générique d'Adwaita.
ICONS = ROOT / "integration/native/icons/Winlux/scalable/mimetypes"
for kind in ("application/x-ms-dos-executable", "application/x-msdownload", "application/x-dosexec",
             "application/vnd.microsoft.portable-executable", "application/x-ms-ne-executable"):
    if not (ICONS / (kind.replace("/", "-") + ".svg")).is_file():
        failures.append(f"{kind} : pas d'icône dans le thème Winlux")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print(f"OK: types de fichiers traduits ({len(own.findall(NS + 'mime-type'))} types du paquet Winlux)")
