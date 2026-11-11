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
# Navigateurs : vrais noms, « Par défaut »/« Ouvrir » seulement une fois installé.
shown, names = module["browser_buttons_shown"], module["BROWSER_NAMES"]
assert list(names) == ["brave", "chrome", "edge", "firefox", "opera", "operagx", "tor", "vivaldi"], names
assert names["firefox"] == "Mozilla Firefox" and names["operagx"] == "Opera GX" and names["tor"] == "Tor Browser", names
# Ordre alphabétique des noms affichés : aucun navigateur mis en avant (#1716).
assert sorted(names.values(), key=str.casefold) == list(names.values()), names
assert set(module["BROWSER_PUBLISHERS"]) == set(names) == set(module["BROWSER_HOSTS"])
title, text = module["browser_confirmation"]("tor", {"version": "16.0.1", "source": "https://dist.torproject.org/torbrowser/16.0.1/x.tar.xz"})
assert "Tor Browser" in title and "16.0.1" in text and "EF6E 286D" in text and "dist.torproject.org" in text, text
state = {"browser_chrome": "installed", "browser_edge": "downloadable", "browser_firefox": "update"}
assert shown(state, "browser-default-chrome") and shown(state, "browser-launch-chrome")
assert not shown(state, "browser-install-chrome")
assert shown(state, "browser-install-edge") and not shown(state, "browser-default-edge") and not shown(state, "browser-launch-edge")
assert shown(state, "browser-install-firefox") and shown(state, "browser-launch-firefox")
assert not shown({}, "browser-default-chrome") and shown({}, "browser-install-chrome")
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
# Icônes du bureau (#30) : Corbeille seule par défaut, choix xfconf respecté.
shown = pages["desktop_icon_shown"]
stored = {}
shown.__globals__["xfconf_get"] = lambda channel, prop: stored.get(prop)
assert shown("trash") and not shown("filesystem") and not shown("home") and not shown("removable")
stored["/desktop-icons/file-icons/show-home"] = "true"
stored["/desktop-icons/file-icons/show-trash"] = "false"
assert shown("home") and not shown("trash")
for script in ("winlux-native-session.sh", "winlux-user-session.sh"):
    text = Path(__file__).with_name(script).read_text()
    assert "for icon in trash:true filesystem:false home:false removable:false" in text, script
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
                # Page 7 refaite (#875-#882) : sa carte d'état, plus de barre ni de rapport brut.
                assert (window.update_panel is not None) == (index == 7), title
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
        # #739 : « Compatibilité » d'un .exe dans l'Explorateur ouvre Programmes
        # Windows sur la carte de ce programme, en tête (et non la page Écran).
        programs_page = module["EXTRA"].PAGE_PROGRAMS
        if programs_page not in window.hidden_pages:
            with tempfile.TemporaryDirectory() as folder:
                exe = Path(folder) / "hello32.exe"
                exe.write_bytes(b"MZ")
                assert module["exe_from_windows"](str(exe)) == exe
                assert module["exe_from_windows"]("") is None
                assert module["exe_from_windows"](str(Path(folder) / "absent.exe")) is None
                window.pending_exe = exe
                window.open_page(programs_page)
                assert window.current == programs_page
                first = window.programs_box.get_children()[0]
                texts = [w.get_text() for w in walk(first) if isinstance(w, Gtk.Label)]
                assert str(exe) in texts, texts
                assert window.pending_exe is None
                print("OK: Compatibilit\u00e9 d'un .exe ouvre sa carte (#739)")
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
        panel_module = None
        try:
            window.open_page(7)
            panel = window.update_panel
            panel_globals = panel.refresh.__globals__
            panel_module = panel_globals
            original_panel_read = panel_globals["read"]
            files = {}
            panel_globals["read"] = lambda path: files.get(str(path), "")
            run = str(panel_globals["RUN"])
            # À jour : un bouton « Rechercher », pas d'installation ni de barre (#877, #879).
            files[run + "/update-state.txt"] = "installed=v0.2.20\nlast_check=1\nprevious=\n"
            panel.refresh({"update_busy": "0"})
            assert panel.kind == "uptodate" and panel.action == "check", panel.kind
            assert not panel.bar.get_visible() and not panel.steps.get_visible(), "barre visible au repos"
            assert not panel.rollback_row.get_visible(), "Revenir proposé sans version précédente"
            # #636 : une version conservée apparaît sans rouvrir la page.
            files[run + "/update-state.txt"] = "installed=v0.2.20\nlast_check=1\nprevious=v0.2.19\n"
            panel.refresh({"update_busy": "0"})
            assert panel.rollback_row.get_visible() and "v0.2.19" in panel.recovery_label.get_text()
            # Disponible : « Télécharger et installer », taille lisible (#875).
            files[run + "/update-state.txt"] = "installed=v0.2.20\navailable=v0.2.21\ndownload_bytes=485490688\nlast_check=1\n"
            panel.refresh({"update_busy": "0"})
            assert panel.action == "install" and "463" in panel.subtitle.get_text() and "octets" not in panel.subtitle.get_text()
            # En cours : barre et étapes, pas de bouton.
            files[run + "/update-progress"] = "phase=downloading\npercent=37\nunit=bytes\ndone=180000000\ntotal=485490688\n"
            panel.refresh({"update_busy": "1"})
            assert panel.kind == "running" and panel.bar.get_visible() and panel.steps.get_visible()
            assert not panel.primary.get_visible()
            # Échec : la carte le dit, « Réessayer » et « Afficher les détails ».
            files.pop(run + "/update-progress")
            panel.failed("update-apply", "downloading")
            assert panel.kind == "error" and panel.action == "retry" and panel.details_button.get_visible()
            window.update_failed = None
            panel_globals["read"] = original_panel_read
            # #634 : l'écran ne s'éteint pas pendant une installation lancée ici.
            calls = []

            class FakeBus:
                def call_sync(self, _name, _path, _interface, method, parameters, *_rest):
                    calls.append((method, parameters.unpack()))
                    return GLib.Variant("(u)", (7,)) if method == "Inhibit" else None
            real_bus = namespace["Gio"].bus_get_sync
            namespace["Gio"].bus_get_sync = lambda *_: FakeBus()
            try:
                namespace["read"] = lambda path: "update_busy=1\n" if path == "/run/winlux/control.txt" else original_read(path)
                window.poll(); window.poll()
                assert [call[0] for call in calls] == ["Inhibit"] and window.screen_cookie == 7, calls
                namespace["read"] = lambda path: "update_busy=0\n" if path == "/run/winlux/control.txt" else original_read(path)
                window.poll()
                assert calls[-1] == ("UnInhibit", (7,)) and window.screen_cookie is None, calls
            finally:
                namespace["Gio"].bus_get_sync = real_bus
                namespace["read"] = original_read
        finally:
            namespace["read"] = original_read
            if panel_module is not None: panel_module["read"] = original_panel_read
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
