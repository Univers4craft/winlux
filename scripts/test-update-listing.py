#!/usr/bin/python3
"""Paramètres > Mises à jour : la taille ne s'affiche que si une mise à jour existe.

Défaut v0.2.14 : déjà à jour, la page affichait « Taille à télécharger :
463 Mo », comme si une mise à jour attendait. On vérifie les deux moteurs
(APT et historique) sur un système isolé, sans root.
"""
import contextlib
import io
from pathlib import Path
import runpy
import tempfile

project = Path(__file__).resolve().parent.parent
SHA = "a" * 64


def apt_listing(installed, available):
    module = runpy.run_path(str(project / "scripts/winlux-update-apt.py"), run_name="listing_test")
    namespace = module["listing"].__globals__
    files = {
        "/usr/share/winlux/version": installed,
        str(namespace["STATE"] / "available-manifest"):
            f"format=3\nbackend=apt\nbase=ubuntu-resolute\nversion={available}\nscope=system\naction=reboot\n"
            f"asset=winlux-desktop-{available}.deb\nsha256={SHA}\ndownload_bytes=485490688\nunpacked_kib=1422336\n",
    }
    namespace["read"] = lambda path: files.get(str(path), "")
    namespace["package_version"] = lambda _path: ""
    output = io.StringIO()
    with contextlib.redirect_stdout(output):
        module["listing"]()
    return output.getvalue()


text = apt_listing("v0.2.14", "v0.2.14")
assert "Winlux est à jour." in text, text
for hidden in ("Taille à télécharger", "Taille décompressée", "Composants", "Version disponible"):
    assert hidden not in text, (hidden, text)
text = apt_listing("v0.2.14", "v0.2.15")
assert "Version disponible : v0.2.15" in text and "Taille à télécharger : 485490688 octets" in text, text
assert "Winlux est à jour." not in text, text

# Moteur historique (winlux-update.sh --list) : même règle.
with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    (root / "usr/share/winlux").mkdir(parents=True)
    (root / "var/lib/winlux").mkdir(parents=True)
    (root / "usr/share/winlux/version").write_text("v0.2.14\n")
    script = (project / "scripts/winlux-update.sh").read_text()
    assert 'if up_to_date; then\n        # À jour : ni taille ni composants' in script
    listing = script[script.index("list_updates() {"):]
    listing = listing[:listing.index("\n}\n")]
    assert listing.index("up_to_date") < listing.index("Taille à télécharger"), "taille affichée avant le test « à jour »"

# Les textes restent traduits dans les quatre langues de Paramètres.
settings = (project / "scripts/winlux-settings.py").read_text()
assert "'Winlux est à jour.': ('Winlux is up to date.'" in settings
assert "'Taille à télécharger : ': ('Download size: '" in settings
print("test-update-listing : OK (taille seulement si une mise à jour est disponible)")
