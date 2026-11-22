#!/usr/bin/python3
"""Recherche du menu Démarrer : aucun chemin AppData ni marque Microsoft à
l'écran (#2905).

Un dossier personnel avec le groupe « PuTTY (64-bit) » du menu Démarrer
(AppData/Roaming/Microsoft/Windows/Start Menu/Programs) et un putty.exe
installé dans AppData/Local/Programs. La recherche « putty » ne montre pas
le groupe du menu, montre putty.exe sous « Données des programmes › PuTTY »,
et aucun texte ni infobulle du menu ne contient « Microsoft » ou « AppData ».

    xvfb-run -a dbus-run-session -- python3 -B scripts/test-start-search-appdata.py
"""
import json
import os
from pathlib import Path
import runpy
import tempfile

HERE = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp(prefix="winlux-start-appdata-"))
home = tmp / "home"
os.environ.update(HOME=str(home), WINLUX_LANG="fr", WINLUX_OSK_ROOT=str(tmp), XDG_DATA_HOME=str(home / ".local/share"))
os.environ.pop("XDG_CONFIG_HOME", None)
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "proc/bus/input/devices").write_text('N: Name="AT Translated Set 2 keyboard"\nH: Handlers=kbd event0\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
group = home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs/PuTTY (64-bit)"
group.mkdir(parents=True)
(group / "PuTTY.lnk").write_bytes(b"L")
(group / "PuTTY Manual.lnk").write_bytes(b"L")
(home / "AppData/Local/Programs/PuTTY").mkdir(parents=True)
(home / "AppData/Local/Programs/PuTTY/putty.exe").write_bytes(b"MZ")
(home / "Documents").mkdir()
(home / "Documents/putty-notes.txt").write_text("x")
(home / ".local/share/applications").mkdir(parents=True)
(home / ".config/winlux").mkdir(parents=True)
(home / ".config/winlux/start.json").write_text(json.dumps({"pins": [], "recent": []}))

module = runpy.run_path(str(HERE / "winlux-start-menu.py"), run_name="start_appdata_test")
place = module["file_place"]
assert place(group / "PuTTY.lnk", home) == "Menu Démarrer", place(group / "PuTTY.lnk", home)
assert place(home / "AppData/Local/Programs/PuTTY/putty.exe", home) == "Données des programmes › PuTTY"
assert place(home / "AppData/Roaming/Microsoft/x.dat", home) == "Données des programmes"
assert place(home / "Documents/a.txt", home) == "~/Documents" and place(home / "a.txt", home) == "~"

Gtk, GLib = module["Gtk"], module["GLib"]
app = module["Start"]()
problems = []


def texts(widget, found):
    if isinstance(widget, Gtk.Label): found.append(widget.get_text())
    if isinstance(widget, Gtk.Widget) and widget.get_tooltip_text(): found.append(widget.get_tooltip_text())
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): texts(child, found)
    return found


def search():
    app.search.set_text("putty")
    app.render()
    GLib.timeout_add(1500, check)
    return False


def check():
    try:
        names = [path.name for path in app.file_results]
        assert "PuTTY (64-bit)" not in names, names
        assert "putty.exe" in names and "putty-notes.txt" in names, names
        shown = texts(app.window, [])
        assert "Données des programmes › PuTTY" in shown and "~/Documents" in shown, shown
        leaks = [text for text in shown if "microsoft" in text.casefold() or "appdata" in text.casefold()]
        assert not leaks, leaks
    except AssertionError as error:
        problems.append(error)
    app.quit()
    return False


def started():
    if not app.window: return True
    GLib.timeout_add(300, search)
    return False


GLib.timeout_add(300, started)
app.run([])
if problems: raise problems[0]
print("OK: recherche du menu Démarrer sans chemin AppData ni marque Microsoft (#2905)")
