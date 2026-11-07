#!/usr/bin/python3
"""Ancrage des fenêtres comme Windows 11 (session native, openbox).

Windows + flèches suit la logique de Windows :
- ← / → : moitié gauche / droite ; depuis la moitié opposée, la fenêtre
  reprend sa taille d'avant ;
- ↑ : depuis une moitié, quart du haut ; depuis un quart du bas, moitié ;
  sinon agrandie ;
- ↓ : depuis une moitié, quart du bas ; depuis un quart du haut, moitié ;
  agrandie → taille d'avant ; sinon réduite.
Après un ancrage (moitié, quart, case d'une disposition), l'Assistant
d'ancrage remplit la place encore libre avec les autres fenêtres, en cartes
(aperçu ou icône, titre) : un clic ou Entrée la place, Échap ferme. Une case
déjà occupée par une fenêtre ancrée n'est pas proposée. Les fenêtres ainsi
placées forment un groupe d'ancrage : survoler le bouton de l'une d'elles
dans la barre des tâches montre le groupe, un clic le remet au premier plan.
Windows + Z (ou clic droit sur le bouton Agrandir) ouvre les dispositions : moitiés, deux tiers / un tiers, trois
colonnes, quatre quarts.

    winlux-snap key left|right|up|down   (Windows + flèche)
    winlux-snap layouts                  (Windows + Z)
    winlux-snap place ZONE [FENÊTRE]     place la fenêtre active (ou FENÊTRE) dans ZONE
    winlux-snap zones                    zones connues (tests)
    winlux-snap ontop [FENÊTRE]          Windows + Ctrl + T : toujours au premier plan
    winlux-snap watch                    ancrage à la souris (lancé avec la session)
    winlux-snap restore-group FENÊTRE    remet le groupe d'ancrage de FENÊTRE au premier plan

À la souris (winlux-snap watch) : une fenêtre glissée par sa barre de titre
jusqu'au bord gauche ou droit prend la moitié de l'écran, jusqu'à un coin un
quart, jusqu'en haut tout l'écran ; près du haut, au centre, la barre des
dispositions descend et l'on y lâche la fenêtre dans une case. Une fenêtre
ancrée qu'on décroche reprend sa taille d'avant. Laisser la souris sur le
bouton Agrandir ouvre la grille des dispositions sous le bouton.

Une zone est « disposition:case », par exemple half:left, third:2, quarter:tr.
L'état d'ancrage et la taille d'avant sont gardés dans des propriétés X de la
fenêtre (_WINLUX_SNAP, _WINLUX_RESTORE).
"""
import ctypes
import ctypes.util
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
    'Choisissez une fenêtre pour cet emplacement': ('Choose a window for this space', 'Elija una ventana para este espacio', 'Fenster für diesen Bereich auswählen'),
    'Emplacement {n} sur {total} · Échap pour fermer': ('Space {n} of {total} · Esc to close', 'Espacio {n} de {total} · Esc para cerrar', 'Bereich {n} von {total} · Esc zum Schließen'),
    'Sans titre': ('Untitled', 'Sin título', 'Ohne Titel'),
    'Groupe d’ancrage': ('Snap group', 'Grupo de acoplamiento', 'Andockgruppe'),
    'Cliquez pour remettre le groupe au premier plan': ('Click to bring the group back', 'Haga clic para restaurar el grupo', 'Klicken, um die Gruppe wiederherzustellen'),
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
QUARTERS = ("quarter:tl", "quarter:tr", "quarter:bl", "quarter:br")


def following_zones(zone):
    """Cases que l'Assistant d'ancrage propose après « zone », dans l'ordre :
    l'autre côté d'une moitié (ou d'un deux tiers / un tiers), les autres
    quarts après un coin, les autres cases de la disposition sinon."""
    if zone in OPPOSITE: return [OPPOSITE[zone]]
    if zone in QUARTERS: return [quarter for quarter in QUARTERS if quarter != zone]
    layout = next((layout for layout in LAYOUTS if zone in layout), ())
    return [cell for cell in layout if cell != zone]


def overlap(a, b):
    ax, ay, aw, ah = a
    bx, by, bw, bh = b
    w, h = min(ax + aw, bx + bw) - max(ax, bx), min(ay + ah, by + bh) - max(ay, by)
    return w * h if w > 0 and h > 0 else 0


def free_zones(zones, area, taken):
    """Cases encore libres : une case couverte (aux quatre cinquièmes) par une
    fenêtre déjà ancrée n'est plus proposée, comme sous Windows."""
    free = []
    for zone in zones:
        rect = zone_rect(zone, area)
        size = max(1, rect[2] * rect[3])
        if any(overlap(rect, other) >= 0.8 * size for other in taken): continue
        free.append(zone)
    return free


