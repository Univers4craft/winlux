#!/usr/bin/python3
"""Zone de notification des programmes Windows (assistant de Shell_NotifyIcon).

Lancé par le runtime Winlux à la première icône d'un programme. Lit sur son
entrée des lignes séparées par des tabulations :
  set <clé> <exécutable> <infobulle> <visible 0|1>
  delete <clé>
  balloon <clé> <titre> <texte> <drapeaux NIIF_*>
  menu <clé>      (programme sans menu : menu de Winlux sur l'icône, #927)
et écrit sur sa sortie « <clé> <événement> <x> <y> » pour chaque clic
(ldown, lup, ldbl, rdown, rup, mdown, mup, balloon-show). Il se termine quand
le programme se ferme (fin de l'entrée).
"""
import hashlib
from importlib.machinery import SourceFileLoader
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import warnings
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GdkPixbuf, GLib, Gtk

warnings.filterwarnings("ignore", category=DeprecationWarning)  # Gtk.StatusIcon = XEmbed.

def catalogue():
    """Extraction d'icône PE partagée avec winlux-applications (sans exécuter le PE)."""
    for candidate in [os.environ.get("WINLUX_APPLICATIONS", ""), "/usr/bin/winlux-applications",
                      str(Path(__file__).resolve().parent / "winlux-applications.py")]:
        if candidate and Path(candidate).is_file():
            loader = SourceFileLoader("winlux_applications", candidate)
            spec = importlib.util.spec_from_loader("winlux_applications", loader)
            module = importlib.util.module_from_spec(spec)
            loader.exec_module(module)
            return module
    return None

CATALOGUE = catalogue()
ICONS = {}
NAMES = {}  # clé → nom de l'icône pour la zone de notification

def language():
    """Langue de l'interface (Paramètres), fr par défaut."""
    lang = os.environ.get("WINLUX_LANG", "")
    if not lang:
        try:
            for line in Path("/etc/winlux/locale.conf").read_text(encoding="utf-8").splitlines():
                if line.startswith("LANG="): lang = line[5:].strip().strip('"')
        except OSError: pass
    return lang[:2] if lang[:2] in ("en", "es", "de") else "fr"

HIDE = {"fr": "Ranger dans les icônes cachées", "en": "Move to hidden icons",
        "es": "Mover a iconos ocultos", "de": "In ausgeblendete Symbole verschieben"}

