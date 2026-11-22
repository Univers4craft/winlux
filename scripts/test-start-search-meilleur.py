#!/usr/bin/python3
"""Recherche du menu Démarrer : le Meilleur résultat est le mieux classé,
application ou page des Paramètres (#3018).

Une application « Copier le texte de l'écran » et l'application Paramètres.
« écran » : la page Paramètres « Écran » (nom qui commence par la recherche)
est le Meilleur résultat, l'application (un seul de ses mots commence par
la recherche) vient ensuite sous « Applications », et Entrée ouvre la page.
« paramètres » : à rang égal, l'application Paramètres reste en tête.

Ce qui est supposé : Microsoft ne publie pas la règle de « Meilleur
résultat » ; la règle testée est celle de rank() (nom qui commence par la
recherche, puis un mot qui commence par elle).

    xvfb-run -a dbus-run-session -- python3 -B scripts/test-start-search-meilleur.py
"""
import json
import os
from pathlib import Path
import runpy
import tempfile

HERE = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp(prefix="winlux-start-meilleur-"))
home = tmp / "home"
os.environ.update(HOME=str(home), WINLUX_LANG="fr", WINLUX_OSK_ROOT=str(tmp), XDG_DATA_HOME=str(home / ".local/share"))
os.environ.pop("XDG_CONFIG_HOME", None)
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "proc/bus/input/devices").write_text('N: Name="AT Translated Set 2 keyboard"\nH: Handlers=kbd event0\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
applications = home / ".local/share/applications"
applications.mkdir(parents=True)
(applications / "winlux-settings.desktop").write_text("[Desktop Entry]\nType=Application\nName=Paramètres\nExec=true\nCategories=Settings;\n")
(applications / "essai-ocr.desktop").write_text("[Desktop Entry]\nType=Application\nName=Copier le texte de l'écran\n"
                                                "Exec=true\nCategories=Utility;\n")
(home / ".config/winlux").mkdir(parents=True)
(home / ".config/winlux/start.json").write_text(json.dumps({"pins": [], "recent": []}))

module = runpy.run_path(str(HERE / "winlux-start-menu.py"), run_name="start_meilleur_test")
Gtk, GLib = module["Gtk"], module["GLib"]
app = module["Start"]()
problems = []


def texts(widget, found):
    if isinstance(widget, Gtk.Label): found.append(widget.get_text())
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): texts(child, found)
    return found


def shown(query):
    app.search.set_text(query)
    app.render()
    return [text for text in texts(app.content, []) if text]


def check():
    try:
        labels = shown("écran")
        print(f"      « écran » : {labels[:6]}")
        assert labels[:2] == ["Meilleur résultat", "Écran"], labels
        assert labels.index("Applications") < labels.index("Copier le texte de l'écran"), labels
        opened, launched = [], []
        app.open_setting = opened.append
        app.launch = launched.append
        app.first_result()
        print(f"      Entrée sur « écran » : page {opened}, application {[a.get_display_name() for a in launched]}")
        assert len(opened) == 1 and not launched, (opened, launched)
        labels = shown("paramètres")
        print(f"      « paramètres » : {labels[:4]}")
        assert labels[:2] == ["Meilleur résultat", "Paramètres"], labels
    except AssertionError as error:
        problems.append(error)
    app.quit()
    return False


def started():
    if not app.window: return True
    GLib.timeout_add(300, check)
    return False


GLib.timeout_add(300, started)
app.run([])
if problems: raise problems[0]
print("OK: Meilleur résultat de la recherche du menu Démarrer, application ou page des Paramètres (#3018)")
