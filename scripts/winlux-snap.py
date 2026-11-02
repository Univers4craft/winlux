#!/usr/bin/python3
"""Ancrage des fenêtres comme Windows 11 (session native, openbox).

Windows + flèches suit la logique de Windows :
- ← / → : moitié gauche / droite ; depuis la moitié opposée, la fenêtre
  reprend sa taille d'avant ;
- ↑ : depuis une moitié, quart du haut ; depuis un quart du bas, moitié ;
  sinon agrandie ;
- ↓ : depuis une moitié, quart du bas ; depuis un quart du haut, moitié ;
  agrandie → taille d'avant ; sinon réduite.
Après un ancrage en moitié, l'Assistant d'ancrage propose les autres fenêtres
pour remplir l'autre moitié. Windows + Z (ou clic droit sur le bouton
Agrandir) ouvre les dispositions : moitiés, deux tiers / un tiers, trois
colonnes, quatre quarts.

    winlux-snap key left|right|up|down   (Windows + flèche)
    winlux-snap layouts                  (Windows + Z)
    winlux-snap place ZONE [FENÊTRE]     place la fenêtre active (ou FENÊTRE) dans ZONE
    winlux-snap zones                    zones connues (tests)
    winlux-snap ontop [FENÊTRE]          Windows + Ctrl + T : toujours au premier plan

Une zone est « disposition:case », par exemple half:left, third:2, quarter:tr.
L'état d'ancrage et la taille d'avant sont gardés dans des propriétés X de la
fenêtre (_WINLUX_SNAP, _WINLUX_RESTORE).
"""
import os
import re
import shutil
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
    'Dispositions d’ancrage': ('Snap layouts', 'Diseños de acoplamiento', 'Andocklayouts'),
    'Choisissez une fenêtre pour l’autre côté': ('Choose a window for the other side', 'Elija una ventana para el otro lado', 'Fenster für die andere Seite auswählen'),
    'Échap pour fermer': ('Esc to close', 'Esc para cerrar', 'Esc zum Schließen'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# Zones en fractions de la zone de travail (x, y, largeur, hauteur).
ZONES = {
    "half:left": (0, 0, 1 / 2, 1), "half:right": (1 / 2, 0, 1 / 2, 1),
    "wide:left": (0, 0, 2 / 3, 1), "wide:right": (2 / 3, 0, 1 / 3, 1),
    "narrow:left": (0, 0, 1 / 3, 1), "narrow:right": (1 / 3, 0, 2 / 3, 1),
    "third:1": (0, 0, 1 / 3, 1), "third:2": (1 / 3, 0, 1 / 3, 1), "third:3": (2 / 3, 0, 1 / 3, 1),
    "quarter:tl": (0, 0, 1 / 2, 1 / 2), "quarter:tr": (1 / 2, 0, 1 / 2, 1 / 2),
    "quarter:bl": (0, 1 / 2, 1 / 2, 1 / 2), "quarter:br": (1 / 2, 1 / 2, 1 / 2, 1 / 2),
}
# Dispositions de Windows + Z, chacune avec ses cases.
LAYOUTS = (("half:left", "half:right"), ("wide:left", "wide:right"), ("third:1", "third:2", "third:3"),
           ("narrow:left", "narrow:right"), ("quarter:tl", "quarter:tr", "quarter:bl", "quarter:br"))
# Case restante proposée par l'Assistant d'ancrage.
OPPOSITE = {"half:left": "half:right", "half:right": "half:left", "wide:left": "wide:right",
            "wide:right": "wide:left", "narrow:left": "narrow:right", "narrow:right": "narrow:left"}


def next_state(state, key, maximized=False):
    """(action, zone) pour Windows + flèche depuis l'état d'ancrage actuel.

    action : « snap » (zone), « restore », « maximize » ou « minimize »."""
    if key == "left":
        if state in ("half:right", "quarter:tr", "quarter:br"): return "restore", None
        return "snap", "half:left"
    if key == "right":
        if state in ("half:left", "quarter:tl", "quarter:bl"): return "restore", None
        return "snap", "half:right"
    if key == "up":
        if state == "half:left": return "snap", "quarter:tl"
        if state == "half:right": return "snap", "quarter:tr"
        if state == "quarter:bl": return "snap", "half:left"
        if state == "quarter:br": return "snap", "half:right"
        if state in ("quarter:tl", "quarter:tr"): return "none", None
        return "maximize", None
    if key == "down":
        if state == "half:left": return "snap", "quarter:bl"
        if state == "half:right": return "snap", "quarter:br"
        if state == "quarter:tl": return "snap", "half:left"
        if state == "quarter:tr": return "snap", "half:right"
        if state in ("quarter:bl", "quarter:br"): return "none", None
        if maximized: return "restore", None
        return "minimize", None
    raise ValueError(key)


def zone_rect(zone, area):
    """Rectangle (x, y, w, h) d'une zone dans la zone de travail."""
    fx, fy, fw, fh = ZONES[zone]
    ax, ay, aw, ah = area
    x, y = ax + round(fx * aw), ay + round(fy * ah)
    right, bottom = ax + round((fx + fw) * aw), ay + round((fy + fh) * ah)
    return x, y, right - x, bottom - y


def client_geometry(rect, extents):
    """Géométrie à demander pour que le cadre (bordures comprises) remplisse rect."""
    left, right, top, bottom = extents
    x, y, w, h = rect
    return x, y, max(40, w - left - right), max(40, h - top - bottom)


def pick_area(areas, point):
    """Zone de travail de l'écran qui contient le point (sinon la première)."""
    px, py = point
    for area in areas:
        x, y, w, h = area
        if x <= px < x + w and y <= py < y + h: return area
    return areas[0]


# ── Accès X (wmctrl, xdotool, xprop, xwininfo, xrandr) ────────────────────────────

def _run(args, timeout=3):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return ""
    return result.stdout if result.returncode == 0 else ""


def window_number(window):
    """Identifiant X en nombre (xdotool l'écrit en décimal, wmctrl en hexadécimal)."""
    try: return int(str(window), 0)
    except (TypeError, ValueError): return 0


def active_window():
    return _run(["xdotool", "getactivewindow"]).strip()


def window_geometry(window):
    """(x, y, w, h) du cadre complet, d'après xwininfo et _NET_FRAME_EXTENTS.

    (xdotool getwindowgeometry compte deux fois la décoration d'openbox.)"""
    out = _run(["xwininfo", "-id", str(window)])
    values = {}
    for label, key in (("Absolute upper-left X", "x"), ("Absolute upper-left Y", "y"), ("Width", "w"), ("Height", "h")):
        match = re.search(rf"^\s*{label}:\s*(-?\d+)", out, re.M)
        if match: values[key] = int(match.group(1))
    if len(values) < 4: return None
    left, right, top, bottom = frame_extents(window)
    return values["x"] - left, values["y"] - top, values["w"] + left + right, values["h"] + top + bottom


def frame_extents(window):
    out = _run(["xprop", "-id", window, "_NET_FRAME_EXTENTS"])
    numbers = [int(n) for n in re.findall(r"\d+", out.split("=", 1)[-1])] if "=" in out else []
    return tuple(numbers[:4]) if len(numbers) >= 4 else (0, 0, 0, 0)


def get_prop(window, name):
    out = _run(["xprop", "-id", window, name])
    match = re.search(r'= "(.*)"', out)
    return match.group(1) if match else ""


def set_prop(window, name, value):
    _run(["xprop", "-id", window, "-f", name, "8s", "-set", name, value])


def is_maximized(window):
    out = _run(["xprop", "-id", window, "_NET_WM_STATE"])
    return "_NET_WM_STATE_MAXIMIZED_VERT" in out and "_NET_WM_STATE_MAXIMIZED_HORZ" in out


def work_areas():
    """Zone de travail de chaque écran : écran ∩ _NET_WORKAREA (barre des tâches exclue)."""
    numbers = [int(n) for n in re.findall(r"-?\d+", _run(["xprop", "-root", "_NET_WORKAREA"]).split("=", 1)[-1])]
    workarea = tuple(numbers[:4]) if len(numbers) >= 4 else None
    monitors = []
    for line in _run(["xrandr", "--listmonitors"]).splitlines()[1:]:
        match = re.search(r"(\d+)/\d+x(\d+)/\d+\+(-?\d+)\+(-?\d+)", line)
        if match:
            w, h, x, y = (int(group) for group in match.groups())
            monitors.append((x, y, w, h))
    if not monitors:
        return [workarea] if workarea else [(0, 0, 1920, 1080)]
    if workarea is None: return monitors
    areas = []
    for mx, my, mw, mh in monitors:
        wx, wy, ww, wh = workarea
        x, y = max(mx, wx), max(my, wy)
        right, bottom = min(mx + mw, wx + ww), min(my + mh, wy + wh)
        areas.append((x, y, right - x, bottom - y) if right > x and bottom > y else (mx, my, mw, mh))
    return areas


def area_of(window):
    geometry = window_geometry(window)
    if geometry is None: return work_areas()[0]
    x, y, w, h = geometry
    return pick_area(work_areas(), (x + w // 2, y + h // 2))


def snap(window, zone, area=None):
    area = area or area_of(window)
    state = get_prop(window, "_WINLUX_SNAP")
    if not state and not is_maximized(window):
        geometry = window_geometry(window)
        if geometry: set_prop(window, "_WINLUX_RESTORE", ",".join(map(str, geometry)))
    _run(["wmctrl", "-i", "-r", window, "-b", "remove,maximized_vert,maximized_horz"])
    x, y, w, h = client_geometry(zone_rect(zone, area), frame_extents(window))
    _run(["wmctrl", "-i", "-r", window, "-e", f"1,{x},{y},{w},{h}"])
    set_prop(window, "_WINLUX_SNAP", zone)


def restore(window):
    _run(["wmctrl", "-i", "-r", window, "-b", "remove,maximized_vert,maximized_horz"])
    saved = get_prop(window, "_WINLUX_RESTORE")
    try:
        x, y, w, h = (int(part) for part in saved.split(","))
    except ValueError:
        saved = ""
    if saved:
        x, y, w, h = client_geometry((x, y, w, h), frame_extents(window))
        _run(["wmctrl", "-i", "-r", window, "-e", f"1,{x},{y},{w},{h}"])
    set_prop(window, "_WINLUX_SNAP", "")


def other_windows(exclude, desktop=None):
    """Fenêtres normales du bureau virtuel courant : [(id, titre)]."""
    if desktop is None:
        current = re.search(r"^(\d+)\s+\*", _run(["wmctrl", "-d"]), re.M)
        desktop = current.group(1) if current else "0"
    found = []
    for line in _run(["wmctrl", "-l"]).splitlines():
        parts = line.split(None, 3)
        if len(parts) < 3 or parts[1] not in (desktop, "-1"): continue
        window = parts[0]
        if window_number(window) == window_number(exclude): continue
        kind = _run(["xprop", "-id", window, "_NET_WM_WINDOW_TYPE"])
        if "NORMAL" not in kind and "_NET_WM_WINDOW_TYPE" in kind and "not found" not in kind: continue
        if "_NET_WM_STATE_HIDDEN" in _run(["xprop", "-id", window, "_NET_WM_STATE"]): continue
        title = parts[3] if len(parts) > 3 else ""
        if title in ("xfce4-panel", "Bureau", "Desktop", "xfdesktop"): continue
        found.append((window, title))
    return found


# ── Toujours au premier plan (Windows + Ctrl + T) ──────────────────────────

ONTOP_BORDER = 3          # épaisseur du cadre coloré, en pixels
ONTOP_COLOR = "#8178b7"   # couleur d'accent de Winlux


def is_above(window):
    return "_NET_WM_STATE_ABOVE" in _run(["xprop", "-id", str(window), "_NET_WM_STATE"])


def border_rects(geometry, size=ONTOP_BORDER):
    """Quatre bandes (x, y, l, h) qui entourent le cadre de la fenêtre."""
    x, y, w, h = geometry
    return [(x - size, y - size, w + 2 * size, size), (x - size, y + h, w + 2 * size, size),
            (x - size, y, size, h), (x + w, y, size, h)]


def ontop(window):
    """Bascule « toujours au premier plan » et dessine un cadre coloré tant qu'il dure."""
    if not window: return 1
    hexa = "0x%x" % window_number(window)
    _run(["wmctrl", "-i", "-r", hexa, "-b", "toggle,above"])
    above = is_above(window)
    sound = "/usr/share/winlux/sounds/notification.wav"
    if os.path.exists(sound) and shutil.which("paplay"):
        subprocess.Popen(["paplay", sound], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not above or os.environ.get("WINLUX_ONTOP_BORDER") == "0":
        return 0
    if os.fork():           # le raccourci rend la main ; le cadre vit à part
        return 0
    os.setsid()
    return ontop_border(window)


def ontop_border(window):
    Gdk, GLib, Gtk = _gtk()
    css = Gtk.CssProvider()
    css.load_from_data(f"window.winlux-ontop {{ background: {ONTOP_COLOR}; }}".encode())
    bands = []
    for _ in range(4):
        band = Gtk.Window(type=Gtk.WindowType.POPUP)
        band.get_style_context().add_class("winlux-ontop")
        band.set_accept_focus(False)
        bands.append(band)
    Gtk.StyleContext.add_provider_for_screen(bands[0].get_screen(), css, 600)
    state = {"last": None}

    def follow():
        if not is_above(window):
            Gtk.main_quit()
            return False
        geometry = window_geometry(window)
        if geometry is None:
            Gtk.main_quit()
            return False
        hidden = "_NET_WM_STATE_HIDDEN" in _run(["xprop", "-id", str(window), "_NET_WM_STATE"])
        if (geometry, hidden) != state["last"]:
            state["last"] = (geometry, hidden)
            for band, (x, y, w, h) in zip(bands, border_rects(geometry)):
                if hidden:
                    band.hide()
                    continue
                band.move(x, y)
                band.resize(max(1, w), max(1, h))
                band.show()
        return True

    if follow():
        GLib.timeout_add(250, follow)
        Gtk.main()
    return 0


# ── Interfaces (GTK) ────────────────────────────────────────────────────────

def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    return Gdk, GLib, Gtk


CSS = b"""
  window { background: #202430; color: #ecedf5; border: 1px solid #3c4458; border-radius: 8px; }
  .heading { font-weight: bold; }
  .subtitle { color: #adb6c9; font-size: 12px; }
  button.zone { background: #3a4254; border: 1px solid #56607a; border-radius: 4px; min-width: 10px; min-height: 10px; padding: 0; }
  button.zone:hover { background: #8178b7; border-color: #b8b0ff; }
  .layout { background: #2a3040; border-radius: 6px; padding: 6px; }
  button.window { background: #2a3040; border: 1px solid #48536b; border-radius: 6px; padding: 10px; color: #ecedf5; }
  button.window:hover { background: #343b4e; border-color: #8178b7; }
"""


def layouts_popup(window):
    """Windows + Z : grilles de disposition, comme au survol du bouton Agrandir."""
    Gdk, GLib, Gtk = _gtk()
    if not window: return 1
    popup = Gtk.Window(title=_t("Dispositions d’ancrage"))
    popup.set_role("winlux-snap-layouts")
    popup.set_decorated(False)
    popup.set_keep_above(True)
    popup.set_skip_taskbar_hint(True)
    css = Gtk.CssProvider()
    css.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(popup.get_screen(), css, 600)
    flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=3, min_children_per_line=3,
                       row_spacing=8, column_spacing=8, margin=10)
    popup.add(flow)
    cell_w, cell_h = 96, 60
    chosen = {}

    def choose(zone):
        chosen["zone"] = zone
        popup.destroy()
        Gtk.main_quit()

    for layout in LAYOUTS:
        fixed = Gtk.Fixed()
        fixed.get_style_context().add_class("layout")
        fixed.set_size_request(cell_w + 4, cell_h + 4)
        for zone in layout:
            fx, fy, fw, fh = ZONES[zone]
            button = Gtk.Button()
            button.get_style_context().add_class("zone")
            button.set_size_request(max(8, round(fw * cell_w) - 3), max(8, round(fh * cell_h) - 3))
            button.connect("clicked", lambda _, z=zone: choose(z))
            fixed.put(button, 2 + round(fx * cell_w), 2 + round(fy * cell_h))
        flow.add(fixed)
    popup.connect("key-press-event", lambda _, event: event.keyval == Gdk.KEY_Escape and (Gtk.main_quit() or True))
    popup.connect("focus-out-event", lambda *_: GLib.timeout_add(200, lambda: popup.is_active() or Gtk.main_quit()) and False)
    # En haut au centre de la fenêtre, là où Windows montre les dispositions.
    geometry = window_geometry(window)
    popup.show_all()
    if geometry:
        x, y, w, _ = geometry
        width, _ = popup.get_size()
        popup.move(max(0, x + w - width - 60), max(0, y + 30))
    popup.present()
    Gtk.main()
    if "zone" not in chosen: return 0
    zone = chosen["zone"]
    snap(window, zone)
    _run(["xdotool", "windowactivate", window])
    layout = next(layout for layout in LAYOUTS if zone in layout)
    free = [cell for cell in layout if cell != zone]
    if free: return assist(window, free)
    return 0


def assist(window, free_zones):
    """Assistant d'ancrage : propose les autres fenêtres pour les cases restantes."""
    Gdk, GLib, Gtk = _gtk()
    area = area_of(window)
    used = [window]
    for zone in free_zones:
        taken = {window_number(item) for item in used}
        candidates = [item for item in other_windows(None) if window_number(item[0]) not in taken]
        if not candidates: return 0
        picked = {}
        popup = Gtk.Window(title=_t("Choisissez une fenêtre pour l’autre côté"))
        popup.set_role("winlux-snap-assist")
        popup.set_decorated(False)
        popup.set_keep_above(True)
        popup.set_skip_taskbar_hint(True)
        css = Gtk.CssProvider()
        css.load_from_data(CSS)
        Gtk.StyleContext.add_provider_for_screen(popup.get_screen(), css, 600)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=14)
        popup.add(box)
        title = Gtk.Label(label=_t("Choisissez une fenêtre pour l’autre côté"), xalign=0)
        title.get_style_context().add_class("heading")
        box.pack_start(title, False, False, 0)
        hint = Gtk.Label(label=_t("Échap pour fermer"), xalign=0)
        hint.get_style_context().add_class("subtitle")
        box.pack_start(hint, False, False, 0)
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=2, row_spacing=8, column_spacing=8)
        box.pack_start(flow, True, True, 0)
        for candidate, name in candidates[:12]:
            button = Gtk.Button(label=name[:60] or candidate)
            button.get_style_context().add_class("window")
            button.connect("clicked", lambda _, c=candidate: (picked.setdefault("window", c), Gtk.main_quit()))
            flow.add(button)
        popup.connect("key-press-event", lambda _, event: event.keyval == Gdk.KEY_Escape and (Gtk.main_quit() or True))
        x, y, w, h = zone_rect(zone, area)
        popup.set_default_size(min(w - 40, 640), -1)
        popup.show_all()
        width, height = popup.get_size()
        popup.move(x + max(0, (w - width) // 2), y + max(0, (h - height) // 2))
        popup.present()
        Gtk.main()
        popup.destroy()
        if "window" not in picked: return 0
        snap(picked["window"], zone, area)
        _run(["wmctrl", "-i", "-a", picked["window"]])
        used.append(picked["window"])
    return 0


def key(direction):
    window = active_window()
    if not window: return 1
    state = get_prop(window, "_WINLUX_SNAP")
    action, zone = next_state(state, direction, is_maximized(window))
    if action == "snap":
        snap(window, zone)
        if zone in OPPOSITE and os.environ.get("WINLUX_SNAP_ASSIST", "1") != "0":
            return assist(window, [OPPOSITE[zone]])
    elif action == "restore":
        restore(window)
    elif action == "maximize":
        if not state and not is_maximized(window):
            geometry = window_geometry(window)
            if geometry: set_prop(window, "_WINLUX_RESTORE", ",".join(map(str, geometry)))
        _run(["wmctrl", "-i", "-r", window, "-b", "add,maximized_vert,maximized_horz"])
        set_prop(window, "_WINLUX_SNAP", "")
    elif action == "minimize":
        _run(["xdotool", "windowminimize", window])
    return 0


def main(argv):
    if len(argv) == 3 and argv[1] == "key" and argv[2] in ("left", "right", "up", "down"):
        return key(argv[2])
    if len(argv) == 2 and argv[1] == "layouts":
        return layouts_popup(active_window())
    if len(argv) in (3, 4) and argv[1] == "place" and argv[2] in ZONES:
        window = argv[3] if len(argv) == 4 else active_window()
        if not window: return 1
        snap(window, argv[2])
        return 0
    if len(argv) in (2, 3) and argv[1] == "ontop":
        return ontop(argv[2] if len(argv) == 3 else active_window())
    if len(argv) == 2 and argv[1] == "zones":
        print("\n".join(ZONES))
        return 0
    print(__doc__.strip().split("\n\n")[-3], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
