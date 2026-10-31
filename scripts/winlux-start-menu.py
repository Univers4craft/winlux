#!/usr/bin/python3
"""Windows 11 inspired native Start: real XDG apps, pins and context actions."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, Gio, GLib

STATE = Path.home() / ".config/winlux/start.json"
DEFAULTS = ["winlux-explorer.desktop", "winlux-browser-edge.desktop", "org.xfce.mousepad.desktop",
            "winlux-settings.desktop", "winlux-terminal.desktop", "winlux-updates.desktop"]
SETTINGS = [("Système et alimentation", 0), ("Applications et démarrage", 1), ("Réseau Wi-Fi", 2),
            ("Son", 3), ("Alimentation batterie veille", 21), ("Clavier", 4), ("Souris", 5), ("Personnalisation fond écran thème", 6),
            ("Mises à jour", 7), ("Écran résolution", 8), ("À propos version", 9), ("Navigateurs", 11)]

def category(app):
    """Use installed desktop metadata, including generated Windows launchers."""
    categories = set((app.get_categories() or "").split(";")) if hasattr(app, "get_categories") else set()
    for key, label in [("Settings", "Paramètres"), ("Network", "Internet"),
                       ("Office", "Bureautique"), ("Graphics", "Graphisme"),
                       ("AudioVideo", "Audio et vidéo"), ("Game", "Jeux"),
                       ("Development", "Développement"), ("System", "Système"),
                       ("Utility", "Accessoires")]:
        if key in categories: return label
    return "Autres applications"

class Start(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="org.winlux.Start", flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.window = None
        self.context = None
        self.all_apps = False
        self.file_generation = 0
        self.file_query = ""
        self.file_results = []
        try: self.state = json.loads(STATE.read_text())
        except (OSError, ValueError): self.state = {"pins": DEFAULTS, "recent": []}

    def save(self):
        STATE.parent.mkdir(parents=True, exist_ok=True)
        temporary = STATE.with_suffix(".tmp")
        temporary.write_text(json.dumps(self.state, ensure_ascii=False))
        temporary.replace(STATE)

    def do_activate(self):
        if self.window and self.window.get_visible():
            self.hide()
            return
        if not self.window: self.build()
        self.apps = {app.get_id(): app for app in Gio.AppInfo.get_all() if app.should_show() and app.get_id()}
        self.all_apps = False
        self.search.set_text("")
        self.render()
        monitor = self.window.get_display().get_primary_monitor()
        if monitor is None: monitor = self.window.get_display().get_monitor(0)
        area = monitor.get_workarea()
        width, height = min(660, area.width - 24), min(650, area.height - 24)
        self.window.set_default_size(width, height)
        self.window.set_size_request(width, height)
        self.window.move(area.x + (area.width - width) // 2, area.y + area.height - height - 10)
        self.window.show_all()
        self.window.present()
        self.search.grab_focus()
        GLib.idle_add(self.grab_pointer)

    def build(self):
        self.window = Gtk.ApplicationWindow(application=self, title="Démarrer Winlux")
        self.window.set_decorated(False)
        self.window.set_resizable(False)
        self.window.set_skip_taskbar_hint(True)
        self.window.set_skip_pager_hint(True)
        self.window.set_type_hint(Gdk.WindowTypeHint.UTILITY)
        self.window.set_keep_above(True)
        self.window.connect("delete-event", lambda *_: self.hide())
        self.window.connect("focus-out-event", self.blur)
        self.window.connect("key-press-event", self.key)
        self.window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        self.window.connect("button-press-event", self.outside_click)
        css = Gtk.CssProvider()
        css.load_from_data(b"""
          #winlux-start { background: #242238; color: #f7f5ff; border: 1px solid #6b5f92; border-radius: 8px; }
          #winlux-start entry { border-radius: 5px; background: #171624; padding: 9px; border-bottom: 2px solid #b19af9; }
          #winlux-start button { background: transparent; border: none; box-shadow: none; border-radius: 5px; padding: 7px; }
          #winlux-start button:hover { background: #433b60; }
          #winlux-start .heading { font-weight: bold; font-size: 12pt; }
          #winlux-start .footer { background: #1d1a2d; padding: 12px 25px; }
          #winlux-start .app-name { font-size: 9pt; }
        """)
        Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        self.window.set_name("winlux-start")
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.window.add(outer)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18, margin=28)
        outer.pack_start(body, True, True, 0)
        self.search = Gtk.SearchEntry(placeholder_text="Applications, fichiers et paramètres")
        self.search.connect("search-changed", lambda *_: self.render())
        self.search.connect("activate", self.first_result)
        body.pack_start(self.search, False, False, 0)
        header = Gtk.Box()
        self.heading = Gtk.Label(label="Épinglé", xalign=0)
        self.heading.get_style_context().add_class("heading")
        header.pack_start(self.heading, True, True, 0)
        self.toggle = Gtk.Button(label="Toutes les applications  ›")
        self.toggle.connect("clicked", self.toggle_list)
        header.pack_end(self.toggle, False, False, 0)
        body.pack_start(header, False, False, 0)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        scroll.add(self.content)
        body.pack_start(scroll, True, True, 0)
        footer = Gtk.Box(spacing=12)
        footer.get_style_context().add_class("footer")
        footer.pack_start(Gtk.Image.new_from_icon_name("avatar-default-symbolic", Gtk.IconSize.LARGE_TOOLBAR), False, False, 0)
        footer.pack_start(Gtk.Label(label="Winlux", xalign=0), True, True, 0)
        power = Gtk.Button.new_from_icon_name("system-shutdown-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
        power.set_tooltip_text("Options d’alimentation")
        power.connect("clicked", self.power)
        footer.pack_end(power, False, False, 0)
        outer.pack_end(footer, False, False, 0)

    def hide(self):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        self.window.hide()
        return True

    def grab_pointer(self):
        if self.window.get_visible() and not self.context:
            self.pointer_status=self.window.get_display().get_default_seat().grab(self.window.get_window(),Gdk.SeatCapabilities.POINTER,
                False,None,None,None,None)
            if self.pointer_status==Gdk.GrabStatus.SUCCESS:self.window.grab_add()
        return False

    def outside_click(self, _, event):
        x,y=self.window.get_position();width,height=self.window.get_size()
        if not (x<=event.x_root<x+width and y<=event.y_root<y+height):return self.hide()
        return False

    def blur(self, *_):
        def later():
            if not self.window.is_active() and not self.context: self.hide()
            return False
        GLib.timeout_add(150, later)
        return False

    def key(self, _, event):
        return self.hide() if event.keyval == Gdk.KEY_Escape else False

    def toggle_list(self, *_):
        self.all_apps = not self.all_apps
        self.render()

    def first_result(self, *_):
        query=self.search.get_text().strip().casefold()
        setting=next((page for label,page in SETTINGS if query and query in label.casefold()),None)
        if setting is not None:self.open_setting(setting)
        elif self.results:self.launch(self.results[0])
        elif self.file_results:self.open_file(self.file_results[0])

    def launch(self, app):
        try:
            app.launch([], self.window.get_display().get_app_launch_context())
            self.state["recent"] = ([app.get_id()] + [value for value in self.state.get("recent", []) if value != app.get_id()])[:6]
            self.save()
            self.hide()
        except GLib.Error as error:
            dialog = Gtk.MessageDialog(transient_for=self.window, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text="Impossible d’ouvrir l’application")
            dialog.format_secondary_text(str(error))
            dialog.run()
            dialog.destroy()

    def app_button(self, app, grid=False):
        button = Gtk.Button()
        if grid: button.set_size_request(88, 82)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL if grid else Gtk.Orientation.HORIZONTAL, spacing=6)
        icon = app.get_icon() or Gio.ThemedIcon.new("application-x-executable")
        image = Gtk.Image.new_from_gicon(icon, Gtk.IconSize.DIALOG if grid else Gtk.IconSize.LARGE_TOOLBAR)
        image.set_pixel_size(32 if grid else 24)
        box.pack_start(image, False, False, 0)
        label = Gtk.Label(label=app.get_display_name(), xalign=0.5 if grid else 0)
        label.set_ellipsize(3)
        label.set_max_width_chars(12 if grid else 50)
        if grid:
            label.set_line_wrap(True)
            label.set_lines(2)
        label.get_style_context().add_class("app-name")
        box.pack_start(label, True, True, 0)
        button.add(box)
        button.set_tooltip_text(app.get_display_name())
        button.connect("clicked", lambda *_: self.launch(app))
        button.connect("button-press-event", lambda widget, event: self.app_context(widget, event, app))
        return button

    def render(self):
        if not self.window or not hasattr(self, "apps"): return
        for widget in self.content.get_children(): widget.destroy()
        query = self.search.get_text().strip().casefold()
        listing = self.all_apps or bool(query)
        self.heading.set_text("Résultats" if query else "Toutes les applications" if listing else "Épinglé")
        self.toggle.set_label("‹  Retour" if listing else "Toutes les applications  ›")
        self.results = sorted((app for app in self.apps.values() if query in app.get_display_name().casefold()), key=lambda app: app.get_display_name().casefold())
        if listing:
            if query:
                for label, page in SETTINGS:
                    if query in label.casefold():
                        button = Gtk.Button(label="Paramètres · " + label)
                        button.connect("clicked", lambda _, target=page: self.open_setting(target))
                        self.content.pack_start(button, False, False, 0)
                if query != self.file_query:
                    self.file_query = query
                    self.file_generation += 1
                    self.file_results = []
                    if len(query) >= 2:
                        threading.Thread(target=self.find_files, args=(query, self.file_generation), daemon=True).start()
                for path in self.file_results:
                    button = Gtk.Button(label="Fichier · " + str(path.relative_to(Path.home())))
                    button.connect("clicked", lambda _, target=path: self.open_file(target))
                    self.content.pack_start(button, False, False, 0)
                if len(self.file_results) == 30:
                    self.content.pack_start(Gtk.Label(label="30 premiers fichiers affichés. Précisez la recherche.", xalign=0), False, False, 0)
            previous = None
            if not query: self.results.sort(key=lambda app: (category(app), app.get_display_name().casefold()))
            for app in self.results:
                group = category(app)
                if not query and group != previous:
                    heading = Gtk.Label(label=group, xalign=0, margin_top=10)
                    heading.get_style_context().add_class("heading")
                    self.content.pack_start(heading, False, False, 0)
                    previous = group
                self.content.pack_start(self.app_button(app), False, False, 0)
            if not self.results: self.content.pack_start(Gtk.Label(label="Aucune application trouvée"), False, False, 0)
        else:
            grid = Gtk.Grid(column_homogeneous=True, column_spacing=4, row_spacing=12)
            self.content.pack_start(grid, False, False, 0)
            pinned = [self.apps[value] for value in self.state.get("pins", DEFAULTS) if value in self.apps]
            for index, app in enumerate(pinned): grid.attach(self.app_button(app, True), index % 6, index // 6, 1, 1)
            heading = Gtk.Label(label="Recommandations", xalign=0, margin_top=15)
            heading.get_style_context().add_class("heading")
            self.content.pack_start(heading, False, False, 0)
            recent = [self.apps[value] for value in self.state.get("recent", []) if value in self.apps]
            for app in recent[:4]: self.content.pack_start(self.app_button(app), False, False, 0)
            if not recent:
                self.content.pack_start(Gtk.Label(label="Vos applications récemment ouvertes apparaîtront ici.", xalign=0, wrap=True), False, False, 0)
        self.content.show_all()

    def open_setting(self, page):
        subprocess.Popen(["winlux-shell-app", "settings", str(page)])
        self.hide()

    def open_file(self, path):
        self.hide()
        try:Gio.AppInfo.launch_default_for_uri(path.as_uri(),self.window.get_display().get_app_launch_context())
        except GLib.Error as error:
            dialog=Gtk.MessageDialog(transient_for=self.window,modal=True,message_type=Gtk.MessageType.ERROR,
                buttons=Gtk.ButtonsType.CLOSE,text="Impossible d’ouvrir ce fichier",secondary_text=str(error))
            dialog.run();dialog.destroy()

    def find_files(self, query, generation):
        # Bounded name search in visible personal directories. Never follows
        # directory symlinks or scans the system disks from the UI thread.
        matches, visited = [], 0
        for directory, folders, files in os.walk(Path.home(), followlinks=False):
            if generation != self.file_generation: return
            folders[:] = [name for name in folders if not name.startswith(".") and not Path(directory, name).is_symlink()]
            for name in folders + files:
                visited += 1
                if not name.startswith(".") and query in name.casefold(): matches.append(Path(directory, name))
                if len(matches) >= 30 or visited >= 30000: break
            if len(matches) >= 30 or visited >= 30000: break
        def deliver():
            if generation == self.file_generation:
                self.file_results = matches
                self.render()
            return False
        GLib.idle_add(deliver)

    def menu(self, widget, event, actions):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        menu = Gtk.Menu()
        self.context = menu
        for title, callback in actions:
            item = Gtk.MenuItem(label=title)
            item.connect("activate", lambda _, action=callback: action())
            menu.append(item)
        def deactivate(*_):
            self.context=None
            GLib.idle_add(self.grab_pointer)
        menu.connect("deactivate", deactivate)
        menu.show_all()
        if event: menu.popup_at_pointer(event)
        else: menu.popup_at_widget(widget, Gdk.Gravity.NORTH_EAST, Gdk.Gravity.SOUTH_EAST, None)

    def app_context(self, widget, event, app):
        if event.button != 3: return False
        identifier = app.get_id()
        def pin():
            pins = self.state.setdefault("pins", list(DEFAULTS))
            if identifier in pins: pins.remove(identifier)
            else: pins.append(identifier)
            self.save()
            self.render()
        def shortcut():
            source = app.get_filename()
            desktop = Path(GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_DESKTOP) or str(Path.home() / "Bureau"))
            desktop.mkdir(parents=True, exist_ok=True)
            target = desktop / Path(source).name
            if not target.exists():
                shutil.copyfile(source, target)
                target.chmod(0o755)
                subprocess.run(["gio", "set", "-t", "string", str(target), "metadata::trusted", "true"],
                               check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.menu(widget, event, [("Ouvrir", lambda: self.launch(app)),
                                 ("Détacher du menu Démarrer" if identifier in self.state.get("pins", DEFAULTS) else "Épingler au menu Démarrer", pin),
                                 ("Créer un raccourci sur le bureau", shortcut)])
        return True

    def power(self, widget):
        def action(name):
            subprocess.Popen(["/usr/bin/winlux-desktop-action", name])
            self.hide()
        self.menu(widget, None, [("Redémarrer", lambda: action("reboot")), ("Arrêter", lambda: action("shutdown"))])

if __name__ == "__main__":
    raise SystemExit(Start().run())
