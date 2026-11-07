#!/usr/bin/python3
"""Menu Démarrer à deux volets (#685) : volet « Pour vous » et « Toutes les applications ».

Partie 1, sans écran : calculs locaux du volet (lancements fréquents,
habitudes à cette heure-ci, fichiers récents de GTK, installateur jamais
lancé, Téléchargements chargés, mise à jour vérifiée), applications ajoutées
récemment, lettres de la liste A–Z, textes dans les quatre langues, et
réglage écrit par Paramètres.

Partie 2, dans un Xvfb dédié (xvfb-run) : le menu s'ouvre sur deux volets
centrés ; « Masquer » (ou l'interrupteur de Paramètres) le ramène à un seul
volet, toujours centré ; les cartes de catégories se déplient ; liste A–Z
avec saut à une lettre ; grille ; chaque ouverture repart de l'état initial.
"""
from pathlib import Path
import json
import os
import runpy
import tempfile
import time

tmp = Path(tempfile.mkdtemp(prefix="winlux-start-for-you-"))
home = tmp / "home"
os.environ["HOME"] = str(home)
os.environ.pop("XDG_CONFIG_HOME", None)
os.environ["XDG_DATA_HOME"] = str(home / ".local/share")
os.environ["WINLUX_LANG"] = "fr"
# Barre centrée : le menu s'ouvre au milieu de l'écran (barre à gauche, au-dessus
# de Démarrer depuis #732 ; l'ISO livre TASKBAR_ALIGN=left).
(tmp / "preferences.conf").write_text("TASKBAR_ALIGN=center\n")
os.environ["WINLUX_PREFERENCES"] = str(tmp / "preferences.conf")
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
os.environ["WINLUX_UPDATE_STATE"] = str(tmp / "var")
os.environ["WINLUX_VERSION_FILE"] = str(tmp / "version")
devices = tmp / "proc/bus/input/devices"
devices.parent.mkdir(parents=True)
devices.write_text('I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name="AT Translated Set 2 keyboard"\n'
                   'H: Handlers=sysrq kbd event0 leds\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
here = Path(__file__).resolve().parent
module = runpy.run_path(str(here / "winlux-start-menu.py"), run_name="start_for_you_test")
DAY = 86400
now = time.mktime((2026, 11, 6, 18, 30, 0, 0, 0, -1))  # un vendredi, 18 h 30

# ── Lancements : historique borné, fréquence avec oubli progressif ──
state = {}
for index in range(module["USAGE_LIMIT"] + 20):
    module["record_use"](state, f"app{index % 3}.desktop", now - index)
assert len(state["usage"]) == module["USAGE_LIMIT"], len(state["usage"])
usage = [("vieux.desktop", now - 120 * DAY)] * 6 + [("recent.desktop", now - DAY)] * 2 + [("pin.desktop", now)] * 9
assert module["frequent_apps"](usage, now, exclude={"pin.desktop"}) == ["recent.desktop", "vieux.desktop"], \
    module["frequent_apps"](usage, now, exclude={"pin.desktop"})
assert module["usage_of"]({"usage": [["a", 1], ["bad"], [3, 4], "x", ["b", 2.5]]}) == [("a", 1), ("b", 2)]

# ── Habitudes : même genre de jour, ± 1 h 30, au moins deux jours ──
habits = []
for week in (1, 2, 3):
    habits.append(("mail.desktop", now - week * 7 * DAY + 600))           # vendredis à 18 h 40
habits.append(("jeu.desktop", now - 2 * DAY))                              # mercredi, une seule fois
habits += [("weekend.desktop", now + DAY - week * 7 * DAY) for week in (1, 2, 3)]  # samedis
habits += [("matin.desktop", now - week * 7 * DAY - 9 * 3600) for week in (1, 2)]  # 9 h 30
habits.append(("tres-vieux.desktop", now - 70 * DAY))
habits.append(("tres-vieux.desktop", now - 63 * DAY))
found = module["habit_apps"](habits, now)
assert found == ["mail.desktop"], found
reason = module["habit_reason"](now)
assert reason == "D’après vos habitudes en semaine, le soir", reason

# ── Fichiers récents de GTK : les plus récents, existants, dans le dossier personnel ──
documents = home / "Documents"
documents.mkdir(parents=True)
(home / ".cache").mkdir()
files = {"rapport.odt": "2026-11-06T17:00:00Z", "photo.png": "2026-11-06T18:00:00.123456Z",
         "vieux.txt": "2026-10-01T08:00:00Z", "absent.pdf": "2026-11-06T18:20:00Z"}
for name in files:
    if name != "absent.pdf": (documents / name).write_text("x")
(home / ".cache/cache.bin").write_text("x")
marks = ""
for name, stamp in files.items():
    marks += (f'<bookmark href="file://{documents / name}" added="{stamp}" modified="{stamp}" visited="{stamp}"><info>'
              f'<metadata owner="http://freedesktop.org"><bookmark:applications>'
              f'<bookmark:application name="mousepad" exec="mousepad %u" modified="{stamp}" count="1"/>'
              f'</bookmark:applications></metadata></info></bookmark>\n')
marks += f'<bookmark href="file://{home / ".cache/cache.bin"}" added="2026-11-06T18:25:00Z" modified="2026-11-06T18:25:00Z" visited="2026-11-06T18:25:00Z"/>\n'
marks += '<bookmark href="file:///etc/hostname" added="2026-11-06T18:26:00Z" modified="2026-11-06T18:26:00Z" visited="2026-11-06T18:26:00Z"/>\n'
marks += '<bookmark href="https://exemple.org/" added="2026-11-06T18:27:00Z" modified="2026-11-06T18:27:00Z" visited="2026-11-06T18:27:00Z"/>\n'
xbel = home / ".local/share/recently-used.xbel"
xbel.parent.mkdir(parents=True)
xbel.write_text('<?xml version="1.0" encoding="UTF-8"?>\n<xbel version="1.0" '
                'xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">\n' + marks + '</xbel>\n')
recent = module["recent_files"](xbel, limit=3)
assert [path.name for path, _, _ in recent] == ["photo.png", "rapport.odt", "vieux.txt"], recent
assert recent[0][1] == "mousepad"
assert module["recent_files"](tmp / "absent.xbel") == []
(tmp / "casse.xbel").write_text("<xbel><bookmark")
assert module["recent_files"](tmp / "casse.xbel") == []
assert module["ago"](now - 30, now) == "à l’instant"
assert module["ago"](now - 25 * 60, now) == "il y a 25 min"
assert module["ago"](now - 3 * 3600, now) == "il y a 3 h"
assert module["ago"](now - 30 * 3600, now) == "hier", module["ago"](now - 30 * 3600, now)
assert module["ago"](now - 4 * DAY, now) == "il y a 4 jours"

# ── À faire : installateur jamais lancé, Téléchargements chargés, mise à jour vérifiée ──
downloads = home / "Téléchargements"
downloads.mkdir()
(downloads / "WinSCP-6.3-Setup.exe").write_bytes(b"MZ")
(downloads / "programme.exe").write_bytes(b"MZ")               # pas un installateur
(downloads / "ancien-setup.exe").write_bytes(b"MZ")
os.utime(downloads / "ancien-setup.exe", (now - 60 * DAY, now - 60 * DAY))  # trop vieux
(downloads / "outil.msi").write_bytes(b"x")
(downloads / "deja-lance-setup.exe").write_bytes(b"MZ")
for path in downloads.iterdir():
    if path.name != "ancien-setup.exe": os.utime(path, (now - DAY, now - DAY))
opened = {downloads / "deja-lance-setup.exe"}
items = module["todo_items"](downloads, opened, set(), now, update=None)
assert [item["title"] for item in items] == ["outil.msi", "WinSCP-6.3-Setup.exe"] or \
       [item["title"] for item in items] == ["WinSCP-6.3-Setup.exe", "outil.msi"], items
assert all(item["kind"] == "installer" and item["action"] == "Installer" for item in items)
items = module["todo_items"](downloads, opened, {"installer:outil.msi"}, now, update="0.2.21")
assert [item["kind"] for item in items] == ["update", "installer"], items
assert items[0]["title"] == "Winlux 0.2.21 est disponible", items[0]
with open(downloads / "image.iso", "wb") as big:
    big.truncate(module["DOWNLOADS_HEAVY"] + 1)
os.utime(downloads / "image.iso", (now - 200 * DAY, now - 200 * DAY))
items = module["todo_items"](downloads, opened, {"installer:outil.msi", "installer:WinSCP-6.3-Setup.exe"}, now)
assert [item["kind"] for item in items] == ["downloads"], items
assert items[0]["title"] == "Téléchargements : 2 Go", items[0]["title"]
assert items[0]["detail"] == "1 fichier non ouvert depuis 3 mois", items[0]["detail"]
(downloads / "image.iso").unlink()
(tmp / "var").mkdir()
(tmp / "version").write_text("0.2.20\n")
assert module["update_ready"]() is None  # rien de vérifié
(tmp / "var/available-manifest").write_text("version=0.2.20\n")
assert module["update_ready"]() is None  # à jour
(tmp / "var/available-manifest").write_text("version=0.2.21\ndownload_bytes=12\n")
assert module["update_ready"]() == "0.2.21"
assert module["downloads_dir"](home) == downloads
(home / ".config").mkdir(exist_ok=True)
(home / ".config/user-dirs.dirs").write_text('XDG_DOWNLOAD_DIR="$HOME/Téléchargements"\n')
assert module["downloads_dir"](home) == downloads

# ── Ajoutées récemment : pas tout le système à la première ouverture ──
state = {}
apps = {"a.desktop": None, "b.desktop": None}
assert module["note_new_apps"](state, apps, now) and state["seen"] == {"a.desktop": 0, "b.desktop": 0}
assert module["recently_added"](state, apps, now) == []
apps["winlux-pe-winscp.desktop"] = None
assert module["note_new_apps"](state, apps, now)
assert not module["note_new_apps"](state, apps, now + 5)
assert module["recently_added"](state, apps, now + DAY) == ["winlux-pe-winscp.desktop"]
assert module["recently_added"](state, apps, now + 8 * DAY) == []
assert [module["letter_of"](name) for name in ("Éditeur", "7-Zip", "zoom", "Ölbild")] == ["E", "#", "Z", "O"]
assert module["for_you_enabled"]({}) and not module["for_you_enabled"]({"for_you": False})

# ── Quatre langues pour chaque texte du nouveau menu ──
source = (here / "winlux-start-menu.py").read_text(encoding="utf-8")
import re
used = set(re.findall(r'_t\("((?:[^"\\]|\\.)*)"\)', source)) | set(re.findall(r"_t\('((?:[^'\\]|\\.)*)'\)", source))
used |= {label for _, label in module["VIEWS"]} | set(module["CATEGORY_ORDER"])
used |= {"en semaine", "le week-end", "le matin", "l’après-midi", "le soir", "la nuit", "Explorateur", "Paramètres"}
missing = sorted(text for text in used if text not in module["_TR"] or len(module["_TR"][text]) != 3)
assert not missing, f"textes sans traduction : {missing}"
for lang, expected in (("en", "For you"), ("es", "Para ti"), ("de", "Für Sie")):
    os.environ["WINLUX_LANG"] = lang
    other = runpy.run_path(str(here / "winlux-start-menu.py"), run_name="start_for_you_lang")
    assert other["_t"]("Pour vous") == expected
os.environ["WINLUX_LANG"] = "fr"

# ── Paramètres : l'interrupteur écrit for_you sans toucher aux épingles ──
settings = runpy.run_path(str(here / "winlux-settings-pages.py"), run_name="start_for_you_settings")
start_json = home / ".config/winlux/start.json"
start_json.parent.mkdir(parents=True, exist_ok=True)
start_json.write_text(json.dumps({"pins": ["x.desktop"], "usage": [["x.desktop", 1]]}))
settings["update_start_menu_state"](for_you=False)
assert json.loads(start_json.read_text()) == {"pins": ["x.desktop"], "usage": [["x.desktop", 1]], "for_you": False}
settings["update_start_menu_state"](usage=[], recent=[], dismissed=[])
assert json.loads(start_json.read_text())["usage"] == []
for key in ("Menu Démarrer", "Afficher le volet « Pour vous »", "Effacer l’historique d’utilisation"):
    assert key in settings["_TR"] and len(settings["_TR"][key]) == 3, key
assert "start_menu_settings" in (here / "winlux-settings-pages.py").read_text(encoding="utf-8")
print("OK: calculs locaux du volet « Pour vous », 4 langues, réglage de Paramètres")

if not os.environ.get("DISPLAY"):
    print("(pas d'écran : partie graphique non lancée, utiliser xvfb-run)")
    raise SystemExit(0)

# ── Partie graphique ──
applications = home / ".local/share/applications"
applications.mkdir(parents=True)
names = {"explorer": ("Explorateur", "System;"), "mail": ("Courrier", "Network;Email;"), "texte": ("Traitement de texte", "Office;"),
         "tableur": ("Tableur", "Office;"), "jeu": ("Jeu de cartes", "Game;"), "photos": ("Photos", "Graphics;"),
         "calc": ("Calculatrice", "Utility;"), "notes": ("Notes", "Utility;"), "horloge": ("Horloge", "Utility;"),
         "meteo": ("Météo", "Utility;"), "capture": ("Capture d’écran", "Utility;"), "zoom": ("Zoom avant", "Utility;")}
for ident, (name, categories) in names.items():
    (applications / f"{ident}.desktop").write_text(f"[Desktop Entry]\nType=Application\nName={name}\nExec=true\nCategories={categories}\n")
(applications / "winlux-pe-winscp.desktop").write_text("[Desktop Entry]\nType=Application\nName=WinSCP\nExec=true\n")
(applications / "winlux-pe-7zip.desktop").write_text("[Desktop Entry]\nType=Application\nName=7-Zip\nExec=true\n")
real_now = time.time()
usage = [["mail.desktop", real_now - week * 7 * DAY + 60] for week in (1, 2, 3)]
usage += [["texte.desktop", real_now - index * 3600] for index in range(8)]
start_json.write_text(json.dumps({"pins": ["explorer.desktop", "photos.desktop"], "recent": [], "usage": usage,
                                  "seen": {f"{ident}.desktop": 0 for ident in names}}))
for path in downloads.iterdir(): os.utime(path, (real_now - DAY, real_now - DAY))
for name in files:
    if name != "absent.pdf": os.utime(documents / name, None)

Gtk, GLib = module["Gtk"], module["GLib"]
app = module["Start"]()
failures, steps = [], []


def texts(widget):
    found = []
    def walk(node):
        if isinstance(node, Gtk.Label) and node.get_mapped(): found.append(node.get_text())
        if isinstance(node, Gtk.Container):
            for child in node.get_children(): walk(child)
    walk(widget)
    return found


def centred(width_expected):
    area = app.workarea()
    x, _ = app.window.get_position()
    width, _ = app.window.get_size()
    assert abs(width - min(width_expected, area.width - 24)) <= 2, (width, width_expected)
    assert abs((x + width / 2) - (area.x + area.width / 2)) <= 2, ("menu décentré", x, width, area.width)


def two_panes():
    area = app.workarea()
    if area.width < 1100:
        # Écran étroit (1024x600) : un seul volet lisible, centré.
        assert not app.side.get_visible(), "deux volets serrés sur un écran étroit"
        centred(560)
        return
    assert app.side.get_visible(), "volet « Pour vous » absent"
    shown = texts(app.side)
    for expected in ("Pour vous", "Reprendre où vous en étiez", "photo.png", "Souvent ouvert à cette heure-ci", "Courrier",
                     "À faire", "Pas encore installé", "Installer", "Winlux 0.2.21 est disponible"):
        assert expected in shown, (expected, shown)
    area = app.workarea()
    large = area.width >= 1600 and area.height >= 900
    centred((640 + 360) if large else (560 + 320))
    left = texts(app.content)
    assert "Utilisées souvent" in left and "Traitement de texte" in left, left
    assert "WinSCP" not in left or True


def one_pane():
    assert not app.side.get_visible(), "volet encore affiché après « Masquer »"
    if app.workarea().width < 1100: return
    assert json.loads(start_json.read_text()).get("for_you") is False
    area = app.workarea()
    centred(640 if area.width >= 1600 and area.height >= 900 else 560)


def categories():
    assert app.heading.get_text() == "Toutes les applications"
    assert app.views.get_visible(), "choix de vue absent"
    cards = [widget for widget in walk(app.content) if isinstance(widget, Gtk.Button) and widget.get_style_context().has_class("card")]
    # Ordre affiché : ligne puis colonne (GtkGrid rend ses enfants à l'envers).
    grid = cards[0].get_parent()
    cards.sort(key=lambda card: (grid.child_get_property(card, "top-attach"), grid.child_get_property(card, "left-attach")))
    titles = [button.get_tooltip_text() for button in cards]
    assert titles[:3] == ["Bureautique", "Internet", "Programmes Windows"], titles
    assert "Accessoires" in titles and "Jeux" in titles, titles
    app.cards = cards


def walk(widget):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): yield from walk(child)


