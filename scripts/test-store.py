#!/usr/bin/env python3
"""Teste le service du Store (winlux-store) avec apt, Flatpak, AppImageHub et
un catalogue Windows simulés : index fusionné, fiche, installations, mises à
jour et refus des installateurs sans empreinte valide."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import zlib

project = Path(__file__).resolve().parent.parent
store = project / "scripts/winlux-store.py"

def png(path, w=4, h=3):
    raw = b"".join(b"\0" + b"\x30\x60\x90" * w for _ in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

def mock(bindir, name, body):
    p = bindir / name
    p.write_text("#!/bin/sh\n" + body)
    p.chmod(0o755)

def check(cond, message):
    if not cond:
        print(f"ÉCHEC : {message}", file=sys.stderr)
        sys.exit(1)

with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    root, home, bindir = w / "root", w / "home", w / "bin"
    for d in (root, home, bindir):
        d.mkdir()
    log = w / "log"
    env = {**os.environ, "WINLUX_CONTROL_ROOT": str(root), "HOME": str(home),
           "XDG_DATA_HOME": str(home / ".local/share"), "XDG_CACHE_HOME": str(home / ".cache"),
           "XDG_CONFIG_HOME": str(home / ".config"), "WINLUX_DRIVE_C": str(home),
           "WINLUX_STORE_CATALOG": str(project / "integration/store-catalog.txt"),
           "WINLUX_STORE_OFFLINE": "1", "LOG": str(log), "LANG": "fr_FR.UTF-8",
           "WINLUX_APPIMAGE_FEED": (w / "feed.json").as_uri(),
           "PATH": f"{bindir}:/usr/bin:/bin"}

    # --- apt simulé : VLC installé avec mise à jour, GIMP disponible ---
    mock(bindir, "dpkg-query", "printf 'vlc\\tii \\t3.0.20-1\\t12000\\ngit\\tii \\t1:2.43\\t9000\\nlibc6\\tii \\t2.39\\t100\\n'\n")
    mock(bindir, "apt", "printf 'Listing...\\nvlc/noble-updates 3.0.21-1 amd64 [upgradable from: 3.0.20-1]\\nlibc6/noble-updates 2.40 amd64 [upgradable from: 2.39]\\n'\n")
    mock(bindir, "apt-cache", """case "$1" in
policy) shift; for p in "$@"; do echo "$p:"; case "$p" in 0ad) echo '  Candidate: (none)';; *) echo '  Candidate: 1.0-1';; esac; done;;
show) printf 'Package: gimp\\nVersion: 2.10.36-3\\nInstalled-Size: 20480\\nHomepage: https://www.gimp.org\\nDescription: retouche\\n Premier paragraphe.\\n .\\n Second paragraphe.\\n';;
esac
""")
    mock(bindir, "apt-get", """printf 'apt-get %s\\n' "$*" >> "$LOG"
for a in "$@"; do case "$a" in APT::Status-Fd=*) fd=${a#APT::Status-Fd=};; esac; done
eval "echo 'pmstatus:x:60.0:Installation' >&$fd"
""")
    mock(bindir, "id", "echo 0\n")
    # Élévation simulée : la CI a un sudo sans mot de passe dont le
    # secure_path lancerait le vrai apt-get au lieu de la maquette.
    mock(bindir, "sudo", '[ "$1" = -n ] && shift\nexec "$@"\n')
    mock(bindir, "pkexec", 'exec "$@"\n')
    sw = root / "var/lib/swcatalog"
    (sw / "yaml").mkdir(parents=True)
    yaml = """---
File: DEP-11
Origin: ubuntu-noble-universe
MediaBaseUrl: https://media.example/
---
Type: desktop-application
ID: org.gimp.GIMP.desktop
Package: gimp
Name:
  C: GNU Image Manipulation Program
Summary:
  C: Create images and edit photographs
  fr: Créer des images et retoucher des photos
Description:
  C: >-
    <p>GIMP is an image editor.</p>
    <ul><li>Layers</li><li>Filters</li></ul>
Categories:
- Graphics
- 2DGraphics
ProjectLicense: GPL-3.0+
Url:
  homepage: https://www.gimp.org/
Icon:
  cached:
  - name: gimp_gimp.png
    width: 64
    height: 64
Screenshots:
- default: true
  thumbnails:
  - url: g/gimp/752.png
    width: 752
    height: 423
  source-image:
    url: g/gimp/orig.png
Releases:
- version: 2.10.36
  unix-timestamp: 1700000000
  description:
    C: <p>Corrections.</p>
---
Type: desktop-application
ID: org.example.Tool
Package: example-tool
Name:
  C: Example Tool
Summary:
  C: A tool
Categories:
- Development
"""
    (sw / "yaml/ubuntu.yml.gz").write_bytes(gzip.compress(yaml.encode()))
    png(sw / "icons/ubuntu-noble-universe/64x64/gimp_gimp.png")

    # --- Flatpak simulé : GIMP (doublon fusionné) et Krita ---
    mock(bindir, "flatpak", """case "$1" in
remotes) printf 'flathub\\tsystem\\n';;
list) printf 'org.kde.krita\\t5.2.0\\tflathub\\n';;
remote-ls) printf 'org.kde.krita\\t5.2.2\\n';;
*) printf 'flatpak %s\\n' "$*" >> "$LOG"; echo 'Installation 100%';;
esac
""")
    fp = home / ".local/share/flatpak/appstream/flathub/x86_64/active"
    fp.mkdir(parents=True)
    (fp / "appstream.xml").write_text("""<?xml version="1.0"?>