def hide(name):
    for tray in ("/usr/bin/winlux-tray", str(Path(__file__).resolve().parent / "winlux-tray.py")):
        if Path(tray).is_file():
            command = [tray] if not tray.endswith(".py") else [sys.executable, "-B", tray]
            try: subprocess.Popen(command + ["overflow", "hide", name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except OSError: pass
            return

def own_menu(key, event=None):
    """Petit menu de Winlux sur l'icône (#927) : programme sans menu, ou
    Maj + clic droit. « Ranger dans les icônes cachées », annulable depuis
    Paramètres > Personnalisation."""
    menu = Gtk.Menu()
    item = Gtk.MenuItem(label=HIDE[language()])
    item.connect("activate", lambda *_: hide(NAMES.get(key, GLib.get_prgname() or "")))
    menu.append(item)
    menu.show_all()
    # Placé contre l'icône, comme le menu d'une icône de la zone de
    # notification (popup_at_pointer refuse de s'ouvrir sans clic en cours).
    icon = ICONS.get(key)
    button, when = (event.button, event.time) if event is not None else (3, Gtk.get_current_event_time())
    if icon is not None:
        menu.popup(None, None, Gtk.StatusIcon.position_menu, icon, button, when)
    else:
        menu.popup(None, None, None, None, button, when)
    OPEN_MENUS.append(menu)  # gardé en vie le temps de l'affichage
    menu.connect("deactivate", lambda m: OPEN_MENUS.remove(m) if m in OPEN_MENUS else None)

OPEN_MENUS = []
BUTTONS = {1: "l", 2: "m", 3: "r"}
TIPS = {}      # clé → texte de l'infobulle
QUIET = set()  # clés dont l'infobulle est retirée depuis un clic

def pointer_on(icon):
    """Le pointeur est-il sur l'icône ?"""
    try:
        found, _screen, area, _orientation = icon.get_geometry()
        _screen, x, y = Gdk.Display.get_default().get_default_seat().get_pointer().get_position()
    except (AttributeError, TypeError, ValueError):
        return False
    return bool(found) and area.x <= x < area.x + area.width and area.y <= y < area.y + area.height

def quiet_tooltip(key):
    """Clic sur l'icône : l'infobulle s'efface, comme sous Windows, pour ne
    pas rester par-dessus le menu du programme. Elle revient une fois le
    pointeur sorti de l'icône (#1371)."""
    icon = ICONS.get(key)
    if icon is None: return
    icon.set_has_tooltip(False)
    if key not in QUIET:
        QUIET.add(key)
        GLib.timeout_add(300, tooltip_back, key)

def tooltip_back(key):
    icon = ICONS.get(key)
    if icon is None:
        QUIET.discard(key)
        return False
    if pointer_on(icon): return True  # encore sur l'icône : on attend
    QUIET.discard(key)
    icon.set_tooltip_text(TIPS.get(key, ""))
    return False

def emit(key, event, x=0, y=0):
    try:
        sys.stdout.write(f"{key}\t{event}\t{int(x)}\t{int(y)}\n")
        sys.stdout.flush()
    except (BrokenPipeError, OSError):
        Gtk.main_quit()

def pixbuf_for(executable):
    path = Path(executable)
    if CATALOGUE is not None and path.is_file():
        try:
            icon = CATALOGUE.icon_for(path, hashlib.sha256(str(path).encode()).hexdigest()[:24])
            if icon.startswith("/"): return GdkPixbuf.Pixbuf.new_from_file_at_scale(icon, 64, 64, True)
        except (GLib.Error, OSError, ValueError): pass
    return None

def program_name(executable):
    """Nom de l'icône pour la zone de notification : nom du .exe, en minuscules."""
    stem = Path(executable.replace("\\", "/")).stem.lower()
    cleaned = "".join(c if c.isalnum() or c in "-_." else "-" for c in stem).strip("-.")
    return cleaned or "winlux-programme"

def pressed(icon, event, key):
    prefix = BUTTONS.get(event.button)
    if prefix is None: return False
    quiet_tooltip(key)
    if prefix == "r" and event.state & Gdk.ModifierType.SHIFT_MASK:
        own_menu(key, event)  # Maj + clic droit : menu de Winlux, pas celui du programme
        return True
    kind = "dbl" if event.type == Gdk.EventType._2BUTTON_PRESS else "down"
    if kind == "dbl" and prefix != "l": return False
    emit(key, prefix + kind, event.x_root, event.y_root)
    return False

def released(icon, event, key):
    prefix = BUTTONS.get(event.button)
    if prefix == "r" and event.state & Gdk.ModifierType.SHIFT_MASK: return True
    if prefix: emit(key, prefix + "up", event.x_root, event.y_root)
    return False

def apply(fields):
    command = fields[0]
    if command == "set" and len(fields) >= 5:
        key, executable, tip, visible = fields[1:5]
        icon = ICONS.get(key)
        if icon is None:
            # Nom propre au programme (putty, filezilla…) : la zone de
            # notification d'XFCE retient et cache chaque icône par ce nom.
            # Sans lui, toutes les icônes .exe s'appelaient
            # « winlux-notify-icon.py » et se cachaient ensemble. Posé avant
            # la première fenêtre ; un assistant sert un seul programme.
            if not ICONS:
                GLib.set_prgname(program_name(executable))
            NAMES[key] = program_name(executable)
            icon = Gtk.StatusIcon()
            icon.connect("button-press-event", pressed, key)
            icon.connect("button-release-event", released, key)
            ICONS[key] = icon
            pixbuf = pixbuf_for(executable)
            if pixbuf is not None: icon.set_from_pixbuf(pixbuf)
            else: icon.set_from_icon_name("application-x-executable")
            icon.set_title(Path(executable).stem or "Winlux")
        TIPS[key] = tip or Path(executable).stem
        if key not in QUIET:  # pendant un menu, le texte attend le retour de l'infobulle
            icon.set_tooltip_text(TIPS[key])
        icon.set_visible(visible == "1")
    elif command == "menu" and len(fields) >= 2 and fields[1] in ICONS:
        own_menu(fields[1])  # programme sans menu (pas de message de rappel)
    elif command == "delete" and len(fields) >= 2:
        icon = ICONS.pop(fields[1], None)
        TIPS.pop(fields[1], None)
        if icon is not None: icon.set_visible(False)
    elif command == "balloon" and len(fields) >= 5:
        key, title, text, flags = fields[1:5]
        level = {"1": "dialog-information", "2": "dialog-warning", "3": "dialog-error"}.get(str(int(flags or 0) & 3), "dialog-information")
        icon = ICONS.get(key)
        name = icon.get_title() if icon is not None else "Winlux"
        if show_notification(name, level, title or name, text):
            emit(key, "balloon-show")

def show_notification(app, icon, title, text):
    """Bulle par org.freedesktop.Notifications (winlux-notifications), sans
    dépendre de notify-send, absent de l'image (#630). notify-send reste un
    repli si aucun serveur ne répond sur le bus."""
    try:
        from gi.repository import Gio
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                      "org.freedesktop.Notifications", "Notify",
                      GLib.Variant("(susssasa{sv}i)", (app, 0, icon, title, text, [], {}, -1)),
                      None, Gio.DBusCallFlags.NONE, 2000, None)
        return True
    except (GLib.Error, ImportError, ValueError):
        pass
    if shutil.which("notify-send"):
        try:
            subprocess.Popen(["notify-send", "--app-name", app, "--icon", icon, title, text],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return True
        except OSError:
            pass
    return False

BUFFER = bytearray()

def readable(source, condition):
    try: data = os.read(source, 65536)
    except OSError: data = b""
    if not data:
        Gtk.main_quit()
        return False
    BUFFER.extend(data)
    while b"\n" in BUFFER:
        line, _, rest = bytes(BUFFER).partition(b"\n")
        BUFFER[:] = rest
        fields = line.decode("utf-8", "replace").split("\t")
        if fields and fields[0]: apply(fields)
    return True

def main():
    GLib.io_add_watch(sys.stdin.fileno(), GLib.PRIORITY_DEFAULT, GLib.IO_IN | GLib.IO_HUP | GLib.IO_ERR, readable)
    Gtk.main()

if __name__ == "__main__": main()
