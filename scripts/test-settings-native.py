#!/usr/bin/python3
"""Exercise settings navigation without launching system actions."""
import runpy
from pathlib import Path

module = runpy.run_path(str(Path(__file__).with_name("winlux-settings.py")), run_name="settings_test")
Gtk, GLib = module["Gtk"], module["GLib"]
# Recherche sans casse ni accents, dans les quatre langues.
fold = module["fold"]
for typed, label in (("parametres", "Paramètres"), ("RESEAU", "Réseau Wi-Fi"), ("offnen", "Öffnen"),
                     ("strasse", "Straße"), ("configuracion", "Configuración"), ("oeuvre", "Œuvre")):
    assert fold(typed) in fold(label), (typed, label)
# Temps de démarrage (page Processeur) : durée lisible et étape la plus longue.
boot = module["boot_summary"]({"boot_known": "1", "boot_total_ms": "8420", "boot_phases": "init:1500,drivers:3150",
                               "boot_slowest": "drivers", "boot_slowest_ms": "3150"})
assert module["seconds"]("8420") in boot and module["seconds"]("3150") in boot, boot
assert module["boot_summary"]({"boot_known": "0"}) != boot
# Mode jeu : une action impossible sur ce matériel n'est pas annoncée.
game = module["game_summary"]({"game_mode": "auto", "game_active": "1", "game_name": "jeu.exe", "cap_game_governor": "0"})
assert "jeu.exe" in game and module["_t"]("processeur en mode performances (sur secteur)") not in game, game
assert module["game_summary"]({}) == ""
# Pages branchées : réglages conservés, valeurs écrites au bon format.
import tempfile
pages = runpy.run_path(str(Path(__file__).with_name("winlux-settings-pages.py")), run_name="settings_pages_test")
with tempfile.TemporaryDirectory() as folder:
    exe = Path(folder) / "jeu.exe"
    Path(str(exe) + ".winlux.ini").write_text("cpu_affinity=0-3\nui_scale=2\n")
    assert pages["profile_write"](exe, ui_scale="1.5", gpu=None)
    profile = pages["profile_read"](exe)
    assert profile == {"cpu_affinity": "0-3", "ui_scale": "1.5"}, profile
    ini = Path(folder) / "accessibility.ini"
    ini.write_bytes(b"# Param\xe8tres\nmagnifier=1\ncontrast=0\n")
    pages["a11y_write"].__globals__["A11Y_FILE"] = ini
    assert pages["a11y_write"](contrast=2, sticky=True)
    assert ini.read_bytes() == b"# Param\xe8tres\nmagnifier=1\ncontrast=2\nsticky=1\n", ini.read_bytes()
    rc = Path(folder) / "rc.xml"
    rc.write_text('<?xml version="1.0"?><openbox_config xmlns="http://openbox.org/3.4/rc"><resize><drawContents>yes</drawContents></resize></openbox_config>')
    pages["openbox_set"].__globals__["OPENBOX_RC"] = rc
    pages["openbox_set"].__globals__["have"] = lambda command: False
    assert pages["openbox_set"]("resize", "drawContents", "no") and pages["openbox_get"]("resize", "drawContents") == "no"
    assert pages["openbox_set"]("theme", "animateIconify", "no") and pages["openbox_get"]("theme", "animateIconify") == "no"
window = module["Settings"]()
failures = []

def in_card(widget):
    while widget is not None:
        if "card" in widget.get_style_context().list_classes(): return True
        widget = widget.get_parent()
    return False

def walk(widget):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): yield from walk(child)

def check():
    try:
        rows = window.rows
        visited = set()
        for category, (name, pages) in enumerate(module["CATEGORIES"]):
            shown = [page for page in pages if page not in window.hidden_pages]
            # Une catégorie sans page possible sur cette machine n'apparaît pas.
            assert rows[("category", category)].get_child_visible() == bool(shown) or not shown, name
            for index in pages:
                title = module["PAGES"][index]
                if index in window.hidden_pages:
                    assert not rows[("page", index)].get_child_visible(), title
                    continue
                window.nav.select_row(rows[("category", category)])
                window.open_page(index)
                visited.add(index)
                assert window.current == index, title
                # Sous-pages dépliées sous la catégorie ouverte, et seulement elle.
                for (kind, key), row in rows.items():
                    if kind == "page" and key not in window.hidden_pages:
                        expected = module["category_of"](key) == category and len(shown) > 1
                        assert row.get_child_visible() == expected, (title, module["PAGES"][key])
                labels = [w for w in window.body.get_children() if isinstance(w, Gtk.Label)]
                assert title in [w.get_text() for w in labels[:2]], title
                assert window.progress.get_visible() == (index == 7)
                # Plus aucune fenêtre séparée : ni « Réglages avancés » ni outils à part.
                buttons = [w.get_label() for w in walk(window.body) if isinstance(w, Gtk.Button) and w.get_label()]
                assert module["_t"]("Réglages avancés") not in buttons, title
                if index in window.EXTRA_PAGES and not (index == module["SOUND_PAGE"] and window.sound_fallback):
                    # Une page branchée montre des réglages, pas un cadre d'état vide.
                    controls = [w for w in walk(window.body) if isinstance(w, (Gtk.Switch, Gtk.ComboBox, Gtk.Button, Gtk.Scale))
                                or (isinstance(w, Gtk.Label) and w.get_text().strip() and in_card(w))]
                    assert controls, f"Page vide : {title}"
                    empty = [w for w in walk(window.body) if isinstance(w, Gtk.Label) and "card" in w.get_style_context().list_classes()
                             and not w.get_text().strip()]
                    assert not empty, f"Cadre vide : {title}"
        # Toute page existante est rangée dans une catégorie ou fondue dans une autre.
        placed = {page for _, pages in module["CATEGORIES"] for page in pages} | set(module["MERGED"])
        assert placed == set(range(len(module["PAGES"]))), sorted(set(range(len(module["PAGES"]))) - placed)
        # Les anciens numéros ouvrent la page fusionnée.
        window.open_page(5)
        assert window.current == 4
        window.open_page(3)
        assert window.current == module["SOUND_PAGE"]
        assert window.process is None, "Navigation launched a system action"
        namespace = window.poll.__globals__
        original_read = namespace["read"]
        try:
            namespace["read"] = lambda path: "Winlux est à jour.\n" if path == "/run/winlux/updates.txt" else original_read(path)
            window.open_page(7)
            window.poll()
            assert not window.install_button.get_parent().get_visible(), "Install button shown for current version"
            if not Path(module["PREVIOUS_PACKAGE"]).is_file():
                assert not window.rollback_button.get_parent().get_visible(), "Restore shown without a previous version"
        finally:
            namespace["read"] = original_read
        window.show_update_error("update-apply", 1)
        dialogs = [widget for widget in Gtk.Window.list_toplevels() if isinstance(widget, Gtk.MessageDialog)]
        assert len(dialogs) == 1, "Update error dialog was not displayed"
        assert "Étape :" in dialogs[0].get_property("secondary-text")
        dialogs[0].destroy()
    except Exception as error:
        failures.append(error)
    finally:
        window.destroy()
    return False

GLib.idle_add(check)
Gtk.main()
if failures:
    raise failures[0]
print(f"OK: {len(module['CATEGORIES'])} categories, {len(module['PAGES'])} native settings pages, progress visibility, no system actions")
