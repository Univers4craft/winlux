#!/usr/bin/python3
"""Vue des tâches (Windows + Tab) et bureaux virtuels, comme Windows 11.

En haut, les fenêtres ouvertes du bureau virtuel courant, avec leur aperçu ;
un clic l'affiche, la croix la ferme, un clic droit la déplace vers un autre
bureau. En bas, les bureaux : un clic y passe, « Nouveau bureau » en ajoute
un, la croix en supprime un (ses fenêtres rejoignent le bureau voisin), un
clic droit le renomme. Échap ou un clic dans le vide ferme la vue.

    winlux-taskview                  ouvre la vue des tâches (Windows + Tab)
    winlux-taskview new              nouveau bureau et passage dessus (Windows + Ctrl + D)
    winlux-taskview close            ferme le bureau courant (Windows + Ctrl + F4)
    winlux-taskview list             bureaux et fenêtres (tests)

Les bureaux sont ceux d'openbox (_NET_NUMBER_OF_DESKTOPS, _NET_DESKTOP_NAMES).
"""
import ctypes
import ctypes.util
import os
import re
import subprocess
import sys


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="): code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    'Vue des tâches': ('Task view', 'Vista de tareas', 'Aktive Anwendungen'),
    'Bureau {n}': ('Desktop {n}', 'Escritorio {n}', 'Desktop {n}'),
    'Nouveau bureau': ('New desktop', 'Nuevo escritorio', 'Neuer Desktop'),
    'Aucune fenêtre ouverte sur ce bureau.': ('No open windows on this desktop.', 'No hay ventanas abiertas en este escritorio.', 'Keine geöffneten Fenster auf diesem Desktop.'),
    'Déplacer vers': ('Move to', 'Mover a', 'Verschieben nach'),
    'Fermer': ('Close', 'Cerrar', 'Schließen'),
    'Renommer': ('Rename', 'Cambiar nombre', 'Umbenennen'),
    'Supprimer le bureau': ('Remove desktop', 'Quitar escritorio', 'Desktop entfernen'),
    'Nom du bureau': ('Desktop name', 'Nombre del escritorio', 'Desktopname'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# Noms par défaut (rc.xml, openbox « desktop N ») : affichés « Bureau N » dans la langue du bureau.
GENERIC_NAME = re.compile(r"\s*((desktop|bureau)\s*\d*|bureau winlux)?\s*", re.I)


def _run(args, timeout=3):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return ""
    return result.stdout if result.returncode == 0 else ""


def desktops():
    """[(numéro, nom affiché, courant ?)]."""
    names = desktop_names()
    found = []
    for line in _run(["wmctrl", "-d"]).splitlines():
        match = re.match(r"^(\d+)\s+([*-])", line)
        if not match: continue
        number = int(match.group(1))
        name = names[number] if number < len(names) else ""
        found.append((number, display_name(number, name), match.group(2) == "*"))
    return found


def display_name(number, name):
    return _t("Bureau {n}").format(n=number + 1) if GENERIC_NAME.fullmatch(name) else name


def desktop_names():
    out = _run(["xprop", "-root", "_NET_DESKTOP_NAMES"])
    return re.findall(r'"((?:[^"\\]|\\.)*)"', out.split("=", 1)[-1]) if "=" in out else []


def windows():
    """[(id hexadécimal, bureau, titre)] des fenêtres normales."""
    found = []
    for line in _run(["wmctrl", "-l"]).splitlines():
        parts = line.split(None, 3)
        if len(parts) < 3: continue
        try: desktop = int(parts[1])
        except ValueError: continue
        if desktop < 0: continue
        kind = _run(["xprop", "-id", parts[0], "_NET_WM_WINDOW_TYPE"])
        if "_NET_WM_WINDOW_TYPE_NORMAL" not in kind and "_NET_WM_WINDOW_TYPE_DIALOG" not in kind \
                and "_NET_WM_WINDOW_TYPE(ATOM)" in kind:
            continue
        title = parts[3] if len(parts) > 3 else ""
        found.append((parts[0], desktop, title))
    return found


def removal_moves(count, removed, placed):
    """Après suppression du bureau `removed` : [(fenêtre, nouveau bureau)].

    Ses fenêtres vont au bureau voisin (le précédent, ou le suivant pour le
    premier) ; celles des bureaux suivants reculent d'un cran."""
    target = removed - 1 if removed > 0 else 0
    moves = []
    for window, desktop in placed:
        if desktop == removed: moves.append((window, target))
        elif desktop > removed: moves.append((window, desktop - 1))
    return moves


def set_desktop_names(names):
    """_NET_DESKTOP_NAMES (UTF-8, noms séparés par un octet nul), lu par openbox."""
    x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XInternAtom.restype = ctypes.c_ulong
    x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x11.XChangeProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong,
                                    ctypes.c_int, ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
    x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    display = x11.XOpenDisplay(None)
    if not display: return False
    data = b"".join(name.encode("utf-8") + b"\0" for name in names)
    x11.XChangeProperty(display, x11.XDefaultRootWindow(display), x11.XInternAtom(display, b"_NET_DESKTOP_NAMES", 0),
                        x11.XInternAtom(display, b"UTF8_STRING", 0), 8, 0, data, len(data))
    x11.XSync(display, 0)
    return True


def new_desktop():
    count = len(desktops()) or 1
    _run(["wmctrl", "-n", str(count + 1)])
    _run(["wmctrl", "-s", str(count)])
    return 0


def remove_desktop(number):
    current = desktops()
    if len(current) <= 1: return 0
    names = desktop_names() + [""] * len(current)
    for window, desktop in removal_moves(len(current), number, [(w, d) for w, d, _ in windows()]):
        _run(["wmctrl", "-i", "-r", window, "-t", str(desktop)])
    kept = [names[index] for index in range(len(current)) if index != number]
    was_current = next((d for d, _, on in current if on), 0)
    _run(["wmctrl", "-n", str(len(current) - 1)])
    set_desktop_names(kept)
    if was_current >= number and was_current > 0:
        _run(["wmctrl", "-s", str(was_current - 1)])
    return 0


def rename_desktop(number, name):
    count = len(desktops())
    names = (desktop_names() + [""] * count)[:count]
    names[number] = name.strip()
    return set_desktop_names(names)


# ── Vue des tâches (GTK) ────────────────────────────────────────────────────

def run_view():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    gi.require_version("GdkX11", "3.0")
    from gi.repository import Gdk, GdkX11, GLib, Gtk

    def thumbnail(window, width=280, height=170):
        """Aperçu de la fenêtre (contenu réel quand le compositeur le garde), sinon rien."""
        try:
            foreign = GdkX11.X11Window.foreign_new_for_display(Gdk.Display.get_default(), int(window, 16))
            if foreign is None: return None
            w, h = foreign.get_width(), foreign.get_height()
            if w < 2 or h < 2: return None
            pixbuf = Gdk.pixbuf_get_from_window(foreign, 0, 0, w, h)
            if pixbuf is None: return None
            scale = min(width / w, height / h)
            return pixbuf.scale_simple(max(1, int(w * scale)), max(1, int(h * scale)), 2)
        except Exception:
            return None

    class View(Gtk.Window):
        def __init__(self):
            super().__init__(title=_t("Vue des tâches"))
            self.set_role("winlux-taskview")
            self.set_decorated(False)
            self.set_keep_above(True)
            self.set_skip_taskbar_hint(True)
            screen = self.get_screen()
            visual = screen.get_rgba_visual()
            if visual is not None and screen.is_composited(): self.set_visual(visual)
            # Pas d'app_paintable : GTK ne peignait alors le voile que sous la
            # zone centrale (marge de 40 px), les fenêtres restaient nettes et
            # cliquables sur tout le pourtour de l'écran.
            css = Gtk.CssProvider()
            css.load_from_data(b"""
              window { background: rgba(16, 18, 26, 0.86); color: #ecedf5; }
              .card { background: #2a3040; border-radius: 8px; padding: 8px; }
              .card:hover { background: #343b4e; }
              .title { font-size: 13px; }
              .hint { color: #adb6c9; font-size: 15px; }
              .desk { background: #2a3040; border-radius: 8px; padding: 8px 16px; color: #ecedf5; border: 2px solid transparent; }
              .desk.on { border-color: #8178b7; }
              .desk:hover { background: #343b4e; }
              button.x { background: transparent; border: none; box-shadow: none; padding: 0 6px; color: #ecedf5; }
              button.x:hover { background: #c42b1c; }
              button.new { background: #30384b; border: 1px dashed #6a7590; border-radius: 8px; padding: 8px 16px; color: #ecedf5; }
            """)
            Gtk.StyleContext.add_provider_for_screen(screen, css, 600)
            monitor = Gdk.Display.get_default().get_monitor_at_window(Gdk.get_default_root_window()) \
                if hasattr(Gdk.Display.get_default(), "get_monitor_at_window") else None
            area = monitor.get_geometry() if monitor else None
            if area is not None:
                self.move(area.x, area.y)
                self.set_default_size(area.width, area.height)
            self.fullscreen()
            outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16, margin=40)
            self.add(outer)
            self.flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=5,
                                    row_spacing=18, column_spacing=18, valign=Gtk.Align.CENTER, halign=Gtk.Align.CENTER)
            outer.pack_start(self.flow, True, True, 0)
            self.desks = Gtk.Box(spacing=12, halign=Gtk.Align.CENTER)
            outer.pack_end(self.desks, False, False, 0)
            self.connect("key-press-event", self.key)
            self.connect("button-release-event", lambda *_: self.close_view())
            self.refresh()

        def refresh(self):
            for child in self.flow.get_children() + self.desks.get_children(): child.destroy()
            desks = desktops() or [(0, display_name(0, ""), True)]
            current = next((number for number, _, on in desks if on), 0)
            shown = [item for item in windows() if item[1] == current and item[2] not in ("xfce4-panel", "xfdesktop")]
            shown = [item for item in shown if int(item[0], 16) != self.own_id()]
            if not shown:
                label = Gtk.Label(label=_t("Aucune fenêtre ouverte sur ce bureau."))
                label.get_style_context().add_class("hint")
                self.flow.add(label)
            for window, _, title in shown:
                self.flow.add(self.window_card(window, title, desks, current))
            for number, name, on in desks:
                self.desks.pack_start(self.desk_card(number, name, on, len(desks)), False, False, 0)
            add = Gtk.Button(label="+  " + _t("Nouveau bureau"))
            add.get_style_context().add_class("new")
            add.connect("clicked", lambda *_: (new_desktop(), GLib.timeout_add(150, self.refresh_later)))
            self.desks.pack_start(add, False, False, 0)
            self.show_all()

        def refresh_later(self):
            self.refresh()
            return False

        def own_id(self):
            gdk_window = self.get_window()
            return gdk_window.get_xid() if gdk_window is not None and hasattr(gdk_window, "get_xid") else 0

        def window_card(self, window, title, desks, current):
            event = Gtk.EventBox()
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            box.get_style_context().add_class("card")
            event.add(box)
            head = Gtk.Box(spacing=6)
            name = Gtk.Label(label=title, xalign=0, ellipsize=3, max_width_chars=30)
            name.get_style_context().add_class("title")
            head.pack_start(name, True, True, 0)
            close = Gtk.Button(label="✕")
            close.get_style_context().add_class("x")
            close.set_tooltip_text(_t("Fermer"))
            close.connect("clicked", lambda *_: (_run(["wmctrl", "-i", "-c", window]), GLib.timeout_add(300, self.refresh_later)))
            head.pack_start(close, False, False, 0)
            box.pack_start(head, False, False, 0)
            pixbuf = thumbnail(window)
            image = Gtk.Image.new_from_pixbuf(pixbuf) if pixbuf else Gtk.Image.new_from_icon_name("application-x-executable", Gtk.IconSize.DIALOG)
            image.set_size_request(280, 170)
            box.pack_start(image, True, True, 0)

            def clicked(_, event):
                if event.button == 3:
                    menu = Gtk.Menu()
                    for number, label, _ in desks:
                        if number == current: continue
                        item = Gtk.MenuItem(label=_t("Déplacer vers") + " " + label)
                        item.connect("activate", lambda *_, n=number: (_run(["wmctrl", "-i", "-r", window, "-t", str(n)]),
                                                                       self.refresh()))
                        menu.append(item)
                    item = Gtk.MenuItem(label=_t("Déplacer vers") + " " + _t("Nouveau bureau"))
                    item.connect("activate", lambda *_: self.move_to_new(window))
                    menu.append(item)
                    menu.show_all()
                    menu.popup_at_pointer(event)
                    return True
                if event.button == 1:
                    self.hide()
                    _run(["wmctrl", "-i", "-a", window])
                    Gtk.main_quit()
                return True
            event.connect("button-release-event", clicked)
            return event

        def move_to_new(self, window):
            count = len(desktops()) or 1
            _run(["wmctrl", "-n", str(count + 1)])
            _run(["wmctrl", "-i", "-r", window, "-t", str(count)])
            GLib.timeout_add(150, self.refresh_later)

        def desk_card(self, number, name, on, count):
            event = Gtk.EventBox()
            box = Gtk.Box(spacing=8)
            box.get_style_context().add_class("desk")
            if on: box.get_style_context().add_class("on")
            event.add(box)
            box.pack_start(Gtk.Label(label=name), False, False, 0)
            if count > 1:
                close = Gtk.Button(label="✕")
                close.get_style_context().add_class("x")
                close.set_tooltip_text(_t("Supprimer le bureau"))
                close.connect("clicked", lambda *_: (remove_desktop(number), GLib.timeout_add(200, self.refresh_later)))
                box.pack_start(close, False, False, 0)

            def clicked(_, event):
                if event.button == 3:
                    menu = Gtk.Menu()
                    item = Gtk.MenuItem(label=_t("Renommer"))
                    item.connect("activate", lambda *_: self.rename(number, name))
                    menu.append(item)
                    if count > 1:
                        item = Gtk.MenuItem(label=_t("Supprimer le bureau"))
                        item.connect("activate", lambda *_: (remove_desktop(number), GLib.timeout_add(200, self.refresh_later)))
                        menu.append(item)
                    menu.show_all()
                    menu.popup_at_pointer(event)
                elif event.button == 1:
                    _run(["wmctrl", "-s", str(number)])
                    GLib.timeout_add(150, self.refresh_later)
                return True
            event.connect("button-release-event", clicked)
            return event

        def rename(self, number, name):
            dialog = Gtk.Dialog(title=_t("Renommer"), transient_for=self, modal=True)
            dialog.add_buttons("OK", Gtk.ResponseType.OK)
            entry = Gtk.Entry(text=name, activates_default=True)
            dialog.get_content_area().pack_start(Gtk.Label(label=_t("Nom du bureau"), xalign=0), False, False, 6)
            dialog.get_content_area().pack_start(entry, False, False, 6)
            dialog.set_default_response(Gtk.ResponseType.OK)
            dialog.show_all()
            if dialog.run() == Gtk.ResponseType.OK and entry.get_text().strip():
                rename_desktop(number, entry.get_text())
            dialog.destroy()
            GLib.timeout_add(150, self.refresh_later)

        def key(self, _, event):
            if event.keyval in (Gdk.KEY_Escape, Gdk.KEY_Super_L, Gdk.KEY_Super_R):
                self.close_view()
                return True
            return False

        def close_view(self):
            Gtk.main_quit()
            return True

    view = View()
    view.present()
    Gtk.main()
    return 0


def main(argv):
    command = argv[1] if len(argv) > 1 else "view"
    if command == "view": return run_view()
    if command == "new": return new_desktop()
    if command == "close":
        current = next((number for number, _, on in desktops() if on), 0)
        return remove_desktop(current)
    if command == "rename" and len(argv) == 4:
        return 0 if rename_desktop(int(argv[2]), argv[3]) else 1
    if command == "list":
        for number, name, on in desktops():
            print(f"{'*' if on else ' '} {number} {name}")
        for window, desktop, title in windows():
            print(f"  {desktop} {title}")
        return 0
    print(__doc__.strip().split("\n\n")[-2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
