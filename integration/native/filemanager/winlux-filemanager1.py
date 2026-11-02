#!/usr/bin/python3
"""org.freedesktop.FileManager1 de la session : l'Explorateur Winlux.

Firefox, Chrome, Edge, les applications GTK et Qt passent par ce service
pour « Afficher dans le dossier » et « Ouvrir le dossier ». Sous Ubuntu, il
était fourni par Thunar, qui s'ouvrait à la place de l'Explorateur. Ce service
(activé par D-Bus, voir integration/native/dbus) ouvre l'Explorateur sur le
dossier demandé :

- ShowItems : le dossier qui contient chaque élément ;
- ShowFolders : le dossier lui-même ;
- ShowItemProperties : le dossier qui contient l'élément.

Il quitte après une minute sans demande. WINLUX_EXPLORER remplace
/usr/bin/winlux-shell-app (tests).
"""
import os
from pathlib import Path
import subprocess
import sys
from urllib.parse import unquote, urlparse

NAME = "org.freedesktop.FileManager1"
OBJECT = "/org/freedesktop/FileManager1"
XML = """<node><interface name="org.freedesktop.FileManager1">
<method name="ShowFolders"><arg type="as" name="URIs" direction="in"/><arg type="s" name="StartupId" direction="in"/></method>
<method name="ShowItems"><arg type="as" name="URIs" direction="in"/><arg type="s" name="StartupId" direction="in"/></method>
<method name="ShowItemProperties"><arg type="as" name="URIs" direction="in"/><arg type="s" name="StartupId" direction="in"/></method>
</interface></node>"""
LIMIT = 16  # au plus 16 fenêtres par demande


def uri_path(uri):
    """URI file:// ou chemin absolu → chemin local, sinon None."""
    if uri.startswith("/"):
        return Path(uri)
    parsed = urlparse(uri)
    if parsed.scheme != "file" or parsed.netloc not in ("", "localhost"):
        return None
    return Path(unquote(parsed.path))


def folders(method, uris):
    """Dossiers à ouvrir pour une demande, sans doublon, dans l'ordre."""
    result = []
    for uri in uris[:LIMIT]:
        path = uri_path(uri)
        if path is None:
            continue
        path = Path(os.path.normpath(path))
        folder = path if method == "ShowFolders" and path.is_dir() else path.parent
        # Élément supprimé ou dossier déplacé : le premier parent qui existe.
        while not folder.is_dir() and folder != folder.parent:
            folder = folder.parent
        if folder not in result:
            result.append(folder)
    return result


def open_explorer(folder, startup=""):
    env = dict(os.environ)
    if startup:
        env["DESKTOP_STARTUP_ID"] = startup
    subprocess.Popen([os.environ.get("WINLUX_EXPLORER", "/usr/bin/winlux-shell-app"), "explorer", str(folder)],
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=True, env=env)


def serve(idle_seconds=60):
    from gi.repository import Gio, GLib

    loop = GLib.MainLoop()
    idle = {"source": 0}

    def quit_later():
        if idle["source"]:
            GLib.source_remove(idle["source"])
        idle["source"] = GLib.timeout_add_seconds(idle_seconds, loop.quit)

    def call(_connection, _sender, _path, _interface, method, parameters, invocation):
        uris, startup = parameters.unpack()
        for folder in folders(method, list(uris)):
            open_explorer(folder, startup)
        invocation.return_value(None)
        quit_later()

    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]

    def acquired(connection, _name):
        connection.register_object(OBJECT, info, call, None, None)

    Gio.bus_own_name(Gio.BusType.SESSION, NAME, Gio.BusNameOwnerFlags.NONE, acquired, None, lambda *_: loop.quit())
    quit_later()
    loop.run()
    return 0


if __name__ == "__main__":
    sys.exit(serve())
