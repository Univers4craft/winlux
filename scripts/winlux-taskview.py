#!/usr/bin/python3
"""Vue des tâches (Windows + Tab) et bureaux virtuels.

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
# Noms automatiques (openbox nomme les nouveaux bureaux dans la langue du
# système, « Espace de travail 2 » en français) : affichés « Bureau N ».
GENERIC_NAME = re.compile(r"\s*((desktop|bureau|workspace|espace de travail|escritorio|[áa]rea de trabajo"
                          r"|arbeitsfl[äa]che|arbeitsbereich|schreibtisch)\s*\d*|bureau winlux)?\s*", re.I)


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
    """[(id hexadécimal, bureau, titre)] des fenêtres normales ; bureau -1 :
    sur tous les bureaux."""
    found = []
    for line in _run(["wmctrl", "-l"]).splitlines():
        parts = line.split(None, 3)
        if len(parts) < 3: continue
        try: desktop = int(parts[1])
        except ValueError: continue
        if desktop < -1: continue
        kind = _run(["xprop", "-id", parts[0], "_NET_WM_WINDOW_TYPE"])
        if "_NET_WM_WINDOW_TYPE_NORMAL" not in kind and "_NET_WM_WINDOW_TYPE_DIALOG" not in kind \
                and "_NET_WM_WINDOW_TYPE(ATOM)" in kind:
            continue
        title = parts[3] if len(parts) > 3 else ""
        found.append((parts[0], desktop, title))
    return found


def on_desktop(found, current):
    """Fenêtres de `found` à montrer sur le bureau `current` : les siennes et
    celles affichées sur tous les bureaux (bureau -1, #3346)."""
    return [item for item in found if item[1] in (current, -1)]


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


def add_desktop():
    """Ajoute un bureau nommé « Bureau N » ; rend son numéro."""
    count = len(desktops()) or 1
    names = (desktop_names() + [""] * count)[:count]
    _run(["wmctrl", "-n", str(count + 1)])
    # Sans nom, openbox appelle le bureau « desktop N » ou « Espace de travail N »,
    # ce qu'affichent sa bulle de changement de bureau et les autres outils (#3350).
    set_desktop_names(names + [_t("Bureau {n}").format(n=count + 1)])
    return count


def new_desktop():
    _run(["wmctrl", "-s", str(add_desktop())])
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


def move_window(window, number):
    """Fenêtre `window` (identifiant hexadécimal) vers le bureau `number`."""
    _run(["wmctrl", "-i", "-r", window, "-t", str(number)])


def rename_desktop(number, name):
    count = len(desktops())
    names = (desktop_names() + [""] * count)[:count]
    names[number] = name.strip()
    return set_desktop_names(names)


class ClientListWatch:
    """Connexion X à part qui signale chaque changement de _NET_CLIENT_LIST
    sur la fenêtre racine : une fenêtre ouverte ou fermée pendant que la vue
    est affichée (#3347). Rien ne tourne entre deux changements."""
    PROPERTY_NOTIFY = 28
    PROPERTY_CHANGE_MASK = 1 << 22

    def __init__(self):
        self.x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
        x11 = self.x11
        x11.XOpenDisplay.restype = ctypes.c_void_p
        x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x11.XDefaultRootWindow.restype = ctypes.c_ulong
        x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        x11.XInternAtom.restype = ctypes.c_ulong
        x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
        x11.XSelectInput.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_long]
        x11.XConnectionNumber.argtypes = [ctypes.c_void_p]
        x11.XPending.argtypes = [ctypes.c_void_p]
        x11.XNextEvent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        x11.XFlush.argtypes = [ctypes.c_void_p]
        x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
        self.display = x11.XOpenDisplay(None)
        if not self.display: raise OSError("XOpenDisplay")
        self.atom = x11.XInternAtom(self.display, b"_NET_CLIENT_LIST", 0)
        x11.XSelectInput(self.display, x11.XDefaultRootWindow(self.display), self.PROPERTY_CHANGE_MASK)
        x11.XFlush(self.display)
        self.event = (ctypes.c_long * 24)()  # XEvent : 192 octets

    def fileno(self):
        return self.x11.XConnectionNumber(self.display)

    def changed(self):
        """Vide la file ; vrai si _NET_CLIENT_LIST a changé."""
        changed = False
        while self.x11.XPending(self.display):
            self.x11.XNextEvent(self.display, ctypes.byref(self.event))
            # XPropertyEvent : type (int), serial, send_event, display, window, atom.
            if self.event[0] & 0xFFFFFFFF == self.PROPERTY_NOTIFY and self.event[5] == self.atom:
                changed = True
        return changed

    def close(self):
        if self.display:
            self.x11.XCloseDisplay(self.display)
            self.display = None