def card_grid(width, height, scale=1, count=1):
    """(colonnes, largeur, hauteur) des cartes de l'assistant, en pixels GTK,
    pour une case de width x height pixels réels : lisible du 4K (échelle 2)
    au 1366x768, où un quart ne fait que 683x364."""
    width, height = width / max(1, scale), height / max(1, scale)
    inner = max(120, width - 2 * 24)
    columns = max(1, min(4, count, int((inner + 16) // (180 + 16))))
    card_w = int(min(300, (inner - 16 * (columns - 1)) / columns))
    card_h = int(max(90, min(card_w * 0.66, height - 120)))
    return columns, card_w, card_h


def parse_icons(values):
    """_NET_WM_ICON (largeur, hauteur, pixels ARGB, …) → [(l, h, pixels)]."""
    icons, index = [], 0
    while index + 2 <= len(values):
        w, h = values[index], values[index + 1]
        if w <= 0 or h <= 0 or w > 1024 or h > 1024 or index + 2 + w * h > len(values): break
        icons.append((w, h, values[index + 2:index + 2 + w * h]))
        index += 2 + w * h
    return icons


def best_icon(icons, size):
    """La plus petite icône au moins aussi grande que size, sinon la plus grande."""
    if not icons: return None
    big = [icon for icon in icons if icon[0] >= size]
    return min(big, key=lambda icon: icon[0]) if big else max(icons, key=lambda icon: icon[0])


def unobscured(window, stacking, geometries):
    """Vrai si aucune fenêtre au-dessus ne cache window : sans compositeur,
    l'aperçu d'une fenêtre cachée montrerait celle de devant."""
    if window not in stacking or window not in geometries: return False
    mine = geometries[window]
    above = stacking[stacking.index(window) + 1:]
    return not any(other in geometries and overlap(mine, geometries[other]) > 64 for other in above)


def order_candidates(candidates, snapped):
    """Fenêtres pas encore ancrées d'abord (ordre gardé), puis les ancrées."""
    return [c for c in candidates if c[0] not in snapped] + [c for c in candidates if c[0] in snapped]


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
    # Changer de case quitte le groupe d'ancrage ; la même case (groupe restauré) le garde.
    if state != zone: set_prop(window, "_WINLUX_SNAP_GROUP", "")
    set_prop(window, "_WINLUX_SNAP", zone)


def leave_snap(window):
    """Fenêtre décrochée, agrandie ou remise à sa taille : plus ancrée ni groupée."""
    set_prop(window, "_WINLUX_SNAP", "")
    set_prop(window, "_WINLUX_SNAP_GROUP", "")


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
    leave_snap(window)


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
ONTOP_COLOR = "#6a3df0"   # couleur d'accent de Winlux


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
  button.zone:hover { background: #6a3df0; border-color: #b8b0ff; }
  .layout { background: #2a3040; border-radius: 6px; padding: 6px; }
  button.window { background: #2a3040; border: 1px solid #48536b; border-radius: 6px; padding: 10px; color: #ecedf5; }
  button.window:hover { background: #343b4e; border-color: #b69cff; }
  window.assist { background: rgba(24, 27, 38, 0.94); border: 1px solid #56607a; border-radius: 8px; }
  window.assist .heading { font-size: 15px; }
  flowboxchild { padding: 0; border-radius: 8px; border: 2px solid transparent; }
  flowboxchild:selected, flowboxchild:focus { background: transparent; border-color: #b69cff; }
  .card { background: #2a3040; border-radius: 8px; padding: 8px; }
  flowboxchild:hover .card { background: #343b4e; }
  .card-title { font-size: 13px; }
  .preview { background: #1b1f2a; border-radius: 4px; }
"""


def layouts_popup(window, at=None):
    """Windows + Z : grilles de disposition ; at=(x, y) : ouverte au survol du
    bouton Agrandir, sous ce point, sans prendre le clavier, et refermée dès
    que la souris s'en éloigne."""
    Gdk, GLib, Gtk = _gtk()
    if not window: return 1
    popup = Gtk.Window(title=_t("Dispositions d’ancrage"))
    if at is not None:
        popup.set_accept_focus(False)
        popup.set_type_hint(Gdk.WindowTypeHint.UTILITY)
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
    # En haut à droite de la fenêtre, là où Windows montre les dispositions.
    geometry = window_geometry(window)
    popup.show_all()
    scale = max(1, popup.get_scale_factor())
    width, height = popup.get_size()
    if at is not None:
        # Sous le bouton Agrandir, sans sortir de l'écran (coordonnées X en pixels réels).
        area = pick_area(work_areas(), at)
        left = min(max(area[0], at[0] - width * scale // 2), area[0] + area[2] - width * scale)
        popup.move(left // scale, (at[1] + 8) // scale)
        anchor = (at[0] - 20, at[1] - 30, 40, 40)

        def pointer_left():
            _, px, py = popup.get_display().get_default_seat().get_pointer().get_position()
            px, py = px * scale, py * scale
            ox, oy = popup.get_position()
            inside = ox * scale - 24 <= px <= (ox + width) * scale + 24 and oy * scale - 24 <= py <= (oy + height) * scale + 24
            on_button = anchor[0] <= px <= anchor[0] + anchor[2] and anchor[1] <= py <= anchor[1] + anchor[3]
            if inside or on_button:
                state["away"] = 0
            else:
                state["away"] += 1
                if state["away"] >= 3: Gtk.main_quit()
            return True

        state = {"away": 0}
        GLib.timeout_add(150, pointer_left)
    elif geometry:
        x, y, w, _ = geometry
        popup.move(max(0, x + w - width * scale - 60) // scale, max(0, y + 30) // scale)
    popup.present()
    Gtk.main()
    if "zone" not in chosen: return 0
    zone = chosen["zone"]
    snap(window, zone)
    _run(["xdotool", "windowactivate", window])
    layout = next(layout for layout in LAYOUTS if zone in layout)
    free = [cell for cell in layout if cell != zone]
    if free and os.environ.get("WINLUX_SNAP_ASSIST", "1") != "0": return assist(window, free)
    return 0


def snapped_windows(exclude, area):
    """{fenêtre: rectangle} des fenêtres ancrées visibles sur ce bureau et cet écran."""
    found = {}
    for window, _ in other_windows(exclude):
        zone = get_prop(window, "_WINLUX_SNAP")
        if zone not in ZONES or is_maximized(window): continue
        rect, geometry = zone_rect(zone, area), window_geometry(window)
        # Encore à sa place (une fenêtre déplacée au clavier garde sa propriété).
        if geometry and all(abs(a - b) <= 12 for a, b in zip(rect, geometry)): found[window] = rect
    return found


def stacking_order():
    """Fenêtres du bas vers le haut (_NET_CLIENT_LIST_STACKING), en nombres."""
    out = _run(["xprop", "-root", "_NET_CLIENT_LIST_STACKING"])
    # « _NET_CLIENT_LIST_STACKING(WINDOW): window id # 0x400003, 0x600003 »
    return [int(value, 16) for value in re.findall(r"0x[0-9a-fA-F]+", out)]


def window_icon_values(window):
    """_NET_WM_ICON de la fenêtre (liste de nombres), lue par Xlib."""
    try:
        return _X().prop(window_number(window), "_NET_WM_ICON") or []
    except (OSError, AttributeError, TypeError):
        return []


def assist(window, wanted, area=None):
    """Assistant d'ancrage : la place libre se remplit de cartes, une par autre
    fenêtre ; la carte choisie va dans la case, puis la case suivante."""
    area = area or area_of(window)
    taken = {other: rect for other, rect in snapped_windows(None, area).items() if window_number(other) != window_number(window)}
    zones = free_zones(wanted, area, list(taken.values()))
    # Groupe d'ancrage : la fenêtre, celles qui occupent déjà le reste de la
    # place, puis celles choisies ici.
    group = [window] + covering(wanted, area, taken)
    form_group(group)
    if not zones: return 0
    Gdk, GLib, Gtk = _gtk()
    from gi.repository import GdkPixbuf, Gio
    try:
        __import__("gi").require_version("GdkX11", "3.0")
        from gi.repository import GdkX11
    except (ValueError, ImportError):
        GdkX11 = None
    used = [window]
    css = Gtk.CssProvider()
    css.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, 600)
    for step, zone in enumerate(zones, 1):
        numbers = {window_number(item) for item in used}
        candidates = [item for item in other_windows(None) if window_number(item[0]) not in numbers]
        if not candidates: return 0
        snapped_now = set(snapped_windows(None, area))
        candidates = order_candidates(candidates, snapped_now)[:16]
        x, y, w, h = zone_rect(zone, area)
        # Aperçus pris avant d'afficher l'assistant, seulement des fenêtres
        # entièrement visibles (pas de compositeur sous openbox).
        stacking = stacking_order()
        geometries = {}
        for candidate, _ in candidates:
            geometry = window_geometry(candidate)
            if geometry: geometries[window_number(candidate)] = geometry
        for number in stacking:
            if number not in geometries:
                geometry = window_geometry(str(number))
                if geometry: geometries[number] = geometry
        popup = Gtk.Window(title=_t("Choisissez une fenêtre pour l’autre côté") if zone in OPPOSITE.values() and len(zones) == 1
                           else _t("Choisissez une fenêtre pour cet emplacement"))
        popup.set_role("winlux-snap-assist")
        popup.set_decorated(False)
        popup.set_keep_above(True)
        popup.set_skip_taskbar_hint(True)
        popup.set_skip_pager_hint(True)
        popup.get_style_context().add_class("assist")
        scale = max(1, popup.get_scale_factor())
        columns, card_w, card_h = card_grid(w, h, scale, len(candidates))
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin=18)
        popup.add(box)
        title = Gtk.Label(label=popup.get_title(), xalign=0, ellipsize=3)
        title.get_style_context().add_class("heading")
        box.pack_start(title, False, False, 0)
        hint = Gtk.Label(label=_t("Emplacement {n} sur {total} · Échap pour fermer").format(n=step, total=len(zones))
                         if len(zones) > 1 else _t("Échap pour fermer"), xalign=0)
        hint.get_style_context().add_class("subtitle")
        box.pack_start(hint, False, False, 0)
        scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vscrollbar_policy=Gtk.PolicyType.AUTOMATIC)
        box.pack_start(scroll, True, True, 6)
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.SINGLE, activate_on_single_click=True, homogeneous=True,
                           min_children_per_line=columns, max_children_per_line=columns, row_spacing=16, column_spacing=16,
                           valign=Gtk.Align.START, halign=Gtk.Align.CENTER)
        scroll.add(flow)
        theme = Gtk.IconTheme.get_default()
        for candidate, name in candidates:
            flow.add(_card(Gtk, GdkPixbuf, Gio, Gdk, GdkX11, theme, candidate, name, card_w, card_h,
                           unobscured(window_number(candidate), stacking, geometries)))
        picked = {}
        flow.connect("child-activated", lambda _, child: (picked.setdefault("window", child.winlux_window), Gtk.main_quit()))
        popup.connect("key-press-event", lambda _, event: event.keyval == Gdk.KEY_Escape and (Gtk.main_quit() or True))
        # Comme sous Windows : passer à une autre fenêtre ferme l'assistant.
        popup.connect("focus-out-event", lambda *_: GLib.timeout_add(250, lambda: popup.is_active() or Gtk.main_quit()) and False)
        margin = 6 * scale
        popup.set_size_request(max(1, (w - 2 * margin) // scale), max(1, (h - 2 * margin) // scale))
        popup.move((x + margin) // scale, (y + margin) // scale)
        popup.show_all()
        first = flow.get_child_at_index(0)
        if first is not None:
            flow.select_child(first)
            first.grab_focus()
        popup.present()
        Gtk.main()
        popup.destroy()
        while Gtk.events_pending(): Gtk.main_iteration()
        if "window" not in picked: return 0
        snap(picked["window"], zone, area)
        _run(["wmctrl", "-i", "-a", picked["window"]])
        used.append(picked["window"])
        group.append(picked["window"])
        form_group(group)
    return 0


def covering(wanted, area, taken):
    """Fenêtres ancrées qui occupent (aux quatre cinquièmes) une des cases voulues."""
    found = []
    for zone in wanted:
        rect = zone_rect(zone, area)
        size = max(1, rect[2] * rect[3])
        for other, other_rect in taken.items():
            if other not in found and overlap(rect, other_rect) >= 0.8 * size: found.append(other)
    return found


def form_group(windows):
    """Un même identifiant de groupe sur chaque fenêtre (au moins deux)."""
    unique = []
    for item in windows:
        if window_number(item) not in {window_number(other) for other in unique}: unique.append(item)
    if len(unique) < 2: return None
    group = f"{window_number(unique[0]):x}.{os.getpid()}"
    for item in unique: set_prop(item, "_WINLUX_SNAP_GROUP", group)
    return group


def group_members(window, props):
    """Fenêtres du groupe de window d'après {fenêtre: (groupe, case)} ; un groupe
    ne compte que ses fenêtres encore ancrées, et au moins deux."""
    group, zone = props.get(window, ("", ""))
    if not group or zone not in ZONES: return []
    members = [item for item, (other, place) in props.items() if other == group and place in ZONES]
    return members if len(members) >= 2 else []


def group_props(desktop=None):
    """{fenêtre: (groupe, case)} des fenêtres du bureau courant, réduites comprises."""
    if desktop is None:
        current = re.search(r"^(\d+)\s+\*", _run(["wmctrl", "-d"]), re.M)
        desktop = current.group(1) if current else "0"
    found = {}
    for line in _run(["wmctrl", "-l"]).splitlines():
        parts = line.split(None, 3)
        if len(parts) < 3 or parts[1] not in (desktop, "-1"): continue
        group = get_prop(parts[0], "_WINLUX_SNAP_GROUP")
        if group: found[parts[0]] = (group, get_prop(parts[0], "_WINLUX_SNAP"))
    return found


def find_member(window, props):
    """La clé de props (format wmctrl) qui désigne window (n'importe quel format)."""
    return next((item for item in props if window_number(item) == window_number(window)), None)


def restore_group(window):
    """Remet tout le groupe de window au premier plan, chaque fenêtre à sa case
    (réduites comprises) ; window finit active."""
    props = group_props()
    key = find_member(window, props)
    members = group_members(key, props) if key else []
    if not members: return 1
    for item in [m for m in members if m != key] + [key]:
        _run(["wmctrl", "-i", "-a", item])
    for item in members:
        snap(item, props[item][1])
    _run(["wmctrl", "-i", "-a", key])
    return 0


def _card(Gtk, GdkPixbuf, Gio, Gdk, GdkX11, theme, window, name, card_w, card_h, visible):
    """Carte d'une fenêtre : petite icône et titre, puis l'aperçu (ou la grande icône)."""
    child = Gtk.FlowBoxChild()
    child.winlux_window = window
    card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
    card.get_style_context().add_class("card")
    card.set_size_request(card_w, card_h)
    child.add(card)
    head = Gtk.Box(spacing=8)
    small, large = _icons(Gtk, GdkPixbuf, Gio, theme, window, 20, 64)
    head.pack_start(small, False, False, 0)
    label = Gtk.Label(label=name or _t("Sans titre"), xalign=0, ellipsize=3)
    label.get_style_context().add_class("card-title")
    head.pack_start(label, True, True, 0)
    card.pack_start(head, False, False, 0)
    preview = None
    if visible and GdkX11 is not None:
        try:
            foreign = GdkX11.X11Window.foreign_new_for_display(Gdk.Display.get_default(), window_number(window))
            pixbuf = Gdk.pixbuf_get_from_window(foreign, 0, 0, foreign.get_width(), foreign.get_height()) if foreign else None
            if pixbuf is not None:
                factor = min((card_w - 16) / pixbuf.get_width(), (card_h - 48) / pixbuf.get_height())
                if factor > 0:
                    preview = Gtk.Image.new_from_pixbuf(pixbuf.scale_simple(max(1, int(pixbuf.get_width() * factor)),
                                                                            max(1, int(pixbuf.get_height() * factor)),
                                                                            GdkPixbuf.InterpType.BILINEAR))
        except Exception:
            preview = None
    frame = Gtk.Box()
    frame.get_style_context().add_class("preview")
    frame.pack_start(preview or large, True, True, 0)
    card.pack_start(frame, True, True, 0)
    child.set_tooltip_text(name or _t("Sans titre"))
    return child


def _icons(Gtk, GdkPixbuf, Gio, theme, window, small, large):
    """(petite, grande) icônes de l'application : celle de son lanceur
    (WM_CLASS), sinon _NET_WM_ICON (logiciels .exe compris), sinon générique."""
    klass = re.search(r'WM_CLASS\(\w+\) = "(.*)", "(.*)"', _run(["xprop", "-id", window, "WM_CLASS"]))
    names = []
    if klass:
        for value in (klass.group(2), klass.group(1)):
            names += [value, value.lower()]
    for value in names:
        info = None
        try: info = Gio.DesktopAppInfo.new(value + ".desktop")
        except TypeError: info = None
        icon = info.get_icon() if info else None
        if icon is None and value and theme.has_icon(value): icon = Gio.ThemedIcon.new(value)
        if icon is not None:
            return (Gtk.Image.new_from_gicon(icon, Gtk.IconSize.LARGE_TOOLBAR),
                    _sized(Gtk.Image.new_from_gicon(icon, Gtk.IconSize.DIALOG), large))
    icons = parse_icons(window_icon_values(window))
    images = []
    for size in (small, large):
        chosen = best_icon(icons, size)
        if chosen is None:
            images.append(_sized(Gtk.Image.new_from_icon_name("application-x-executable", Gtk.IconSize.DIALOG), size))
            continue
        w, h, pixels = chosen
        data = bytearray()
        for argb in pixels:
            data += bytes(((argb >> 16) & 255, (argb >> 8) & 255, argb & 255, (argb >> 24) & 255))
        pixbuf = GdkPixbuf.Pixbuf.new_from_bytes(__import__("gi").repository.GLib.Bytes.new(bytes(data)),
                                                 GdkPixbuf.Colorspace.RGB, True, 8, w, h, w * 4)
        if w != size: pixbuf = pixbuf.scale_simple(size, size, GdkPixbuf.InterpType.BILINEAR)
        images.append(Gtk.Image.new_from_pixbuf(pixbuf))
    return images[0], images[1]


def _sized(image, size):
    image.set_pixel_size(size)
    return image


# ── Groupes d'ancrage (survol d'un bouton de la barre des tâches) ─────────

def button_under(point, buttons):
    """La fenêtre dont le bouton de la barre des tâches (_NET_WM_ICON_GEOMETRY,
    posé par la barre d'XFCE) est sous la souris : [(fenêtre, rect)] → fenêtre."""
    px, py = point
    for window, (x, y, w, h) in buttons:
        if w > 0 and h > 0 and x <= px < x + w and y <= py < y + h: return window
    return None


def popup_origin(button, size, monitor):
    """Coin haut gauche de la vignette : au-dessus d'un bouton du bas de l'écran,
    en dessous d'un bouton du haut, centrée, sans sortir de l'écran."""
    bx, by, bw, bh = button
    width, height = size
    mx, my, mw, mh = monitor
    x = min(max(mx, bx + bw // 2 - width // 2), mx + mw - width)
    y = by - height - 8 if by + bh // 2 > my + mh // 2 else by + bh + 8
    return x, min(max(my, y), my + mh - height)


def group_popup(window, button):
    """Vignette « Groupe d'ancrage » au-dessus du bouton survolé : la disposition
    avec chaque fenêtre à sa case ; un clic remet le groupe au premier plan."""
    props = group_props()
    key = find_member(window, props)
    members = group_members(key, props) if key else []
    if not members: return 1
    Gdk, GLib, Gtk = _gtk()
    from gi.repository import GdkPixbuf, Gio
    popup = Gtk.Window(title=_t("Groupe d’ancrage"))
    popup.set_role("winlux-snap-group")
    popup.set_accept_focus(False)
    popup.set_type_hint(Gdk.WindowTypeHint.TOOLTIP)
    popup.set_decorated(False)
    popup.set_keep_above(True)
    popup.set_skip_taskbar_hint(True)
    popup.get_style_context().add_class("assist")
    css = Gtk.CssProvider()
    css.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(popup.get_screen(), css, 600)
    area = area_of(members[0])
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin=10)
    popup.add(box)
    heading = Gtk.Label(label=_t("Groupe d’ancrage"), xalign=0)
    heading.get_style_context().add_class("heading")
    box.pack_start(heading, False, False, 0)
    map_w = 260
    map_h = max(80, round(map_w * area[3] / max(1, area[2])))
    layout = Gtk.Fixed()
    layout.get_style_context().add_class("preview")
    layout.set_size_request(map_w, map_h)
    box.pack_start(layout, False, False, 0)
    hint = Gtk.Label(label=_t("Cliquez pour remettre le groupe au premier plan"), xalign=0, ellipsize=3)
    hint.get_style_context().add_class("subtitle")
    box.pack_start(hint, False, False, 0)
    titles = {item: title for item, title in other_windows(None)}
    for item in members:
        if item not in titles:
            titles[item] = _run(["xdotool", "getwindowname", str(window_number(item))]).strip()
    chosen = {}
    theme = Gtk.IconTheme.get_default()
    for item in members:
        fx, fy, fw, fh = ZONES[props[item][1]]
        cell = Gtk.Button()
        cell.get_style_context().add_class("window")
        cell.set_size_request(max(20, round(fw * map_w) - 4), max(20, round(fh * map_h) - 4))
        inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4, valign=Gtk.Align.CENTER)
        small, _ = _icons(Gtk, GdkPixbuf, Gio, theme, item, 20, 32)
        inner.pack_start(small, False, False, 0)
        label = Gtk.Label(label=titles.get(item) or _t("Sans titre"), ellipsize=3, max_width_chars=14)
        label.get_style_context().add_class("card-title")
        inner.pack_start(label, False, False, 0)
        cell.add(inner)
        cell.set_tooltip_text(titles.get(item) or "")
        cell.connect("clicked", lambda _, it=item: (chosen.setdefault("window", it), Gtk.main_quit()))
        layout.put(cell, 2 + round(fx * map_w), 2 + round(fy * map_h))
    popup.connect("key-press-event", lambda _, event: event.keyval == Gdk.KEY_Escape and (Gtk.main_quit() or True))
    popup.show_all()
    scale = max(1, popup.get_scale_factor())
    width, height = popup.get_size()
    monitor = pick_area([m for m in _monitors()] or [area], (button[0] + button[2] // 2, button[1] + button[3] // 2))
    x, y = popup_origin(button, (width * scale, height * scale), monitor)
    popup.move(x // scale, y // scale)
    popup.present()
    state = {"away": 0}

    def pointer_left():
        _, px, py = popup.get_display().get_default_seat().get_pointer().get_position()
        px, py = px * scale, py * scale
        ox, oy = popup.get_position()
        inside = ox * scale - 16 <= px <= (ox + width) * scale + 16 and oy * scale - 16 <= py <= (oy + height) * scale + 16
        on_button = button[0] <= px < button[0] + button[2] and button[1] - 12 <= py < button[1] + button[3] + 12
        state["away"] = 0 if inside or on_button else state["away"] + 1
        if state["away"] >= 3: Gtk.main_quit()
        return True

    GLib.timeout_add(150, pointer_left)
    Gtk.main()
    popup.destroy()
    if "window" not in chosen: return 0
    return restore_group(chosen["window"])


def _monitors():
    monitors = []
    for line in _run(["xrandr", "--listmonitors"]).splitlines()[1:]:
        match = re.search(r"(\d+)/\d+x(\d+)/\d+\+(-?\d+)\+(-?\d+)", line)
        if match:
            w, h, x, y = (int(group) for group in match.groups())
            monitors.append((x, y, w, h))
    return monitors


# ── Ancrage à la souris (winlux-snap watch) ────────────────────────────────

EDGE = 2              # pixels du bord de l'écran qui déclenchent l'ancrage
CORNER = 0.2          # part de la hauteur de l'écran qui compte comme un coin
DRAG_START = 8        # déplacement minimal (pixels) avant de parler de glisser
BAR_REACH = 72        # distance du haut où la barre des dispositions descend
HOVER_DELAY = 0.5     # secondes sur le bouton Agrandir avant la grille
BAR_CELL = (64, 40)   # case d'une disposition dans la barre (pixels à l'échelle 1)
BAR_GAP = 10


def drag_zone(point, monitor):
    """Zone visée en lâchant la fenêtre au point donné sur cet écran (bords
    réels, barre des tâches comprise) : half:*, quarter:*, « maximize » ou None."""
    px, py = point
    mx, my, mw, mh = monitor
    left, right = px <= mx + EDGE - 1, px >= mx + mw - EDGE
    top, bottom = py <= my + EDGE - 1, py >= my + mh - EDGE
    corner = max(EDGE, round(mh * CORNER))
    if left or right:
        side = "l" if left else "r"
        if py < my + corner: return f"quarter:t{side}"
        if py >= my + mh - corner: return f"quarter:b{side}"
        return "half:left" if left else "half:right"
    if top: return "maximize"
    return None


def snap_bar(monitor, scale=1):
    """Barre des dispositions en haut au centre : (rectangle de la barre,
    [(zone, rectangle de sa case)]), en pixels réels."""
    cell_w, cell_h = BAR_CELL[0] * scale, BAR_CELL[1] * scale
    gap = BAR_GAP * scale
    width = len(LAYOUTS) * cell_w + (len(LAYOUTS) + 1) * gap
    height = cell_h + 2 * gap
    mx, my, mw, _ = monitor
    bar = (mx + (mw - width) // 2, my + 8 * scale, width, height)
    cells = []
    for index, layout in enumerate(LAYOUTS):
        cx, cy = bar[0] + gap + index * (cell_w + gap), bar[1] + gap
        for zone in layout:
            fx, fy, fw, fh = ZONES[zone]
            x, y = cx + round(fx * cell_w), cy + round(fy * cell_h)
            cells.append((zone, (x + 1, y + 1, round(fw * cell_w) - 2, round(fh * cell_h) - 2)))
    return bar, cells


def bar_wanted(point, monitor, shown):
    """La barre descend quand la fenêtre approche du haut, au centre de l'écran,
    et reste tant que la souris ne s'en éloigne pas."""
    px, py = point
    mx, my, mw, _ = monitor
    reach = BAR_REACH * 2 if shown else BAR_REACH
    return py < my + reach and mx + mw // 4 <= px < mx + mw - mw // 4


def bar_zone(point, cells):
    px, py = point
    for zone, (x, y, w, h) in cells:
        if x <= px < x + w and y <= py < y + h: return zone
    return None


def float_geometry(point, frame, saved):
    """Fenêtre ancrée décrochée : sa taille d'avant, sous la souris au même endroit relatif."""
    px, _ = point
    x, y, w, _ = frame
    _, _, sw, sh = saved
    ratio = (px - x) / w if w else 0.5
    return round(px - sw * min(max(ratio, 0), 1)), y, sw, sh


def maximize_button_rank(layout):
    """Rang du bouton Agrandir en partant de la droite de la barre de titre (titleLayout d'openbox)."""
    right = layout.split("L", 1)[1] if "L" in layout else layout
    return len(right) - 1 - right.index("M") if "M" in right else None


def title_layout():
    for path in (os.path.expanduser("~/.config/openbox/rc.xml"), "/usr/share/winlux/native/config/openbox/rc.xml"):
        try:
            with open(path, encoding="utf-8") as handle:
                match = re.search(r"<titleLayout>\s*([A-Z]+)\s*</titleLayout>", handle.read())
        except OSError:
            continue
        if match: return match.group(1)
    return "NLIMC"


class _X:
    """Requêtes X directes (XQueryPointer…) : assez légères pour suivre la souris."""

    class Attributes(ctypes.Structure):
        _fields_ = [("x", ctypes.c_int), ("y", ctypes.c_int), ("width", ctypes.c_int), ("height", ctypes.c_int),
                    ("border_width", ctypes.c_int), ("depth", ctypes.c_int), ("visual", ctypes.c_void_p),
                    ("root", ctypes.c_ulong), ("c_class", ctypes.c_int), ("bit_gravity", ctypes.c_int),
                    ("win_gravity", ctypes.c_int), ("backing_store", ctypes.c_int), ("backing_planes", ctypes.c_ulong),
                    ("backing_pixel", ctypes.c_ulong), ("save_under", ctypes.c_int), ("colormap", ctypes.c_ulong),
                    ("map_installed", ctypes.c_int), ("map_state", ctypes.c_int), ("all_event_masks", ctypes.c_long),
                    ("your_event_mask", ctypes.c_long), ("do_not_propagate_mask", ctypes.c_long),
                    ("override_redirect", ctypes.c_int), ("screen", ctypes.c_void_p)]

    def __init__(self):
        lib = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
        lib.XOpenDisplay.restype = ctypes.c_void_p
        lib.XOpenDisplay.argtypes = [ctypes.c_char_p]
        lib.XDefaultRootWindow.restype = ctypes.c_ulong
        lib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        window, integer, unsigned = ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_uint)
        lib.XQueryPointer.argtypes = [ctypes.c_void_p, ctypes.c_ulong, window, window, integer, integer, integer, integer, unsigned]
        lib.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, window, window,
                                   ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), unsigned]
        lib.XGetWindowAttributes.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(_X.Attributes)]
        lib.XFree.argtypes = [ctypes.c_void_p]
        lib.XSetErrorHandler.restype = ctypes.c_void_p
        self.lib = lib
        self.display = lib.XOpenDisplay(None)
        if not self.display: raise OSError("pas d'affichage X")
        # Une fenêtre fermée entre deux requêtes ne doit pas arrêter le suivi.
        self._handler = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)(lambda *_: 0)
        lib.XSetErrorHandler(self._handler)
        self.root = lib.XDefaultRootWindow(self.display)

    def pointer(self, window=None):
        """(x, y absolus, fenêtre fille sous la souris, x, y dans la fenêtre, masque)."""
        root, child = ctypes.c_ulong(), ctypes.c_ulong()
        rx, ry, wx, wy, mask = ctypes.c_int(), ctypes.c_int(), ctypes.c_int(), ctypes.c_int(), ctypes.c_uint()
        self.lib.XQueryPointer(self.display, window or self.root, ctypes.byref(root), ctypes.byref(child), ctypes.byref(rx),
                               ctypes.byref(ry), ctypes.byref(wx), ctypes.byref(wy), ctypes.byref(mask))
        return rx.value, ry.value, child.value, wx.value, wy.value, mask.value

    def attributes(self, window):
        found = _X.Attributes()
        if not self.lib.XGetWindowAttributes(self.display, window, ctypes.byref(found)): return None
        return found

    def prop(self, window, name):
        """Propriété X : liste de nombres (format 32) ou octets (format 8), None si absente."""
        lib = self.lib
        if not hasattr(self, "_atoms"):
            self._atoms = {}
            lib.XInternAtom.argtypes, lib.XInternAtom.restype = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong
            lib.XGetWindowProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long, ctypes.c_long,
                                               ctypes.c_int, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int),
                                               ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong),
                                               ctypes.POINTER(ctypes.c_void_p)]
        atom = self._atoms.get(name)
        if atom is None:
            atom = self._atoms[name] = lib.XInternAtom(self.display, name.encode(), 0)
        kind, fmt, count, after, data = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_ulong(), ctypes.c_ulong(), ctypes.c_void_p()
        if lib.XGetWindowProperty(self.display, window, atom, 0, 1 << 20, 0, 0, ctypes.byref(kind), ctypes.byref(fmt),
                                  ctypes.byref(count), ctypes.byref(after), ctypes.byref(data)) != 0 or not data.value:
            return None
        try:
            if fmt.value == 32:
                # Xlib rend des « long » natifs (64 bits sur amd64).
                values = ctypes.cast(data, ctypes.POINTER(ctypes.c_ulong))
                return [values[index] & 0xFFFFFFFF for index in range(count.value)]
            if fmt.value == 8: return ctypes.string_at(data, count.value)
            return None
        finally:
            lib.XFree(data)

    def children(self, window):
        root, parent = ctypes.c_ulong(), ctypes.c_ulong()
        items, count = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
        if not self.lib.XQueryTree(self.display, window, ctypes.byref(root), ctypes.byref(parent), ctypes.byref(items), ctypes.byref(count)):
            return []
        found = [items[index] for index in range(count.value)]
        if items: self.lib.XFree(items)
        return found


BUTTON1, ALT = 1 << 8, 1 << 3


def hovered_maximize(x11, rank):
    """(fenêtre du cadre, x, y du bas du bouton) quand la souris est sur un bouton Agrandir."""
    if rank is None: return None
    px, py, frame, *_ = x11.pointer()
    if not frame: return None
    _, _, title, *_ = x11.pointer(frame)
    if not title: return None
    _, _, button, *_ = x11.pointer(title)
    if not button: return None
    title_attributes = x11.attributes(title)
    if title_attributes is None or title_attributes.height > 80: return None
    buttons = []
    for child in x11.children(title):
        found = x11.attributes(child)
        # Boutons d'openbox : carrés et visibles (l'étiquette du titre est large, les poignées étroites).
        if found and found.map_state == 2 and found.width >= 8 and abs(found.width - found.height) <= 4:
            buttons.append((found.x, child, found))
    buttons.sort(key=lambda item: item[0], reverse=True)
    if rank >= len(buttons) or buttons[rank][1] != button: return None
    _, _, found = buttons[rank]
    # Bas du bouton en coordonnées de l'écran : position de la souris moins sa position dans le bouton.
    _, _, _, bx, by, _ = x11.pointer(button)
    return frame, px - bx + found.width // 2, py - by + found.height


def client_of(x11, frame):
    """Fenêtre cliente (celle de _NET_CLIENT_LIST) dans un cadre openbox, en décimal."""
    listed = _run(["xprop", "-root", "_NET_CLIENT_LIST"])
    clients = {int(value, 16) for value in re.findall(r"0x[0-9a-f]+", listed.split("#", 1)[-1])}
    pending = [(frame, 0)]
    while pending:
        window, depth = pending.pop(0)
        if window in clients: return str(window)
        if depth < 3: pending.extend((child, depth + 1) for child in x11.children(window))
    return None


def drop(window, zone, point=None):
    """Fenêtre lâchée : ancrée (et l'Assistant d'ancrage pour l'autre moitié),
    agrandie, ou reprise à sa taille d'avant si elle était ancrée."""
    if zone == "maximize":
        if not get_prop(window, "_WINLUX_SNAP") and not is_maximized(window):
            geometry = window_geometry(window)
            if geometry: set_prop(window, "_WINLUX_RESTORE", ",".join(map(str, geometry)))
        _run(["wmctrl", "-i", "-r", window, "-b", "add,maximized_vert,maximized_horz"])
        leave_snap(window)
        return 0
    if zone == "float":
        saved = get_prop(window, "_WINLUX_RESTORE")
        frame = window_geometry(window)
        try: saved = tuple(int(part) for part in saved.split(","))
        except ValueError: saved = None
        if get_prop(window, "_WINLUX_SNAP") and saved and len(saved) == 4 and frame:
            geometry = float_geometry(point or (frame[0] + frame[2] // 2, frame[1]), frame, saved)
            x, y, w, h = client_geometry(geometry, frame_extents(window))
            _run(["wmctrl", "-i", "-r", window, "-e", f"1,{x},{y},{w},{h}"])
        leave_snap(window)
        return 0
    area = pick_area(work_areas(), point) if point else None
    snap(window, zone, area)
    _run(["wmctrl", "-i", "-a", window])
    if os.environ.get("WINLUX_SNAP_ASSIST", "1") != "0":
        return assist(window, following_zones(zone), area)
    return 0


def watch():
    """Suit la souris : ancrage en glissant une fenêtre, grille au survol d'Agrandir."""
    import time
    Gdk, GLib, Gtk = _gtk()
    import cairo
    try:
        x11 = _X()
    except OSError:
        return 1
    display = Gdk.Display.get_default()
    rank = maximize_button_rank(title_layout())
    composited = Gdk.Screen.get_default().is_composited()

    def monitor_at(point):
        """(écran entier, zone de travail, échelle) en pixels réels (GDK compte en pixels logiques)."""
        found = []
        for index in range(display.get_n_monitors()):
            monitor = display.get_monitor(index)
            scale = monitor.get_scale_factor()
            full, work = monitor.get_geometry(), monitor.get_workarea()
            found.append(((full.x * scale, full.y * scale, full.width * scale, full.height * scale),
                          (work.x * scale, work.y * scale, work.width * scale, work.height * scale), scale))
        for item in found:
            x, y, w, h = item[0]
            if x <= point[0] < x + w and y <= point[1] < y + h: return item
        return found[0] if found else ((0, 0, 1920, 1080), (0, 0, 1920, 1080), 1)

    # Aperçu de l'emplacement : voile translucide avec un compositeur, sinon un cadre.
    def overlay():
        window = Gtk.Window(type=Gtk.WindowType.POPUP)
        window.set_app_paintable(True)
        visual = window.get_screen().get_rgba_visual()
        if composited and visual is not None: window.set_visual(visual)
        return window

    preview = overlay()
    bar = overlay()
    state = {"drag": None, "zone": None, "bar": None, "hover": None, "hover_since": 0.0, "armed": True,
             "press": None, "bar_cells": [], "bar_zone": None}

    def draw_preview(widget, context):
        width, height = widget.get_allocated_width(), widget.get_allocated_height()
        if composited:
            context.set_operator(cairo.OPERATOR_SOURCE)
            context.set_source_rgba(0, 0, 0, 0)
            context.paint()
            context.set_operator(cairo.OPERATOR_OVER)
            context.set_source_rgba(0x6a / 255, 0x3d / 255, 0xf0 / 255, 0.28)
            context.rectangle(6, 6, width - 12, height - 12)
            context.fill()
        context.set_source_rgba(0xb6 / 255, 0x9c / 255, 0xff / 255, 0.9)
        context.set_line_width(3)
        context.rectangle(7.5, 7.5, width - 15, height - 15)
        context.stroke()
        return True

    def draw_bar(widget, context):
        scale = max(1, widget.get_scale_factor())
        ox, oy = state["bar"][0], state["bar"][1]
        context.set_operator(cairo.OPERATOR_SOURCE)
        context.set_source_rgba(0x20 / 255, 0x24 / 255, 0x30 / 255, 0.96 if composited else 1)
        context.paint()
        context.set_operator(cairo.OPERATOR_OVER)
        for zone, (x, y, w, h) in state["bar_cells"]:
            hot = zone == state["bar_zone"]
            context.set_source_rgb(*((0x6a / 255, 0x3d / 255, 0xf0 / 255) if hot else (0x3a / 255, 0x42 / 255, 0x54 / 255)))
            context.rectangle((x - ox) / scale, (y - oy) / scale, w / scale, h / scale)
            context.fill()
        return True

    preview.connect("draw", draw_preview)
    bar.connect("draw", draw_bar)
    if not composited:
        # Sans compositeur, un voile opaque cacherait l'écran : seul le cadre se voit.
        def frame_shape(widget, allocation):
            try:
                region = cairo.Region(cairo.RectangleInt(0, 0, allocation.width, allocation.height))
                region.subtract(cairo.RectangleInt(10, 10, max(1, allocation.width - 20), max(1, allocation.height - 20)))
                widget.shape_combine_region(region)
            except (TypeError, AttributeError):
                pass
        preview.connect("size-allocate", frame_shape)

    def place(window, rect, scale):
        x, y, w, h = rect
        window.move(x // scale, y // scale)
        window.resize(max(1, w // scale), max(1, h // scale))
        window.show_all()
        window.queue_draw()

    def show_target(zone, area, monitor, scale):
        if zone == state["zone"]: return
        state["zone"] = zone
        if zone is None:
            preview.hide()
            return
        place(preview, zone_rect(zone, area) if zone in ZONES else area, scale)

    def pressed_on_title(mask):
        """Bouton pris sur la barre de titre d'un cadre openbox (ou n'importe où avec Alt) : la
        fenêtre cliente sous la souris, sinon None. Vérifié à l'appui, avant tout déplacement."""
        px, py, frame, *_ = x11.pointer()
        if not frame: return None
        if not mask & ALT:
            _, _, title, *_ = x11.pointer(frame)
            title_attributes = x11.attributes(title) if title else None
            frame_attributes = x11.attributes(frame)
            if title_attributes is None or frame_attributes is None: return None
            if title_attributes.height > 80 or title_attributes.width < frame_attributes.width - 16: return None
            _, _, _, _, title_y, _ = x11.pointer(title)
            if title_y < 3: return None  # bord du haut : redimensionnement
        window = client_of(x11, frame)
        if not window: return None
        kind = _run(["xprop", "-id", window, "_NET_WM_WINDOW_TYPE"])
        if "_NET_WM_WINDOW_TYPE_NORMAL" not in kind and "_NET_WM_WINDOW_TYPE(ATOM)" in kind: return None
        frame_geometry = window_geometry(window)
        if frame_geometry is None: return None
        return {"window": window, "snapped": bool(get_prop(window, "_WINLUX_SNAP")), "start": frame_geometry[:2]}

    def finish_drag(point):
        drag, zone = state["drag"], state["bar_zone"] or state["zone"]
        preview.hide()
        bar.hide()
        state.update(drag=None, zone=None, bar=None, bar_zone=None, bar_cells=[])
        if drag is None: return
        frame = window_geometry(drag["window"])
        moved = frame is not None and frame[:2] != drag["start"]
        if zone is None and not (drag["snapped"] and moved): return
        args = [sys.executable, os.path.abspath(__file__), "drop", drag["window"], zone or "float", str(point[0]), str(point[1])]
        # Après la fin du déplacement par openbox (il applique la dernière position au relâchement).
        GLib.timeout_add(60, lambda: subprocess.Popen(args, stdout=subprocess.DEVNULL, start_new_session=True) and False)

    def tick():
        px, py, _, _, _, mask = x11.pointer()
        point = (px, py)
        if mask & BUTTON1:
            if state["press"] is None:
                state["press"] = (px, py, pressed_on_title(mask))
            press = state["press"]
            if state["drag"] is None and press[2] and abs(px - press[0]) + abs(py - press[1]) >= DRAG_START:
                state["drag"] = press[2]
            if state["drag"]:
                monitor, area, scale = monitor_at(point)
                shown = state["bar"] is not None
                if bar_wanted(point, monitor, shown):
                    rect, cells = snap_bar(monitor, scale)
                    if not shown:
                        state["bar"], state["bar_cells"] = rect, cells
                        place(bar, rect, scale)
                    hot = bar_zone(point, state["bar_cells"])
                    if hot != state["bar_zone"]:
                        state["bar_zone"] = hot
                        bar.queue_draw()
                    show_target(hot or (drag_zone(point, monitor) if py <= monitor[1] + EDGE - 1 else None), area, monitor, scale)
                else:
                    if shown:
                        bar.hide()
                        state.update(bar=None, bar_zone=None, bar_cells=[])
                    show_target(drag_zone(point, monitor), area, monitor, scale)
            GLib.timeout_add(30, tick)
            return False
        if state["press"] is not None:
            state["press"] = None
            finish_drag(point)
        now = time.monotonic()
        # Survol d'un bouton de la barre des tâches d'une fenêtre groupée
        # (toutes les 300 ms : lecture de propriétés X, sans programme lancé).
        state["ticks"] = state.get("ticks", 0) + 1
        if state["ticks"] % 3 == 0:
            buttons = []
            for client in x11.prop(x11.root, "_NET_CLIENT_LIST") or []:
                if not x11.prop(client, "_WINLUX_SNAP_GROUP"): continue
                rect = x11.prop(client, "_NET_WM_ICON_GEOMETRY")
                if rect and len(rect) >= 4: buttons.append((client, tuple(rect[:4])))
            over = button_under(point, buttons)
            if over is None:
                state.update(group_hover=None, group_armed=True)
            elif state.get("group_hover") != over:
                state.update(group_hover=over, group_since=now, group_armed=True)
            elif state.get("group_armed") and now - state["group_since"] >= HOVER_DELAY:
                state["group_armed"] = False
                rect = dict(buttons)[over]
                subprocess.Popen([sys.executable, os.path.abspath(__file__), "group", str(over), *map(str, rect)],
                                 stdout=subprocess.DEVNULL, start_new_session=True)
        # Survol du bouton Agrandir.
        hovered = hovered_maximize(x11, rank)
        if hovered is None:
            state.update(hover=None, armed=True)
        elif state["hover"] != hovered[0]:
            state.update(hover=hovered[0], hover_since=now)
        elif state["armed"] and now - state["hover_since"] >= HOVER_DELAY:
            state["armed"] = False
            client = client_of(x11, hovered[0])
            allowed = _run(["xprop", "-id", client, "_NET_WM_ALLOWED_ACTIONS"]) if client else ""
            if client and "MAXIMIZE" in allowed:
                subprocess.Popen([sys.executable, os.path.abspath(__file__), "layouts", client,
                                  str(hovered[1]), str(hovered[2])], stdout=subprocess.DEVNULL, start_new_session=True)
        GLib.timeout_add(100, tick)
        return False

    GLib.timeout_add(100, tick)
    Gtk.main()
    return 0


def key(direction):
    window = active_window()
    if not window: return 1
    state = get_prop(window, "_WINLUX_SNAP")
    action, zone = next_state(state, direction, is_maximized(window))
    if action == "snap":
        snap(window, zone)
        if os.environ.get("WINLUX_SNAP_ASSIST", "1") != "0":
            return assist(window, following_zones(zone))
    elif action == "restore":
        restore(window)
    elif action == "maximize":
        if not state and not is_maximized(window):
            geometry = window_geometry(window)
            if geometry: set_prop(window, "_WINLUX_RESTORE", ",".join(map(str, geometry)))
        _run(["wmctrl", "-i", "-r", window, "-b", "add,maximized_vert,maximized_horz"])
        leave_snap(window)
    elif action == "minimize":
        _run(["xdotool", "windowminimize", window])
    return 0


def main(argv):
    if len(argv) == 3 and argv[1] == "key" and argv[2] in ("left", "right", "up", "down"):
        return key(argv[2])
    if len(argv) == 2 and argv[1] == "layouts":
        return layouts_popup(active_window())
    if len(argv) == 5 and argv[1] == "layouts" and argv[3].isdigit() and argv[4].isdigit():
        return layouts_popup(argv[2], (int(argv[3]), int(argv[4])))
    if len(argv) == 7 and argv[1] == "group" and all(v.lstrip("-").isdigit() for v in argv[3:7]):
        return group_popup(argv[2], tuple(int(v) for v in argv[3:7]))
    if len(argv) == 3 and argv[1] == "restore-group":
        return restore_group(argv[2])
    if len(argv) == 2 and argv[1] == "watch":
        return watch()
    if len(argv) in (4, 6) and argv[1] == "drop" and (argv[3] in ZONES or argv[3] in ("maximize", "float")):
        return drop(argv[2], argv[3], tuple(int(v) for v in argv[4:6]) if len(argv) == 6 else None)
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
