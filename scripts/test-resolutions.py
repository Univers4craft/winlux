#!/usr/bin/python3
"""Bureau en 1080p, 1440p et 4K (#29) : menu Démarrer, réglages rapides et
Paramètres tiennent dans l'écran, à leur place, avec l'échelle de la session.

À lancer dans un Xvfb dédié par résolution, par exemple :

    xvfb-run -a -s '-screen 0 3840x2160x24' python3 scripts/test-resolutions.py build/captures

L'échelle suit la règle de la session (winlux-user-session.sh) : facteur
entier pour GTK (GDK_SCALE), le reste par la densité des polices
(GDK_DPI_SCALE). Une capture par écran est écrite dans le dossier donné.
"""
import os
from pathlib import Path
import runpy
import subprocess
import sys

HERE = Path(__file__).resolve().parent


def session_scale(height):
    """Échelle de Windows selon la hauteur, sans EDID (Xvfb) : 100 %, 150 %, 200 %."""
    return 2.0 if height >= 2000 else 1.5 if height >= 1440 else 1.0


def gtk_factors(scale):
    """(GDK_SCALE entier, complément pour les polices), comme la session."""
    factor = 2 if scale >= 2 else 1
    return factor, scale / factor


def child(kind, output):
    """Ouvre un écran, vérifie sa place, le photographie, puis quitte."""
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    failures = []
    if kind == "start":
        module = runpy.run_path(str(HERE / "winlux-start-menu.py"), run_name="resolution_test")
        app = module["Start"]()
    elif kind == "quick":
        module = runpy.run_path(str(HERE / "winlux-popup.py"), run_name="resolution_test")
        app = module["Popup"]("quick")
    else:
        module = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="resolution_test")
        app = None
        window = module["Settings"]()
        window.show_all()

    def check():
        try:
            win = app.window if app else window
            assert win is not None and win.get_visible(), "fenêtre non affichée"
            area = win.get_display().get_monitor(0).get_geometry()
            x, y = win.get_position()
            width, height = win.get_size()
            assert width <= area.width and height <= area.height, (kind, width, height, area.width, area.height)
            if kind != "settings":
                # Menus et panneaux : entiers dans l'écran, posés au-dessus de la barre.
                assert 0 <= x and x + width <= area.width, (kind, x, width, area.width)
                assert 0 <= y and y + height <= area.height, (kind, y, height, area.height)
                assert y + height >= area.height - 120 * win.get_scale_factor(), ("loin de la barre", kind, y, height)
            if kind == "start":
                center = x + width / 2
                left = x < area.width * 0.1
                assert left or abs(center - area.width / 2) < area.width * 0.05, ("menu ni centré ni à gauche", x, width)
            if kind == "settings":
                # Assez grand pour lire : au moins 60 % de la largeur logique utile en 1080p.
                assert width >= min(area.width * 0.4, 720), (width, area.width)
            root = Gdk.get_default_root_window()
            pixbuf = Gdk.pixbuf_get_from_window(root, 0, 0, root.get_width(), root.get_height())
            assert pixbuf is not None, "capture impossible"
            data = pixbuf.get_pixels()
            assert len(set(data[i] for i in range(0, len(data), 997))) > 8, "écran uniforme"
            pixbuf.savev(output, "png", [], [])
        except Exception as error:  # noqa: BLE001 - remonté au parent
            failures.append(repr(error))
        if app:
            app.quit()
        else:
            Gtk.main_quit()
        return False

    GLib.timeout_add(1500, check)
    if app:
        app.run([])
    else:
        Gtk.main()
    if failures:
        print("ÉCHEC", kind, failures[0], file=sys.stderr)
        sys.exit(1)


def main():
    if len(sys.argv) >= 4 and sys.argv[1] == "--one":
        child(sys.argv[2], sys.argv[3])
        return
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "build/captures")
    out.mkdir(parents=True, exist_ok=True)
    import gi
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk
    root = Gdk.get_default_root_window()
    width, height = root.get_width(), root.get_height()
    scale = session_scale(height)
    factor, dpi = gtk_factors(scale)
    env = {**os.environ, "GDK_SCALE": str(factor), "GDK_DPI_SCALE": f"{dpi:g}", "WINLUX_UI_SCALE": f"{scale:g}"}
    failed = []
    for kind in ("start", "quick", "settings"):
        png = out / f"resolution-{height}p-{kind}.png"
        result = subprocess.run([sys.executable, "-B", __file__, "--one", kind, str(png)], env=env, timeout=120,
                                capture_output=True, text=True)
        if result.returncode != 0:
            failed.append(f"{kind} : {result.stderr.strip()[-400:]}")
    if failed:
        print(f"ÉCHEC {width}x{height} :\n" + "\n".join(failed), file=sys.stderr)
        sys.exit(1)
    print(f"OK: {width}x{height}, échelle {scale:g} (GDK_SCALE={factor}), menu Démarrer, réglages rapides, Paramètres")


if __name__ == "__main__":
    main()
