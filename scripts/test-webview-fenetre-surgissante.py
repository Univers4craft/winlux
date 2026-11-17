#!/usr/bin/python3
"""winlux-webview : taille d'une fenêtre surgissante ouverte par window.open (#2736).

Learn (ICoreWebView2WindowFeatures) : Width/Height et Left/Top sont ignorés
si HasSize/HasPosition valent FALSE. WebKitGTK rend pourtant toujours une
géométrie (celle de la fenêtre ouvrante, au moins 100x100) ; le moteur ne
doit la reprendre que si la page a nommé une taille ou une position.

1. popup_default_size : deux tiers de la zone de travail, au moins 640x480.
2. Si WebKitGTK est installé (sous Xvfb) : une vraie vue, dans une fenêtre de
   1x1 comme celle de Teams, ouvre quatre fenêtres :
   - sans options : taille par défaut, et non 100x100 ;
   - « width=300,height=200 » : 300x200 ;
   - « left=10,top=20 » : position demandée, taille par défaut ;
   - « popup » : taille par défaut.
"""
import importlib.util
import os
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("winlux_webview", HERE / "winlux-webview.py")
webview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(webview)
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


size = webview.popup_default_size
check(size((1920, 1039)) == (1280, 692), f"1920x1039 → {size((1920, 1039))}")
check(size((800, 600)) == (640, 480), f"800x600 → {size((800, 600))}")
check(size((1, 1)) == (640, 480), f"1x1 → {size((1, 1))}")

WebKit2 = None
try:
    if not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
        raise ImportError("pas d'affichage")
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("WebKit2", "4.1")
    from gi.repository import Gtk, WebKit2  # noqa: F401 - Gtk initialise l'affichage
except (ImportError, ValueError):
    WebKit2 = None
if WebKit2 is None:
    print("WebKitGTK ou affichage absent : partie moteur non vérifiée")
else:
    from gi.repository import Gdk, GdkX11, GLib, Gio
    webview.emit = lambda **event: None
    view = webview.View(Gtk, Gdk, GdkX11, GLib, Gio, WebKit2)
    view.create({"parent": 0, "x": 0, "y": 0, "w": 1, "h": 1, "visible": True})
    seen = {}
    ready = view.on_popup_ready

    def record(popup):
        ready(popup)
        # Les quatre fenêtres sont prêtes dans l'ordre des window.open.
        key = "abcd"[len(seen)] if len(seen) < 4 else str(len(seen))
        seen[key] = (popup.winlux_request, popup.get_toplevel().get_default_size())
        if len(seen) == 4:
            GLib.timeout_add(200, Gtk.main_quit)

    view.on_popup_ready = record
    view.view.load_html(
        "<script>"
        "window.open('about:blank?a');"
        "window.open('about:blank?b', '', 'width=300,height=200');"
        "window.open('about:blank?c', '', 'left=10,top=20');"
        "window.open('about:blank?d', 'x', 'popup');"
        "</script>", "http://localhost/")
    GLib.timeout_add_seconds(20, Gtk.main_quit)
    Gtk.main()
    default = size(view.workarea())
    check(len(seen) == 4, f"fenêtres surgissantes prêtes : {sorted(seen)}")

    def dims(key):
        got = seen.get(key, (None, None))[1]
        return (got.width, got.height) if got is not None else None

    check(seen.get("a", (None,))[0] == (False, False), f"sans options : relevé {seen.get('a')}")
    check(dims("a") == default, f"sans options : {dims('a')} au lieu de {default}")
    check(dims("b") == (300, 200), f"width=300,height=200 : {dims('b')}")
    check(seen.get("c", (None,))[0] == (False, True), f"left/top : relevé {seen.get('c')}")
    check(dims("c") == default, f"left/top seuls : {dims('c')} au lieu de {default}")
    check(dims("d") == default, f"popup : {dims('d')} au lieu de {default}")

if failures:
    sys.exit(1)
print("window.open : taille et position reprises seulement si la page les demande (#2736)")
