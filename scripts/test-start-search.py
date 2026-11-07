#!/usr/bin/python3
"""Recherche du menu Démarrer (#461, #606, #607) : toutes les pages des Paramètres, mots-clés
des quatre langues, mots dans n'importe quel ordre, résultats groupés avec
icônes, clic droit « Ouvrir l'emplacement du fichier », bouton Rechercher.

    xvfb-run -a dbus-run-session -- python3 -B scripts/test-start-search.py
"""
import json
import os
from pathlib import Path
import re
import runpy
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
tmp = Path(tempfile.mkdtemp(prefix="winlux-start-search-"))
home = tmp / "home"
os.environ.update(HOME=str(home), WINLUX_LANG="fr", WINLUX_OSK_ROOT=str(tmp), XDG_DATA_HOME=str(home / ".local/share"))
os.environ.pop("XDG_CONFIG_HOME", None)
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "proc/bus/input/devices").write_text('N: Name="AT Translated Set 2 keyboard"\nH: Handlers=kbd event0\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
(home / "Documents/Impôts").mkdir(parents=True)
(home / "Documents/Impôts/Déclaration 2026.pdf").write_text("x")
(home / "Documents/Facture électricité.odt").write_text("x")
# #607 : « son » ne trouve ni « json » ni « Personnalisation » ; nom qui
# commence par la recherche d'abord, puis un mot, les plus récents d'abord.
for name, age in [("Bureau/icones/application-json.png", 0), ("Bureau/Personnalisation.png", 0),
                  ("Musique/Mon son ancien.wav", 5000), ("Musique/Mon son récent.wav", 10), ("Documents/son-test.txt", 9000)]:
    (home / name).parent.mkdir(parents=True, exist_ok=True)
    (home / name).write_text("x")
    os.utime(home / name, (1_700_000_000 - age, 1_700_000_000 - age))
applications = home / ".local/share/applications"
applications.mkdir(parents=True)
(applications / "winlux-settings.desktop").write_text("[Desktop Entry]\nType=Application\nName=Paramètres\nExec=true\nCategories=Settings;\n")
(applications / "gimp.desktop").write_text("[Desktop Entry]\nType=Application\nName=GIMP\nGenericName=Éditeur d'images\n"
                                           "Keywords=photo;retouche;\nExec=true\nCategories=Graphics;\n")
(home / ".config/winlux").mkdir(parents=True)
(home / ".config/winlux/start.json").write_text(json.dumps({"pins": [], "recent": []}))

module = runpy.run_path(str(HERE / "winlux-start-menu.py"), run_name="start_search_test")
fold, settings_for, found_in = module["fold"], module["settings_for"], module["found_in"]


def pages(query):
    return [page for _, page in settings_for(fold(query))]


# ── Pages des Paramètres : toutes indexées, trouvées dans les quatre langues ──
index = module["SETTINGS_INDEX"]
settings_source = (HERE / "winlux-settings.py").read_text(encoding="utf-8")
categories = re.search(r"^CATEGORIES = \[(.*?)\]\n# Anciennes", settings_source, re.S | re.M).group(1)
shown_pages = {int(number) for number in re.findall(r"\d+", re.sub(r'_t\("[^"]*"\)', "", categories))}
assert shown_pages <= {page for page, _, _ in index}, f"pages des Paramètres absentes de la recherche : {shown_pages - {page for page, _, _ in index}}"
for page, label, keywords in index:
    assert label in module["_TR"] and len(module["_TR"][label]) == 3 and all(module["_TR"][label]), label
    assert page in pages(label), (label, pages(label))
expected = {"wifi": 35, "wi fi": 35, "WLAN": 35, "comptes": 33, "mot de passe": 33, "imprimante": 26, "printer": 26,
            "Drucker": 26, "impresora": 26, "vpn": 2, "telephone": 37, "téléphone": 37, "langue": 14, "accessibilite": 12,
            "loupe": 12, "confidentialite": 18, "fond ecran": 21, "écran fond": 21, "theme sombre": 21, "stockage": None,
            "son": 16, "volume": 16, "Lautstärke": 16, "bluetooth": 22, "batterie": 23, "veille": 23, "pilotes": 34,
            "gestionnaire de périphériques": 34, "certificat": 36, "presse-papiers": 32, "heure": 14, "resolution": 8}
for query, page in expected.items():
    found = pages(query)
    if page is None: continue
    assert found and found[0] == page, (query, found)
assert pages("son")[:1] == [16] and 21 not in pages("son"), "« son » ne doit pas trouver « personnalisation »"
assert found_in("wifi", "Wi-Fi") and found_in("ecran fond", "Fond d’écran") and not found_in("son", "Personnalisation")
assert not settings_for(fold("xyzxyz"))

# ── Interface : groupes, icônes, clic droit sur un fichier ──────────────────
Gtk, GLib = module["Gtk"], module["GLib"]
app = module["Start"]()
problems = []


def texts(widget, found):
    if isinstance(widget, Gtk.Label): found.append(widget.get_text())
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): texts(child, found)
    return found