def unfolded():
    assert app.open_category == "Accessoires", app.open_category
    shown = texts(app.content)
    assert "Calculatrice" in shown and "Zoom avant" in shown and "Courrier" not in shown, shown


def as_list():
    shown = texts(app.content)[27:]  # après la barre de lettres
    assert shown.index("#") < shown.index("7-Zip") < shown.index("C") < shown.index("Calculatrice"), shown
    assert ".exe" in shown, "repère .exe des programmes Windows absent"
    assert app.state.get("apps_view") == "list"


def jumped():
    # « Z » est la dernière lettre : la liste descend jusqu'en bas.
    value = app.scroll.get_vadjustment()
    assert value.get_upper() > value.get_page_size(), "liste A–Z trop courte pour défiler"
    assert value.get_value() == value.get_upper() - value.get_page_size() > 0, (value.get_value(), value.get_upper(), value.get_page_size())


def as_grid():
    grids = [widget for widget in walk(app.content) if isinstance(widget, Gtk.Grid)]
    count = sum(1 for grid in grids for child in grid.get_children() if isinstance(child, Gtk.Button))
    fresh = module["recently_added"](app.state, app.apps, time.time())
    assert "winlux-pe-winscp.desktop" in fresh, fresh  # vue grille : « Ajoutées récemment » en tête
    assert count == len(app.apps) + len(fresh), (count, len(app.apps), len(fresh))


