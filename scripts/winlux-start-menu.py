#!/usr/bin/python3
"""Windows 11 inspired native Start: real XDG apps, pins and context actions."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk, Gio, GLib

STATE = Path.home() / ".config/winlux/start.json"
DEFAULTS = ["winlux-explorer.desktop", "winlux-browser-edge.desktop", "org.xfce.mousepad.desktop",
            "winlux-settings.desktop", "winlux-terminal.desktop", "winlux-updates.desktop"]

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
        try: self.state = json.loads(STATE.read_text())
        except (OSError, ValueError): self.state = {"pins": DEFAULTS, "recent": []}

    def save(self):
        STATE.parent.mkdir(parents=True, exist_ok=True)
        temporary = STATE.with_suffix(".tmp")
        temporary.write_text(json.dumps(self.state, ensure_ascii=False))
        temporary.replace(STATE)

    def do_activate(self):
        if self.window and self.window.get_visible():
            self.window.hide()
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
        self.search = Gtk.SearchEntry(placeholder_text="Rechercher une application")
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
        self.window.hide()
        return True

    def blur(self, *_):
        def later():
            if not self.window.is_active() and not self.context: self.window.hide()
            return False
        GLib.timeout_add(150, later)
        return False

    def key(self, _, event):
        return self.hide() if event.keyval == Gdk.KEY_Escape else False

    def toggle_list(self, *_):
        self.all_apps = not self.all_apps
        self.render()

    def first_result(self, *_):
        if self.results: self.launch(self.results[0])

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

    def menu(self, widget, event, actions):
        menu = Gtk.Menu()
        self.context = menu
        for title, callback in actions:
            item = Gtk.MenuItem(label=title)
            item.connect("activate", lambda _, action=callback: action())
            menu.append(item)
        menu.connect("deactivate", lambda *_: setattr(self, "context", None))
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
