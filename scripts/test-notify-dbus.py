#!/usr/bin/python3
"""#630 : les bulles des programmes Windows (Shell_NotifyIcon NIIF_*) et la
notification de « Coller » sur le bureau passent par D-Bus, sans notify-send.

À lancer dans dbus-run-session : un faux serveur org.freedesktop.Notifications
reçoit les appels ; PATH ne contient aucun notify-send.
"""
import ast
import os
import sys
from pathlib import Path

from gi.repository import Gio, GLib

ROOT = Path(__file__).resolve().parent
os.environ["PATH"] = "/nonexistent"  # aucun notify-send possible

XML = """<node><interface name='org.freedesktop.Notifications'>
<method name='Notify'><arg type='s' direction='in'/><arg type='u' direction='in'/>
<arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='s' direction='in'/>
<arg type='as' direction='in'/><arg type='a{sv}' direction='in'/><arg type='i' direction='in'/>
<arg type='u' direction='out'/></method></interface></node>"""
received = []


def on_call(_conn, _sender, _path, _iface, method, params, invocation):
    received.append(params.unpack())
    invocation.return_value(GLib.Variant("(u)", (len(received),)))


def load(script, name):
    """Charge une seule fonction du script, sans lancer Gtk ni main()."""
    tree = ast.parse((ROOT / script).read_text(encoding="utf-8"))
    func = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == name)
    namespace = {}
    exec("import shutil, subprocess\nfrom gi.repository import GLib\n", namespace)
    exec(compile(ast.Module(body=[func], type_ignores=[]), script, "exec"), namespace)
    return namespace[name]


def fail(message):
    print(f"ÉCHEC : {message}", file=sys.stderr)
    sys.exit(1)


def main():
    show = load("winlux-notify-icon.py", "show_notification")
    notify = load("winlux-desktop-paste.py", "notify")
    # Sans serveur : aucune exception, et pas de « balloon-show » renvoyé.
    if show("App", "dialog-information", "T", "X"):
        fail("bulle annoncée affichée alors qu'aucun serveur ne répond")

    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    bus.register_object("/org/freedesktop/Notifications", info, on_call, None, None)
    owned = []
    Gio.bus_own_name_on_connection(bus, "org.freedesktop.Notifications", Gio.BusNameOwnerFlags.NONE,
                                   lambda *_: owned.append(True), None)
    context = GLib.MainContext.default()
    while not owned:
        context.iteration(True)

    results = []
    import threading

    def client():
        results.append(show("7-Zip", "dialog-warning", "Titre bulle", "Texte bulle"))
        results.append(notify("Collé", "2 éléments"))
    worker = threading.Thread(target=client)
    worker.start()
    while worker.is_alive():
        context.iteration(False)
    while context.pending():
        context.iteration(False)

    if results != [True, True]:
        fail(f"appels non aboutis : {results}")
    if len(received) != 2:
        fail(f"{len(received)} notification(s) reçue(s) au lieu de 2")
    app, _, icon, title, body = received[0][:5]
    if (app, icon, title, body) != ("7-Zip", "dialog-warning", "Titre bulle", "Texte bulle"):
        fail(f"bulle mal transmise : {received[0]}")
    if received[1][2:5] != ("edit-paste", "Collé", "2 éléments"):
        fail(f"notification de collage mal transmise : {received[1]}")
    print("OK : bulles et collage notifiés par D-Bus sans notify-send")


if __name__ == "__main__":
    main()
