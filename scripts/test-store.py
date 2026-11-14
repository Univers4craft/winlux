#!/usr/bin/env python3
"""Teste le service du Store (winlux-store) avec apt, Flatpak, AppImageHub et
un catalogue Windows simulés : index fusionné, fiche, installations, mises à
jour et refus des installateurs sans empreinte valide."""
import gzip
import hashlib
import io
import json
import tarfile
import zipfile
import os
from pathlib import Path
import struct
import runpy
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
           "WINLUX_APPIMAGE_FEED": (w / "feed.json").as_uri(), "WINLUX_STORE_APPIMAGEHUB": "1",
           "PATH": f"{bindir}:/usr/bin:/bin"}

    # Fenêtre de confirmation (#1517) : « Installer » sauf CONFIRM_ANSWER=1.
    mock(bindir, "confirm", f"printf '%s\\n' \"$@\" >> {w / 'confirm.log'}\nexit ${{CONFIRM_ANSWER:-0}}\n")
    env["WINLUX_STORE_CONFIRM"] = str(bindir / "confirm")

    # --- apt simulé : VLC installé avec mise à jour, GIMP disponible ---
    mock(bindir, "dpkg-query", "printf 'vlc\\tii \\t3.0.20-1\\t12000\\ngit\\tii \\t1:2.43\\t9000\\nlibc6\\tii \\t2.39\\t100\\n'\n")
    mock(bindir, "apt", "printf 'Listing...\\nvlc/noble-updates 3.0.21-1 amd64 [upgradable from: 3.0.20-1]\\nlibc6/noble-updates 2.40 amd64 [upgradable from: 2.39]\\n'\n")
    mock(bindir, "apt-cache", """case "$1" in
policy) shift; for p in "$@"; do echo "$p:"; case "$p" in 0ad|inconnu) echo '  Candidate: (none)';; *) echo '  Candidate: 1.0-1';; esac; done;;
show) printf 'Package: gimp\\nVersion: 2.10.36-3\\nInstalled-Size: 20480\\nHomepage: https://www.gimp.org\\nDescription: retouche\\n Premier paragraphe.\\n .\\n Second paragraphe.\\n';;
esac
""")
    mock(bindir, "apt-get", """printf 'apt-get %s\\n' "$*" >> "$LOG"
for a in "$@"; do case "$a" in APT::Status-Fd=*) fd=${a#APT::Status-Fd=};; esac; done
eval "echo 'pmstatus:x:60.0:Installation' >&$fd"
eval "echo 'pmstatus:abiword:90.0:Installed abiword (amd64)' >&$fd"
eval "echo 'pmstatus:shared-mime-info:99.0:Processing triggers for shared-mime-info (2.4-4)' >&$fd"
printf 'LC_ALL=%s\\n' "$LC_ALL" >> "$LOG"
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
remote-info) printf '%s\\n' 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef;;
info) printf '%s\\n' "${FP_INSTALLED:-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef}";;
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
    mock(bindir, "winlux", 'printf "winlux %s\\n" "$*" >> "$LOG"\n'
         'if [ "$1" = --msiexec ] && [ "$2" = --info ]; then printf "{AAAA-BBBB}\\tApp\\t1\\n"; fi\n')
    # Page du contrat de licence (#2129) : paquet sans contrat, l'installation suit.
    mock(bindir, "winlux-setup-wizard", 'printf "assistant %s\\n" "$*" >> "$LOG"\nexit 0\n')

    # --- Dépôt Scoop (archive .tar.gz de manifestes) ---
    pe64 = b"MZ" + b"\0" * 0x3A + (0x40).to_bytes(4, "little") + b"PE\0\0\x64\x86" + b"\0" * 64
    pe32 = pe64.replace(b"PE\0\0\x64\x86", b"PE\0\0\x4c\x01")
    def zip_of(name, exe):
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w") as z:
            z.writestr(f"Demo-2.0/{name}", exe)
            z.writestr("Demo-2.0/lisez-moi.txt", "ok")
        path = w / f"{name}.zip"
        path.write_bytes(buf.getvalue())
        return path, hashlib.sha256(buf.getvalue()).hexdigest()
    demo, demo_sha = zip_of("demo.exe", pe64)
    old32, old32_sha = zip_of("vieux.exe", pe32)
    manifests = {
        "demo": {"version": "2.0", "description": "Lecteur vidéo de démonstration",
                 "homepage": "https://www.demo.example.org/", "license": {"identifier": "MIT"},
                 "architecture": {"64bit": {"url": demo.as_uri() + "#/demo.zip", "hash": demo_sha}},
                 "extract_dir": "Demo-2.0", "shortcuts": [["demo.exe", "Démo Player"]]},
        "vieux": {"version": "1", "description": "Outil", "extract_dir": "Demo-2.0",
                  "architecture": {"64bit": {"url": old32.as_uri(), "hash": old32_sha}},
                  "shortcuts": [["vieux.exe", "Vieux"]]},
        "script": {"version": "1", "architecture": {"64bit": {"url": demo.as_uri(), "hash": demo_sha}},
                   "installer": {"script": "Remove-Item x"}, "shortcuts": [["a.exe", "Script"]]},
        "x86": {"version": "1", "url": demo.as_uri(), "hash": demo_sha, "shortcuts": [["a.exe", "Trente-deux"]]},
        "md5": {"version": "1", "architecture": {"64bit": {"url": demo.as_uri(), "hash": "md5:00"}},
                "shortcuts": [["a.exe", "Md5"]]},
    }
    tarball = w / "extras.tar.gz"
    with tarfile.open(tarball, "w:gz") as tar:
        for name, m in manifests.items():
            raw = json.dumps(m).encode()
            info = tarfile.TarInfo(f"Extras-master/bucket/{name}.json")
            info.size = len(raw)
            tar.addfile(info, io.BytesIO(raw))
    (conf / "scoop.json").write_text(json.dumps({"id": "scoop", "name": "Scoop Extras", "format": "scoop",
                                                 "file": str(tarball)}))

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
    check(gimp and gimp[1] == "graphisme" and gimp[5] == "a", "GIMP apt disponible")
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
    for c in ["internet", "bureautique", "multimedia", "graphisme", "developpement", "jeux", "systeme"]:
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
    # Paquets compagnons (#2075) : virt-manager arrive avec QEMU et le pilote
    # libvirt, marqués automatiques, jamais libvirt-daemon-system (systemd).
    st("install", "apt:virt-manager")
    text = log.read_text()
    check("install --no-install-recommends -- virt-manager" in text
          and "install --no-install-recommends --mark-auto -- qemu-system-x86 " in text
          and "libvirt-daemon-driver-qemu" in text and "libvirt-daemon-system" not in text,
          "compagnons de virt-manager (#2075)")
    st("update", "flatpak:org.kde.krita")
    check("flatpak update org.kde.krita -y --noninteractive" in log.read_text(), "mise à jour Flatpak")
    env["CONFIRM_ANSWER"] = "1"
    st("install", "windows:local/notepadpp", code=9)
    check("local_notepadpp" not in log.read_text()
          and not (home / ".cache/winlux/store/installers/local_notepadpp.exe").exists(),
          "programme Windows : Annuler, rien n'est téléchargé (#1517)")
    asked = (w / "confirm.log").read_text()
    check("Annuler" in asked and "Version : 8.6" in asked and "SHA-256" in asked and "garde vos documents" in asked,
          f"confirmation : version, adresse, empreinte, documents gardés ({asked[-300:]})")
    # Flathub : même confirmation, révision montrée puis vérifiée (#1678).
    rev = "0123456789abcdef" * 4
    st("install", "flatpak:org.gimp.GIMP", code=9)
    check("install --user" not in log.read_text(), "Flathub : Annuler, rien n'est installé (#1678)")
    asked = (w / "confirm.log").read_text()
    check("Révision installée : " + rev in asked and "Flathub (flathub) https://flathub.org/apps/org.gimp.GIMP" in asked
          and "Application Linux" in asked and "Annuler" in asked, f"confirmation Flathub ({asked[-300:]})")
    env["CONFIRM_ANSWER"] = "0"
    st("install", "flatpak:org.gimp.GIMP")
    text = log.read_text()
    check("flatpak install --user flathub org.gimp.GIMP -y --noninteractive" in text and "--commit=" not in text,
          "Flathub : installation de la révision confirmée")
    env["FP_INSTALLED"] = "f" * 64
    st("install", "flatpak:org.gimp.GIMP")
    env.pop("FP_INSTALLED")
    check(f"flatpak update --user --commit={rev} org.gimp.GIMP" in log.read_text(),
          "Flathub : le dépôt a avancé, retour à la révision confirmée")
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
    st("install-file", str(home / "a.msi"))
    check(f"winlux --msiexec /i {home}/a.msi /qn" in log.read_text(), "msiexec intégré")

    # Scoop : seules les applications 64 bits sans script sont proposées.
    check("windows:scoop/demo" in lines, "application Scoop listée")
    check(not any(k in lines for k in ("windows:scoop/script", "windows:scoop/x86", "windows:scoop/md5")),
          "manifestes Scoop non pris en charge écartés")
    d = lines["windows:scoop/demo"]
    check(d[2] == "Démo Player" and d[1] == "multimedia" and d[4] == "demo.example.org" and d[10] == "Scoop Extras",
          "fiche Scoop")
    st("install", "windows:scoop/demo")
    exe = home / "Program Files/Démo Player/demo.exe"
    lnk = home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Démo Player.lnk"
    check(exe.read_bytes() == pe64 and lnk.is_file(), "application portable installée")
    check("C:\\Program Files\\Démo Player\\demo.exe".encode("utf-16-le") in lnk.read_bytes(), "raccourci .lnk")
    st("install", "windows:scoop/vieux", code=8)
    check(not (home / "Program Files/Vieux").exists(), "programme 32 bits refusé proprement")
    st("remove", "windows:scoop/demo")
    check(not exe.parent.exists() and not lnk.exists(), "application portable désinstallée")
    msi_cat = w / "msi.json"
    (w / "app.msi").write_bytes(b"msi")
    msi_sha = hashlib.sha256(b"msi").hexdigest()
    msi_cat.write_text(json.dumps({"apps": [{"id": "outil", "name": "Outil MSI", "version": "3",
                                             "installer": {"url": (w / "app.msi").as_uri(), "sha256": msi_sha,
                                                           "type": "msi"}}]}))
    (conf / "msi.json").write_text(json.dumps({"id": "msi", "file": str(msi_cat)}))
    st("refresh")
    st("install", "windows:msi/outil")
    check("winlux --msiexec /i" in log.read_text() and "msi_outil.msi /qn" in log.read_text(), "paquet .msi du catalogue")
    state = json.loads((home / ".local/share/winlux/store-state.json").read_text())
    check(state["windows"]["msi/outil"]["msi"] == "{AAAA-BBBB}", "code produit MSI retenu")
    st("remove", "windows:msi/outil")
    check("winlux --msiexec /x {AAAA-BBBB} /qn" in log.read_text(), "désinstallation MSI")
    # Paquet Ubuntu réel hors du catalogue (police d'emojis, moteur OCR) :
    # installé directement au lieu de « Logiciel inconnu ».
    st("install", "apt:fonts-noto-color-emoji")
    check("install --no-install-recommends -- fonts-noto-color-emoji" in log.read_text(), "paquet hors catalogue installé")
    st("install", "apt:inconnu", code=2)
    check("Logiciel inconnu" in (run / "store-progress.txt").read_text(), "message en français")
    for code, text in (("en", "Unknown software"), ("es", "Software desconocido"), ("de", "Unbekannte Software")):
        subprocess.run([sys.executable, "-I", "-B", str(store), "install", "apt:inconnu"],
                       env={**env, "WINLUX_LANG": code}, capture_output=True, text=True)
        check(text in (run / "store-progress.txt").read_text(), f"message traduit ({code})")
    (run / "store.lock").mkdir()
    st("install", "apt:gimp", code=1)
    (run / "store.lock").rmdir()

    # Logiciels courants de l'appli Bienvenue : liste, file d'attente, reprise.
    def starter_state():
        rows = [l[4:].split("|") for l in (run / "starter.txt").read_text().splitlines() if l.startswith("app=")]
        return {r[0]: r for r in rows}
    st("starter", "--list")
    rows = starter_state()
    check(rows["vlc"][1] == "i" and rows["firefox"][1] == "a" and rows["firefox"][4] == "Ubuntu", "liste Bienvenue")
    check("notepadpp" not in rows, "logiciel sans source utilisable masqué")
    st("starter", "firefox", "7zip", "inconnu")
    text = log.read_text()
    check("install --no-install-recommends -- firefox" in text and "-- 7zip" in text, "installation groupée")
    check(not (home / ".local/share/winlux/starter-queue.txt").exists(), "file d'attente vidée")
    check("busy=0" in (run / "starter.txt").read_text() and starter_state()["firefox"][1] != "f", "état final")
    (home / ".local/share/winlux/starter-queue.txt").write_text("geany\n")
    st("starter", "--resume")
    check("-- geany" in log.read_text(), "reprise après fermeture")
    failing = w / "failbin"
    failing.mkdir()
    mock(failing, "apt-get", "exit 100\n")
    subprocess.run([sys.executable, "-I", "-B", str(store), "starter", "keepassxc"],
                   env={**env, "PATH": f"{failing}:{env['PATH']}"}, capture_output=True, text=True)
    check(starter_state()["keepassxc"][1] == "f", "échec signalé")
    st("starter", "keepassxc")
    check(starter_state()["keepassxc"][1] == "a", "nouvel essai après échec")
    # Un dossier d'échange par compte (WINLUX_STORE_RUN) : un second compte
    # ne pouvait pas remplacer l'index écrit par le premier (/run/winlux sticky).
    mine = w / "run-compte"
    r = subprocess.run([sys.executable, "-I", "-B", str(store), "refresh"],
                       env={**env, "WINLUX_STORE_RUN": str(mine)}, capture_output=True, text=True)
    check(r.returncode == 0 and "refreshing=0" in (mine / "store.txt").read_text(), "index dans le dossier du compte")
    launcher = (project / "scripts/winlux-shell-app.sh").read_text()
    check('"/run/winlux/store-$uid"' in launcher and "test -O" in launcher and "export WINLUX_STORE_RUN" in launcher,
          "winlux-shell-app donne au Store un dossier propre au compte")
    ui = (project / "testexe/src/shell/store.rs").read_text()
    check('c"WINLUX_STORE_RUN"' in ui and 'c"Z:\\run\\winlux\\store' not in ui, "la fenêtre lit le dossier du compte")
    # Désinstallation apt, et toutes les commandes apt-get du Store acceptées par
    # le relais winlux-root (sans quoi « Désinstaller » ne faisait rien, v0.2.17).
    st("remove", "apt:vlc")
    check("purge --autoremove -- vlc" in log.read_text(), "désinstallation apt")
    relay = runpy.run_path(str(project / "scripts/winlux-root.py"), run_name="root_test")
    apt_lines = [l.split()[1:] for l in log.read_text().splitlines() if l.startswith("apt-get ")]
    check(any("purge" in a for a in apt_lines) and any("install" in a for a in apt_lines), "commandes apt relevées")
    for args in apt_lines:
        check(relay["exec_level"](["apt-get", *args]) == "admin", f"refusée par winlux-root : apt-get {' '.join(args)}")
    # Messages d'apt traduits, jamais l'anglais brut d'APT::Status-Fd.
    check("LC_ALL=C.UTF-8" in log.read_text(), "apt lancé en C.UTF-8 pour des messages prévisibles")
    ns = runpy.run_path(str(store), run_name="store_test")
    for lang, expected in (("fr", "abiword installé"), ("en", "abiword installed"),
                           ("es", "abiword instalado"), ("de", "abiword installiert")):
        ns["LANG"] = lang
        apt_message = ns["apt_message"]
        apt_message.__globals__["LANG"] = lang
        check(apt_message("pmstatus", "Installed abiword (amd64)") == expected, f"message apt traduit ({lang})")
        check(apt_message("pmstatus", "Unpacking abiword:amd64 (3.0.5-1)") != "Unpacking abiword:amd64 (3.0.5-1)",
              f"décompression traduite ({lang})")
        check(apt_message("dlstatus", "Retrieving file 1 of 3") == ns["T"][lang]["apt_download"], f"téléchargement ({lang})")
        check("shared-mime-info" in apt_message("pmstatus", "Processing triggers for shared-mime-info (2.4-4)")
              and apt_message("pmstatus", "Processing triggers for shared-mime-info (2.4-4)") != ns["T"][lang]["apt_working"],
              f"déclencheurs de dpkg expliqués ({lang}, #2186)")
        check(apt_message("pmstatus", "Quelque chose d'inconnu") == ns["T"][lang]["apt_working"], f"repli générique ({lang})")
    check("Installed abiword" not in (run / "store-progress.txt").read_text(), "pas d'anglais brut dans le Store")

    # Sélection de l'équipe (#716) : au moins 250 logiciels, chacun décrit
    # dans les quatre langues, rangés dans une catégorie connue.
    rows = [l.split("|") for l in (project / "integration/store-catalog.txt").read_text().splitlines()
            if l and not l.startswith("#") and not l.startswith(("featured=", "compagnons="))]
    check(len(rows) >= 250, f"sélection étendue ({len(rows)} logiciels)")
    check(all(len(r) == 9 and all(r[5:]) for r in rows), "descriptions fr/en/es/de")
    check(all(r[1] in ns["CATEGORIES"] for r in rows), "catégories connues")
    check(len({r[0] for r in rows}) == len(rows) and len({r[3] for r in rows}) == len(rows), "ni identifiant ni paquet en double")
    browsers = {r[0] for r in rows if r[1] == "internet" and "navigat" in r[5].lower()}
    featured = next(l[9:] for l in (project / "integration/store-catalog.txt").read_text().splitlines()
                    if l.startswith("featured=")).split(",")
    check(not browsers & set(featured), "aucun navigateur en vedette (docs/BROWSERS.md)")
    for lang, word in (("en", "Plays almost"), ("es", "Reproduce casi"), ("de", "Spielt fast"), ("fr", "Lit presque")):
        ns["LANG"] = lang
        ns["Apt"].curated.__globals__["LANG"] = lang
        ns["Apt"].curated.__globals__["CATALOG"] = project / "integration/store-catalog.txt"
        check(ns["Apt"]().curated()["vlc"]["desc"].startswith(word), f"description de la sélection ({lang})")
    check(ns["category_of"](["Education", "Science", "Astronomy"]) == "sciences"
          and ns["category_of"](["Game", "Education"]) == "jeux"
          and ns["category_of"](["Graphics", "Photography"]) == "graphisme"
          and ns["known_category"]("navigateurs") == "internet", "règles de catégories")
    # Le Store (store.rs) affiche tout l'index et connaît les mêmes catégories.
    import re as _re
    rs = (project / "testexe/src/shell/store.rs").read_text()
    check(int(_re.search(r"const MAX_APPS: usize = (\d+);", rs)[1]) >= ns["MAX_INDEX"],
          "le Store lit autant d'applications que l'index en contient")
    cats_rs = rs[rs.index("const CATS:"):rs.index("const JEUX:")]
    keys_rs = _re.findall(r'\(\s*b"([a-z]+)",\s*\[', cats_rs)
    check(sorted(keys_rs) == sorted(ns["CATEGORIES"]), f"mêmes catégories dans store.rs ({keys_rs})")
    check(keys_rs.index("jeux") == int(_re.search(r"const JEUX: usize = (\d+);", rs)[1])
          and keys_rs.index("utilitaires") == int(_re.search(r"const UTILITAIRES: usize = (\d+);", rs)[1]),
          "indices JEUX et UTILITAIRES")
    print("Store : OK")
