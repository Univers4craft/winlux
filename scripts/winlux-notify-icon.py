#!/usr/bin/python3
"""Zone de notification des programmes Windows (assistant de Shell_NotifyIcon).

Lancé par le runtime Winlux à la première icône d'un programme. Lit sur son
entrée des lignes séparées par des tabulations :
  set <clé> <exécutable> <infobulle> <visible 0|1>
  delete <clé>
  balloon <clé> <titre> <texte> <drapeaux NIIF_*>
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
BUTTONS = {1: "l", 2: "m", 3: "r"}

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

def pressed(icon, event, key):
    prefix = BUTTONS.get(event.button)
    if prefix is None: return False
    kind = "dbl" if event.type == Gdk.EventType._2BUTTON_PRESS else "down"
    if kind == "dbl" and prefix != "l": return False
    emit(key, prefix + kind, event.x_root, event.y_root)
    return False

def released(icon, event, key):
    prefix = BUTTONS.get(event.button)
    if prefix: emit(key, prefix + "up", event.x_root, event.y_root)
    return False

def apply(fields):
    command = fields[0]
    if command == "set" and len(fields) >= 5:
        key, executable, tip, visible = fields[1:5]
        icon = ICONS.get(key)
        if icon is None:
            icon = Gtk.StatusIcon()
            icon.connect("button-press-event", pressed, key)
            icon.connect("button-release-event", released, key)
            ICONS[key] = icon
            pixbuf = pixbuf_for(executable)
            if pixbuf is not None: icon.set_from_pixbuf(pixbuf)
            else: icon.set_from_icon_name("application-x-executable")
            icon.set_title(Path(executable).stem or "Winlux")
        icon.set_tooltip_text(tip or Path(executable).stem)
        icon.set_visible(visible == "1")
    elif command == "delete" and len(fields) >= 2:
        icon = ICONS.pop(fields[1], None)
        if icon is not None: icon.set_visible(False)
    elif command == "balloon" and len(fields) >= 5:
        key, title, text, flags = fields[1:5]
        level = {"1": "dialog-information", "2": "dialog-warning", "3": "dialog-error"}.get(str(int(flags or 0) & 3), "dialog-information")
        icon = ICONS.get(key)
        name = icon.get_title() if icon is not None else "Winlux"
        if shutil.which("notify-send"):
            try:
                subprocess.Popen(["notify-send", "--app-name", name, "--icon", level, title or name, text],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                emit(key, "balloon-show")
            except OSError: pass

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