<components>
 <component type="desktop-application"><id>org.gimp.GIMP</id><name>GIMP</name>
  <bundle type="flatpak">app/org.gimp.GIMP/x86_64/stable</bundle><summary>Editor</summary></component>
 <component type="desktop-application"><id>org.kde.krita</id><name>Krita</name>
  <name xml:lang="fr">Krita (fr)</name><summary>Digital painting</summary>
  <description><p>Paint.</p><p xml:lang="fr">Peinture.</p></description>
  <bundle type="flatpak">app/org.kde.krita/x86_64/stable</bundle>
  <categories><category>Graphics</category></categories>
  <icon type="cached" height="128">org.kde.krita.png</icon>
  <project_license>GPL-3.0</project_license><developer_name>KDE</developer_name>
  <releases><release version="5.2.2" timestamp="1710000000"><description><p>Bugfix</p></description></release></releases>
 </component>
</components>
""")
    png(fp / "icons/128x128/org.kde.krita.png")

    # --- AppImageHub simulé (lecture seule hors ligne) ---
    (w / "feed.json").write_text(json.dumps({"items": [
        {"name": "Kdenlive Nightly", "description": "Montage. Vidéo", "categories": ["AudioVideo"],
         "authors": [{"name": "KDE"}], "links": [{"type": "GitHub", "url": "kde/kdenlive"}]},
        {"name": "Sans lien"}]}))

    # --- Catalogue Windows interchangeable (fichier local) ---
    installer = w / "setup.exe"
    installer.write_bytes(b"MZ installateur")
    digest = hashlib.sha256(installer.read_bytes()).hexdigest()
    catalog = w / "windows.json"
    catalog.write_text(json.dumps({"apps": [
        {"id": "notepadpp", "name": "Notepad++", "version": "8.6", "category": "developpement",
         "publisher": "Don Ho", "summary": "Éditeur de code", "featured": True,
         "installer": {"url": installer.as_uri(), "sha256": digest, "args": ["/S"]}},
        {"id": "sansempreinte", "name": "Sans empreinte", "installer": {"url": installer.as_uri()}},
        {"id": "faux", "name": "Mauvaise empreinte", "version": "1",
         "installer": {"url": installer.as_uri(), "sha256": "0" * 64}}]}))
    conf = home / ".config/winlux/store-sources.d"
    conf.mkdir(parents=True)
    (conf / "local.json").write_text(json.dumps({"id": "local", "name": "Catalogue local", "file": str(catalog)}))
    mock(bindir, "winlux", 'printf "winlux %s\\n" "$*" >> "$LOG"\n')

    def st(*args, code=0):
        r = subprocess.run([sys.executable, "-I", "-B", str(store), *args], env=env, capture_output=True, text=True)
        check(r.returncode == code, f"{args} -> {r.returncode} {r.stderr}")
        return r

    run = root / "run/winlux"
    st("refresh")
    index = (run / "store.txt").read_text()
    lines = {l.split("|")[0][4:]: l.split("|") for l in index.splitlines() if l.startswith("app=")}
    check("sources=apt,flatpak,appimage,windows" in index, "sources détectées")
    check("refreshing=0" in index, "rafraîchissement terminé")
    check("system_updates=2" in index, "mises à jour système")
    gimp = lines.get("apt:gimp")
    check(gimp and gimp[1] == "multimedia" and gimp[5] == "a", "GIMP apt disponible")
    check(gimp[9].endswith("gimp_gimp.png") and gimp[10] == "Ubuntu" and gimp[11] == "1", "icône, badge, sélection")
    check("flatpak:org.gimp.GIMP" in gimp[12] and "flatpak:org.gimp.GIMP" not in lines, "doublon fusionné")
    vlc = lines.get("apt:vlc")
    check(vlc and vlc[5] == "u" and vlc[6] == "3.0.20-1" and vlc[7] == "3.0.21-1", "mise à jour VLC")
    check("apt:0ad" not in lines, "paquet absent des dépôts masqué")
    check(lines.get("apt:example-tool", [""] * 2)[1] == "developpement", "catégorie AppStream")
    krita = lines.get("flatpak:org.kde.krita")
    check(krita and krita[2] == "Krita (fr)" and krita[5] == "u" and krita[7] == "5.2.2", "Krita Flatpak")
    check(krita[9].endswith("org.kde.krita.png") and krita[10] == "Flathub", "icône Flatpak")
    check("appimage:Kdenlive_Nightly" in lines and not any("Sans lien" in l for l in index.splitlines()), "AppImage")
    npp = lines.get("windows:local/notepadpp")
    check(npp and npp[10] == "Catalogue local" and npp[11] == "1", "catalogue Windows")
    check(not any("empreinte" in k for k in lines), "fiche sans SHA-256 refusée")
    for c in ["navigateurs", "bureautique", "multimedia", "developpement", "jeux", "utilitaires"]:
        check(any(v[1] == c for v in lines.values()), f"catégorie {c}")

    st("details", "apt:gimp")
    detail = (run / "store-detail.txt").read_text()
    check("license=GPL-3.0+" in detail and "desc=GIMP is an image editor." in detail, "fiche apt")
    check("desc=• Layers" in detail and "release=2.10.36|2023-11-14|Corrections." in detail, "liste et versions")
    check("size_kb=20480" in detail and "alt=flatpak:org.gimp.GIMP|Flathub" in detail, "taille et sources")
    st("details", "flatpak:org.kde.krita")
    detail = (run / "store-detail.txt").read_text()
    check("desc=Peinture." in detail and "publisher=KDE" in detail and "release=5.2.2|2024-03-09|Bugfix" in detail, "fiche Flatpak")
    st("details", "apt:inconnu", code=2)

    st("install", "apt:gimp")
    progress = (run / "store-progress.txt").read_text()
    check("state=done" in progress and "percent=100" in progress, "installation apt terminée")
    check("install --no-install-recommends -- gimp" in log.read_text(), "commande apt")
    st("update", "flatpak:org.kde.krita")
    check("flatpak update org.kde.krita -y --noninteractive" in log.read_text(), "mise à jour Flatpak")
    st("install", "windows:local/notepadpp")
    check(f"winlux {home}/.cache/winlux/store/installers/local_notepadpp.exe /S" in log.read_text(), "installateur Windows")
    state = json.loads((home / ".local/share/winlux/store-state.json").read_text())
    check(state["windows"]["local/notepadpp"]["version"] == "8.6", "version Windows enregistrée")
    check("|i|8.6|8.6|" in (run / "store.txt").read_text(), "index mis à jour après installation")
    st("install", "windows:local/faux", code=5)
    check("SHA-256" in (run / "store-progress.txt").read_text(), "empreinte refusée")
    st("update-all")
    text = log.read_text()
    check("apt-get" in text and " upgrade" in text and "flatpak update -y" in text, "tout mettre à jour")
    (home / "Téléchargements").mkdir()
    (home / "Téléchargements/jeu.exe").write_bytes(b"MZ")
    st("install-file", "C:\\Téléchargements\\jeu.exe")
    check(f"winlux {home}/Téléchargements/jeu.exe" in log.read_text(), "installation d'un .exe local")
    (home / "a.msi").write_bytes(b"x")
    st("install-file", str(home / "a.msi"), code=3)
    check("msiexec" in (run / "store-progress.txt").read_text(), "message msiexec")
    st("install", "apt:inconnu", code=2)
    (run / "store.lock").mkdir()
    st("install", "apt:gimp", code=1)
    (run / "store.lock").rmdir()
    print("Store : OK")
