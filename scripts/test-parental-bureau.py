#!/usr/bin/python3
"""#2293 : un blocage parental de gui_demo.exe (directement, ou par l'empreinte
d'une copie renommée) fermait l'Explorateur, la Boutique, le Gestionnaire des
tâches et le Bloc-notes PE du compte surveillé. Le vrai programme du bureau
n'est jamais fermé ; une copie qui en prend le nom reste reconnue.

Racine factice (WINLUX_PARENTAL_ROOT), sans root ni processus réel.
"""
import importlib.util
import json
import os
import shutil
import sys
import tempfile
from importlib.machinery import SourceFileLoader
from pathlib import Path

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-parental-bureau."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"


def check(condition, message):
    if not condition:
        print("ERREUR :", message, file=sys.stderr)
        sys.exit(1)
    print("  OK    ", message)


try:
    (work / "etc/winlux/parental").mkdir(parents=True)
    loader = SourceFileLoader("winlux_parental_bureau", str(HERE / "winlux-parental.py"))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    lib = importlib.util.module_from_spec(spec)
    loader.exec_module(lib)

    shell = work / "opt/winlux/tests/gui_demo.exe"
    shell.parent.mkdir(parents=True)
    shell.write_bytes(b"MZ" + os.urandom(9000))
    docs = work / "home/enfant/Documents"
    docs.mkdir(parents=True)
    shutil.copy(shell, docs / "devoirs.exe")          # copie renommée du bureau, bloquée
    (docs / "jeu.exe").write_bytes(b"MZ" + os.urandom(7000))
    fake = docs / "faux" / "gui_demo.exe"             # jeu bloqué renommé en gui_demo.exe
    fake.parent.mkdir()
    shutil.copy(docs / "jeu.exe", fake)

    # Règles enregistrées avant le correctif : gui_demo.exe bloqué avec d'autres.
    (work / "etc/winlux/parental/enfant.json").write_text(json.dumps({
        "enabled": True,
        "apps": [{"id": "exe:gui_demo.exe", "name": "gui_demo"}, {"id": "bin:mousepad", "name": "Mousepad"},
                 {"id": "exe:devoirs.exe", "name": "devoirs"}, {"id": "exe:jeu.exe", "name": "jeu"}]}))
    cfg = lib.load_config("enfant")
    ids = [app["id"] for app in cfg["apps"]] if cfg else []
    check(cfg is not None and cfg["enabled"] and ids == ["bin:mousepad", "exe:devoirs.exe", "exe:jeu.exe"],
          "ancienne règle exe:gui_demo.exe retirée, les autres règles du compte gardées")
    check(lib.protected("exe:gui_demo.exe") and lib.protected("exe:GUI_DEMO.EXE".lower())
          and not lib.protected("exe:devoirs.exe"), "gui_demo.exe compté parmi les programmes du bureau")

    blocked = lib.blocked_ids(cfg)
    daemon = lib.Daemon()
    winlux = "/usr/bin/winlux"
    # La copie renommée est lancée d'abord : son empreinte (celle du bureau) est apprise.
    check(daemon.identify("enfant", 4001, winlux, ["winlux", str(docs / "devoirs.exe")], "exe:devoirs.exe", blocked)
          == "exe:devoirs.exe", "copie renommée du bureau, bloquée sous son nom : fermée")
    check(daemon.identify("enfant", 4002, winlux, ["winlux", str(docs / "jeu.exe")], "exe:jeu.exe", blocked)
          == "exe:jeu.exe", "jeu bloqué : fermé")
    check(daemon.identify("enfant", 4003, winlux, ["winlux", str(shell)], "exe:gui_demo.exe", blocked) is None,
          "Explorateur (vrai gui_demo.exe) jamais fermé, même avec une copie bloquée de même empreinte")
    check(daemon.identify("enfant", 4004, winlux, ["winlux", str(fake)], "exe:gui_demo.exe", blocked) == "exe:jeu.exe",
          "jeu bloqué renommé gui_demo.exe : toujours reconnu par son empreinte et fermé")
    try:
        lib.validate({"apps": [{"id": "exe:gui_demo.exe"}]})
        refused = False
    except ValueError:
        refused = True
    check(refused, "le bureau ne peut pas être ajouté aux applications bloquées")
    print("OK : applications du bureau Winlux jamais fermées par le contrôle parental (#2293)")
finally:
    shutil.rmtree(work, ignore_errors=True)