# ── Vue des tâches (GTK) ────────────────────────────────────────────────────

MARGIN = 40          # marge autour de la vue
STRIP = 64           # bande des bureaux (bouton de 44 px) et son écart avec la grille
GAP = 18             # écart entre deux cartes
CARD_W, CARD_H = 24, 52  # cadre d'une carte autour de l'aperçu (mesuré sous Xvfb : 22 et 46 px)
THUMB_MAX = (280, 170)


def grid_layout(count, width, height):
    """(colonnes, largeur, hauteur de l'aperçu) pour que `count` cartes et la
    bande des bureaux tiennent dans un écran `width` × `height` (#3066).

    Le nombre de colonnes retenu est celui qui donne les plus grands aperçus,
    au format 280 × 170, sans dépasser cette taille ; à taille égale, le plus
    grand, pour une rangée plutôt qu'une colonne."""
    count = max(1, count)
    room_w = width - 2 * MARGIN
    room_h = height - 2 * MARGIN - STRIP
    best = None
    for columns in range(1, count + 1):
        rows = -(-count // columns)
        thumb_w = (room_w - (columns - 1) * GAP) // columns - CARD_W
        thumb_h = (room_h - (rows - 1) * GAP) // rows - CARD_H
        thumb_h = min(thumb_h, THUMB_MAX[1], thumb_w * THUMB_MAX[1] // THUMB_MAX[0])
        if best is None or thumb_h >= best[2]:
            best = (columns, thumb_h * THUMB_MAX[0] // THUMB_MAX[1], thumb_h)
    columns, _, thumb_h = best
    thumb_h = max(thumb_h, 12)
    return columns, thumb_h * THUMB_MAX[0] // THUMB_MAX[1], thumb_h


def _pid_file():
    """Fichier de la vue ouverte, propre au compte (#3074)."""
    folder = os.environ.get("XDG_RUNTIME_DIR") or f"/tmp/winlux-{os.getuid()}"
    display = "".join(c for c in os.environ.get("DISPLAY", ":0").rpartition(":")[2].split(".")[0] if c.isdigit()) or "0"
    return os.path.join(folder, f"winlux-taskview-{display}.pid")


def running_view(path):
    """Pid d'une vue des tâches encore vivante, sinon None."""
    try:
        with open(path, encoding="ascii") as handle:
            pid = int(handle.read().strip())
        with open(f"/proc/{pid}/cmdline", "rb") as handle:
            command = handle.read()
    except (OSError, ValueError):
        return None
    return pid if pid != os.getpid() and b"winlux-taskview" in command else None


def run_view():
    # Une seule vue par écran (#3074) : un second appel ferme la première.
    path = _pid_file()
    other = running_view(path)
    if other is not None:
        try: os.kill(other, 15)
        except OSError: pass
        return 0
    try:
        os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
        with open(path, "w", encoding="ascii") as handle:
            handle.write(f"{os.getpid()}\n")
    except OSError:
        pass
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    gi.require_version("GdkX11", "3.0")
    from gi.repository import Gdk, GdkX11, GLib, Gtk

    def thumbnail(window, width, height):
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

    DRAG_TARGET = Gtk.TargetEntry.new("application/x-winlux-window", Gtk.TargetFlags.SAME_APP, 0)

    class View(Gtk.Window):
        dragging = False

        def __init__(self):
            super().__init__(title=_t("Vue des tâches"))
            self.set_role("winlux-taskview")
            self.set_decorated(False)
            self.set_keep_above(True)
            self.set_skip_taskbar_hint(True)
            # Sur tous les bureaux (#3074) : « Nouveau bureau » ou la suppression
            # du bureau courant changent de bureau, la vue doit suivre au lieu
            # de rester cachée sur l'ancien, hors de portée d'Échap.
            self.stick()
            screen = self.get_screen()
            visual = screen.get_rgba_visual()
            if visual is not None and screen.is_composited(): self.set_visual(visual)
            # Pas d'app_paintable : GTK ne peignait alors le voile que sous la
            # zone centrale (marge de 40 px), les fenêtres restaient nettes et
            # cliquables sur tout le pourtour de l'écran.
            css = Gtk.CssProvider()
            css.load_from_data(b"""
              window { background: rgba(16, 18, 26, 0.86); color: #ecedf5; }
              .card { background: #2a3040; border-radius: 8px; padding: 6px; border: 2px solid transparent; }
              flowboxchild { padding: 0; outline: none; }
              flowboxchild:focus .card, .desk.focus { border-color: #ecedf5; background: #343b4e; }
              .card:hover { background: #343b4e; }
              .title { font-size: 13px; }
              .hint { color: #adb6c9; font-size: 15px; }
              .desk { background: #2a3040; border-radius: 8px; padding: 8px 16px; color: #ecedf5; border: 2px solid transparent; }
              .desk.on { border-color: #b69cff; }
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
            outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=STRIP - 44, margin=MARGIN)
            self.add(outer)
            self.area = (area.width, area.height) if area is not None else (screen.get_width(), screen.get_height())
            # FlowBox centré : GTK lui donne sa largeur naturelle, une seule
            # colonne, et la vue débordait de l'écran (#3066). Les colonnes
            # sont fixées par grid_layout.
            # Clavier (#3075) : les flèches passent d'une carte à l'autre,
            # Entrée ou Espace ouvre la fenêtre, Suppr la ferme ; Tab mène
            # à la bande des bureaux. La souris reste sur les EventBox.
            self.flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, homogeneous=True, activate_on_single_click=False,
                                    row_spacing=GAP, column_spacing=GAP, valign=Gtk.Align.CENTER, halign=Gtk.Align.CENTER)
            outer.pack_start(self.flow, True, True, 0)
            self.desks = Gtk.Box(spacing=12, halign=Gtk.Align.CENTER)
            outer.pack_end(self.desks, False, False, 0)
            self.flow.connect("child-activated", lambda _, child: getattr(child.get_child(), "window_id", None)
                              and self.open_window(child.get_child().window_id))
            self.connect("key-press-event", self.key)
            # GTK efface l'indicateur de focus au premier affichage : remis
            # après, sinon la carte choisie au clavier ne se voit pas.
            self.connect("map-event", lambda *_: GLib.idle_add(self.show_focus))
            self.connect("button-release-event", lambda *_: self.close_view())
            self.refresh()
            # Fenêtres ouvertes ou fermées pendant que la vue est affichée (#3347).
            try:
                self.watch = ClientListWatch()
                GLib.io_add_watch(self.watch.fileno(), GLib.PRIORITY_DEFAULT, GLib.IO_IN, self.clients_changed)
            except OSError:
                self.watch = None

        def clients_changed(self, *_):
            if self.watch is not None and self.watch.changed() and not getattr(self, "pending", False):
                # Une rafale (programme qui ouvre trois fenêtres) ne refait la vue qu'une fois.
                self.pending = True
                GLib.timeout_add(80, self.refresh_clients)
            return True

        def refresh_clients(self):
            if self.dragging or Gtk.grab_get_current() is not None:
                return True  # glisser ou menu en cours : la vue sera refaite après
            self.pending = False
            desks = desktops()
            current = next((number for number, _, on in desks if on), 0)
            if [item[0] for item in self.listed(current)] != getattr(self, "shown_ids", None):
                self.refresh()
            return False

        def listed(self, current):
            shown = [item for item in on_desktop(windows(), current) if item[2] not in ("xfce4-panel", "xfdesktop")]
            return [item for item in shown if int(item[0], 16) != self.own_id()]

        def refresh(self):
            focused = self.get_focus()
            focused = getattr(focused.get_child(), "window_id", None) if isinstance(focused, Gtk.FlowBoxChild) else None
            for child in self.flow.get_children() + self.desks.get_children(): child.destroy()
            desks = desktops() or [(0, display_name(0, ""), True)]
            current = next((number for number, _, on in desks if on), 0)
            shown = self.listed(current)
            self.shown_ids = [item[0] for item in shown]
            if not shown:
                label = Gtk.Label(label=_t("Aucune fenêtre ouverte sur ce bureau."))
                label.get_style_context().add_class("hint")
                self.flow.add(label)
            columns, thumb_w, thumb_h = grid_layout(len(shown), *self.area)
            self.flow.set_min_children_per_line(columns)
            self.flow.set_max_children_per_line(columns)
            for window, _, title in shown:
                self.flow.add(self.window_card(window, title, desks, current, thumb_w, thumb_h))
            self.desk_events = []
            for number, name, on in desks:
                card = self.desk_card(number, name, on, len(desks))
                self.desk_events.append(card)
                if on: self.current_desk = card
                self.desks.pack_start(card, False, False, 0)
            add = Gtk.Button(label="+  " + _t("Nouveau bureau"))
            add.get_style_context().add_class("new")
            add.connect("clicked", lambda *_: (new_desktop(), GLib.timeout_add(150, self.refresh_later)))
            self.desks.pack_start(add, False, False, 0)
            self.show_all()
            # La carte qui avait le focus le garde quand la vue est refaite.
            first = self.flow.get_child_at_index(self.shown_ids.index(focused) if focused in self.shown_ids else 0)
            if shown and first is not None:
                first.grab_focus()
            elif getattr(self, "current_desk", None) is not None:
                self.current_desk.grab_focus()

        def show_focus(self):
            self.set_focus_visible(True)
            return False

        def open_window(self, window):
            self.hide()
            _run(["wmctrl", "-i", "-a", window])
            Gtk.main_quit()

        def go_to(self, number):
            # Choisir un bureau y mène et ferme la vue (#3074).
            self.hide()
            _run(["wmctrl", "-s", str(number)])
            Gtk.main_quit()

        def refresh_later(self):
            self.refresh()
            return False

        def own_id(self):
            gdk_window = self.get_window()
            return gdk_window.get_xid() if gdk_window is not None and hasattr(gdk_window, "get_xid") else 0

        def window_card(self, window, title, desks, current, thumb_w, thumb_h):
            event = Gtk.EventBox()
            event.window_id = window
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            box.get_style_context().add_class("card")
            event.add(box)
            head = Gtk.Box(spacing=6)
            name = Gtk.Label(label=title, xalign=0, ellipsize=3, max_width_chars=30, width_chars=1)
            name.get_style_context().add_class("title")
            head.pack_start(name, True, True, 0)
            close = Gtk.Button(label="✕")
            close.get_style_context().add_class("x")
            close.set_tooltip_text(_t("Fermer"))
            close.set_can_focus(False)  # au clavier : Suppr sur la carte
            close.connect("clicked", lambda *_: (_run(["wmctrl", "-i", "-c", window]), GLib.timeout_add(300, self.refresh_later)))
            head.pack_start(close, False, False, 0)
            box.pack_start(head, False, False, 0)
            pixbuf = thumbnail(window, thumb_w, thumb_h)
            image = Gtk.Image.new_from_pixbuf(pixbuf) if pixbuf else Gtk.Image.new_from_icon_name("application-x-executable", Gtk.IconSize.DIALOG)
            image.set_size_request(thumb_w, thumb_h)
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
                if event.button == 1 and not self.dragging:
                    self.open_window(window)
                return True
            event.connect("button-release-event", clicked)
            # Glisser la carte sur un bureau de la bande l'y déplace (#3076).
            event.drag_source_set(Gdk.ModifierType.BUTTON1_MASK, [DRAG_TARGET], Gdk.DragAction.MOVE)
            if pixbuf is not None:
                event.drag_source_set_icon_pixbuf(pixbuf.scale_simple(max(1, pixbuf.get_width() // 2),
                                                                      max(1, pixbuf.get_height() // 2), 2))
            event.connect("drag-begin", lambda *_: setattr(self, "dragging", True))
            event.connect("drag-end", lambda *_: GLib.idle_add(lambda: setattr(self, "dragging", False)))
            event.connect("drag-data-get", lambda _w, _c, data, *_: data.set(data.get_target(), 8, window.encode("ascii")))
            return event

        def drop(self, data, number):
            window = bytes(data or b"").decode("ascii", "replace")
            if re.fullmatch(r"0x[0-9a-fA-F]+", window):
                move_window(window, number)
                GLib.timeout_add(150, self.refresh_later)

        def move_to_new(self, window):
            move_window(window, add_desktop())
            GLib.timeout_add(150, self.refresh_later)

        def desk_card(self, number, name, on, count):
            event = Gtk.EventBox()
            box = Gtk.Box(spacing=8)
            box.get_style_context().add_class("desk")
            if on: box.get_style_context().add_class("on")
            event.add(box)
            event.set_can_focus(True)
            event.connect("focus-in-event", lambda *_: box.get_style_context().add_class("focus"))
            event.connect("focus-out-event", lambda *_: box.get_style_context().remove_class("focus"))
            event.connect("key-press-event", lambda _, key: key.keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter, Gdk.KEY_space)
                          and (self.go_to(number) or True))
            box.pack_start(Gtk.Label(label=name), False, False, 0)
            if count > 1:
                close = Gtk.Button(label="✕")
                close.get_style_context().add_class("x")
                close.set_tooltip_text(_t("Supprimer le bureau"))
                close.set_can_focus(False)
                close.connect("clicked", lambda *_: (remove_desktop(number), GLib.timeout_add(200, self.refresh_later)))
                box.pack_start(close, False, False, 0)

            event.drag_dest_set(Gtk.DestDefaults.ALL, [DRAG_TARGET], Gdk.DragAction.MOVE)
            event.connect("drag-motion", lambda *_: box.get_style_context().add_class("focus") or False)
            event.connect("drag-leave", lambda *_: box.get_style_context().remove_class("focus"))
            event.connect("drag-data-received", lambda _w, _c, _x, _y, data, *_: self.drop(data.get_data(), number))

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
                    self.go_to(number)
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
            focused = self.get_focus()
            # Tab : de la grille au bureau courant de la bande, et retour.
            if event.keyval in (Gdk.KEY_Tab, Gdk.KEY_ISO_Left_Tab, Gdk.KEY_KP_Tab):
                if isinstance(focused, Gtk.FlowBoxChild) or focused is None:
                    target = getattr(self, "current_desk", None)
                else:
                    target = self.flow.get_child_at_index(0)
                    target = target if target is not None and getattr(target.get_child(), "window_id", None) else None
                if target is not None:
                    target.grab_focus()
                    return True
            if event.keyval == Gdk.KEY_Delete and isinstance(focused, Gtk.FlowBoxChild) \
                    and getattr(focused.get_child(), "window_id", None):
                _run(["wmctrl", "-i", "-c", focused.get_child().window_id])
                GLib.timeout_add(300, self.refresh_later)
                return True
            return False

        def close_view(self):
            Gtk.main_quit()
            return True

    import signal
    GLib.unix_signal_add(GLib.PRIORITY_HIGH, signal.SIGTERM, lambda: (Gtk.main_quit(), False)[1])
    view = View()
    view.present()
    try:
        Gtk.main()
    finally:
        if running_view(path) is None:
            try: os.unlink(path)
            except OSError: pass
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
