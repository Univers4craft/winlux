#!/usr/bin/env python3
"""#2890 : la Boutique voit un programme Windows installé sans elle.

Un programme inscrit dans « Applications installées » (clé Uninstall du
registre, lue par `winlux --setup --programs`) compte comme installé sur sa
fiche : pas de bouton « Installer », jamais de « mise à jour » vers une
version plus ancienne, et « Désinstaller » passe par son propre désinstalleur
(`winlux --setup --remove CLÉ`). Une version plus ancienne que celle du
catalogue est proposée en mise à jour."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
store = project / "scripts/winlux-store.py"
failures = []


def ok(cond, label):
    print(("OK    " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)


with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    home, bindir, run = w / "home", w / "bin", w / "run"
    for d in (home, bindir, run):
        d.mkdir()
    log, programs = w / "log", w / "programmes.txt"
    installer = w / "setup.exe"
    installer.write_bytes(b"MZ installateur")
    digest = hashlib.sha256(installer.read_bytes()).hexdigest()
    catalog = w / "windows.json"
    catalog.write_text(json.dumps({"apps": [
        {"id": "notepadplusplus", "name": "Notepad++", "version": "8.6.9", "publisher": "Don Ho",
         "installer": {"url": installer.as_uri(), "sha256": digest}},
        {"id": "vlc", "name": "VLC", "version": "3.0.21", "publisher": "VideoLAN",
         "installer": {"url": installer.as_uri(), "sha256": digest}}]}))
    conf = home / ".config/winlux/store-sources.d"
    conf.mkdir(parents=True)
    (conf / "local.json").write_text(json.dumps({"id": "local", "name": "Catalogue local", "file": str(catalog)}))
    # Runtime simulé : --setup --programs rend le registre du test, chaque
    # appel est noté.
    (bindir / "winlux").write_text(f"""#!/bin/sh
printf 'winlux %s\\n' "$*" >> '{log}'
if [ "$1 $2" = "--setup --programs" ]; then cat '{programs}'; echo done=1; fi
exit 0
""")
    (bindir / "winlux").chmod(0o755)
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_DRIVE_C": str(home),
           "XDG_DATA_HOME": str(home / ".local/share"), "XDG_CACHE_HOME": str(home / ".cache"),
           "XDG_CONFIG_HOME": str(home / ".config"), "WINLUX_STORE_RUN": str(run),
           "WINLUX_STORE_OFFLINE": "1", "WINLUX_CONTROL_ROOT": str(w / "root"), "WINLUX_LANG": "fr"}

    def store_run(*args):
        log.write_text("")
        r = subprocess.run([sys.executable, "-I", "-B", str(store), *args], env=env,
                           capture_output=True, text=True, timeout=120)
        return r.returncode, log.read_text().splitlines()

    def card(ref):
        index = (run / "store.txt").read_text().splitlines() if (run / "store.txt").is_file() else []
        line = next((l for l in index if l.startswith(f"app=windows:{ref}|")), "")
        f = line[4:].split("|")
        return (f[5], f[6], f[7]) if len(f) > 7 else None

    def registry(*rows):
        programs.write_text("".join(f"app={k}\t{n}\t{v}\tNotepad++ Team\t7000\t20261122\tnsis\n"
                                    for k, n, v in rows))

    # Banc 2 du 22 novembre : 8.9.9.1 installée à la main, catalogue en 8.6.9.
    registry(("Notepad++", "Notepad++ (64-bit x64)", "8.9.9.1"))
    store_run("refresh")
    ok(card("local/notepadplusplus") == ("i", "8.9.9.1", "8.6.9"),
       f"installée hors Boutique, plus récente : installée, ni Installer ni Mettre à jour ({card('local/notepadplusplus')})")
    ok(card("local/vlc") == ("a", "", "3.0.21"), f"programme absent du registre : à installer ({card('local/vlc')})")

    # Version installée plus ancienne : mise à jour proposée.
    registry(("Notepad++", "Notepad++ (64-bit x64)", "8.5"))
    store_run("refresh")
    ok(card("local/notepadplusplus") == ("u", "8.5", "8.6.9"),
       f"installée hors Boutique, plus ancienne : mise à jour ({card('local/notepadplusplus')})")

    # Même version : installée.
    registry(("Notepad++", "Notepad++ (64-bit x64)", "8.6.9"))
    store_run("refresh")
    ok(card("local/notepadplusplus") == ("i", "8.6.9", "8.6.9"), f"même version : installée ({card('local/notepadplusplus')})")

    # Un autre programme dont le nom commence pareil ne compte pas.
    registry(("NotepadPlus", "Notepad++Plus 2.0", "2.0"), ("VLCx", "VLCPlayerPro 1.0", "1.0"))
    store_run("refresh")
    ok(card("local/notepadplusplus")[0] == "a" and card("local/vlc")[0] == "a",
       "nom seulement voisin (« Notepad++Plus », « VLCPlayerPro ») : pas installé")

    # Désinstaller passe par le désinstalleur du programme.
    registry(("Notepad++", "Notepad++ (64-bit x64)", "8.9.9.1"))
    store_run("refresh")
    code, calls = store_run("remove", "windows:local/notepadplusplus")
    ok(code == 0 and "winlux --setup --remove Notepad++" in calls,
       f"Désinstaller : désinstalleur du programme inscrit ({code}, {calls})")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique : programme Windows installé hors Boutique vu comme installé (#2890)")