def check_search():
    try:
        app.search.set_text("wifi")
        app.render()
        labels = texts(app.content, [])
        assert "Meilleur résultat" in labels and "Wi-Fi" in labels, labels
        app.search.set_text("declaration")
        app.render()
    except AssertionError as error:
        problems.append(error)
        app.quit()
    GLib.timeout_add(1200, check_files)
    return False


def check_files():
    try:
        labels = texts(app.content, [])
        assert "Documents" in labels and "Déclaration 2026.pdf" in labels and "~/Documents/Impôts" in labels, labels
        buttons = [child for child in app.content.get_children() if isinstance(child, Gtk.Button)]
        assert any((button.get_tooltip_text() or "").startswith("Fichier · Documents/Impôts") for button in buttons)
        app.search.set_text("retouche")
        app.render()
        labels = texts(app.content, [])
        assert "GIMP" in labels, ("mot-clé du lanceur", labels)
        app.search.set_text("éditeur images")
        app.render()
        assert "GIMP" in texts(app.content, []), "nom générique, mots dans le désordre"
        opened = []
        app.hide = lambda: None
        module["subprocess"].Popen = lambda args, **_: opened.append(args)
        app.open_location(home / "Documents/Impôts/Déclaration 2026.pdf")
        assert opened == [["winlux-shell-app", "explorer", str(home / "Documents/Impôts")]], opened
        app.search.set_text("son")
        app.render()
    except AssertionError as error:
        problems.append(error)
        app.quit()
    GLib.timeout_add(1200, check_ranking)
    return False


def check_ranking():
    try:
        # #607 : début de mot seulement, puis classement.
        names = [path.name for path in app.file_results]
        assert names == ["son-test.txt", "Mon son récent.wav", "Mon son ancien.wav"], names
        labels = texts(app.content, [])
        assert "Son" in labels and not any("json" in label or "Personnalisation" in label for label in labels), labels
        # #606 : Paramètres et fichiers comme les applications, icône puis texte aligné à gauche.
        rows = [child for child in app.content.get_children() if isinstance(child, Gtk.Button)
                and (child.get_tooltip_text() or "").startswith(("Paramètres · ", "Fichier · "))]
        assert len(rows) >= 4, [row.get_tooltip_text() for row in rows]
        for row in rows:
            box = row.get_child()
            first = box.get_children()[0]
            assert isinstance(box, Gtk.Box) and isinstance(first, Gtk.Image), row.get_tooltip_text()
            title = texts(box, [])[0]
            label = next(widget for widget in box.get_children()[1].get_children() if isinstance(widget, Gtk.Label))
            assert label.get_xalign() == 0 and "/" not in title and "·" not in title, (row.get_tooltip_text(), title)
    except AssertionError as error:
        problems.append(error)
    app.quit()
    return False


def started():
    if not app.window: return True
    GLib.timeout_add(300, check_search)
    return False


GLib.timeout_add(300, started)
app.run([])
if problems: raise problems[0]

# ── Bouton Rechercher de la barre ───────────────────────────────────────────
desktop = (ROOT / "integration/native/applications/winlux-search.desktop").read_text(encoding="utf-8")
assert "Exec=/usr/bin/winlux-start-menu\n" in desktop and all(f"Name[{lang}]=" in desktop for lang in ("en", "es", "de"))
assert (ROOT / "integration/native/search.svg").is_file()
panel = ET.parse(ROOT / "integration/native/config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").getroot()
order = [value.get("value") for value in panel.find(".//property[@name='plugin-ids']")]
search = order[order.index("1") + 1]
assert panel.find(f".//property[@name='plugin-{search}']/property[@name='items']/value").get("value").endswith("winlux-search.desktop")
print("OK: recherche du menu Démarrer (pages des Paramètres en 4 langues, groupes, fichiers, bouton Rechercher)")