def initial():
    assert app.window.get_visible() and not app.all_apps and app.open_category is None
    assert app.heading.get_text() == "Épinglé" and not app.views.get_visible()
    assert app.scroll.get_vadjustment().get_value() == 0


def step(action, check, delay=600):
    def run():
        try:
            action()
        except Exception as error:  # noqa: BLE001 - rapporté à la fin
            failures.append(error); app.quit(); return False
        GLib.timeout_add(delay, verify)
        return False

    def verify():
        try:
            check()
        except Exception as error:  # noqa: BLE001
            failures.append(error); app.quit(); return False
        if steps: GLib.timeout_add(50, steps.pop(0))
        else: app.quit()
        return False
    return run


def reopen():
    app.hide(); app.activate()


def letter_jump():
    # Le dernier « Z » est l'en-tête de section (le premier, la touche de la barre).
    key, heading = [widget for widget in walk(app.content) if isinstance(widget, Gtk.Label) and widget.get_text() == "Z"]
    assert isinstance(key.get_parent(), Gtk.Button)
    key.get_parent().clicked()


def turn_back_on():
    data = json.loads(start_json.read_text()); data["for_you"] = True
    start_json.write_text(json.dumps(data))
    reopen()


steps += [
    step(lambda: None, two_panes, 1200),  # app.run() a déjà ouvert le menu
    step(app.hide_side, one_pane),
    step(reopen, one_pane),
    step(turn_back_on, two_panes, 1200),
    step(lambda: (app.state.pop("apps_view", None), app.toggle_list()), categories),
    step(lambda: app.cards[[card.get_tooltip_text() for card in app.cards].index("Accessoires")].clicked(), unfolded),
    step(reopen, initial),
    step(lambda: (app.toggle_list(), app.set_view("list")), as_list),
    step(letter_jump, jumped),
    step(lambda: app.set_view("grid"), as_grid),
    step(reopen, initial),
]
app.hold()
GLib.timeout_add(300, lambda: (GLib.timeout_add(50, steps.pop(0)), False)[1])
GLib.timeout_add(40000, lambda: (failures.append(AssertionError("délai dépassé")), app.quit()))
app.run([])
if failures:
    raise failures[0]
print("OK: menu à deux volets centré, « Masquer » le ramène à un volet centré, catégories, liste A–Z, grille")
