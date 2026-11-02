#!/usr/bin/python3
"""Pipette à couleurs (Windows + Maj + C), comme l'outil de Windows.

Le raccourci fige l'écran et montre une loupe sous le pointeur ; un clic
choisit la couleur du pixel, Échap ou le clic droit annule. Le code HEX est
copié tout de suite dans le presse-papiers, puis une petite fenêtre montre la
couleur en HEX, RGB et HSL (un bouton Copier pour chacun) et les dernières
couleurs choisies.

    winlux-colorpicker               choisir une couleur à l'écran
    winlux-colorpicker show          fenêtre des couleurs sans choisir
    winlux-colorpicker at X Y        couleur du pixel X, Y (sans fenêtre, tests)
    winlux-colorpicker format HEX    HEX, RGB et HSL d'une couleur (tests)
    winlux-colorpicker history       dernières couleurs, la plus récente en tête
    winlux-colorpicker clear         efface l'historique

Historique : ~/.local/state/winlux/colors (20 couleurs).
"""
import colorsys
import os
from pathlib import Path
import re
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
    'Pipette': ('Color Picker', 'Selector de colores', 'Farbwähler'),
    'Choisir une couleur': ('Pick a color', 'Elegir un color', 'Farbe auswählen'),
    'Copier': ('Copy', 'Copiar', 'Kopieren'),
    'Copié': ('Copied', 'Copiado', 'Kopiert'),
    'Couleurs récentes': ('Recent colors', 'Colores recientes', 'Zuletzt verwendete Farben'),
    'Effacer': ('Clear', 'Borrar', 'Löschen'),
    'Aucune couleur pour l’instant : Windows + Maj + C pour en choisir une.': (
        'No colors yet: press Windows + Shift + C to pick one.',
        'Aún no hay colores: Windows + Mayús + C para elegir uno.',
        'Noch keine Farben: Windows + Umschalt + C zum Auswählen.'),
    'Clic : choisir · Échap : annuler': ('Click: pick · Esc: cancel', 'Clic: elegir · Esc: cancelar',
                                         'Klick: auswählen · Esc: abbrechen'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


HISTORY_SIZE = 20


def state_file():
    base = Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state")
    return base / "winlux/colors"


# ── Couleurs ────────────────────────────────────────────────────────────────

def parse_hex(text):
    """'#1a2B3c', '1a2b3c' ou '#abc' → (r, g, b) ; None si invalide."""
    value = text.strip().lstrip("#")
    if re.fullmatch(r"[0-9a-fA-F]{3}", value): value = "".join(c * 2 for c in value)
    if not re.fullmatch(r"[0-9a-fA-F]{6}", value): return None
    return tuple(int(value[i:i + 2], 16) for i in (0, 2, 4))


def hex_text(rgb):
    return "#%02X%02X%02X" % tuple(rgb)


def rgb_text(rgb):
    return "rgb(%d, %d, %d)" % tuple(rgb)


def hsl_text(rgb):
    hue, light, sat = colorsys.rgb_to_hls(*(c / 255 for c in rgb))
    return "hsl(%d, %d%%, %d%%)" % (round(hue * 360) % 360, round(sat * 100), round(light * 100))


def formats(rgb):
    return [("HEX", hex_text(rgb)), ("RGB", rgb_text(rgb)), ("HSL", hsl_text(rgb))]


def remember(history, color):
    """Couleur en tête, sans doublon, 20 au plus."""
    color = color.upper()
    return ([color] + [c for c in history if c.upper() != color])[:HISTORY_SIZE]


def load_history():
    try: lines = state_file().read_text(encoding="utf-8").split()
    except OSError: return []
    return [hex_text(rgb) for rgb in map(parse_hex, lines) if rgb][:HISTORY_SIZE]


def save_history(history):
    path = state_file()
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("".join(c + "\n" for c in history), encoding="utf-8")
    except OSError:
        pass


def pixel(pixbuf, x, y):
    """(r, g, b) d'un pixel d'un GdkPixbuf."""
    x = max(0, min(pixbuf.get_width() - 1, x))
    y = max(0, min(pixbuf.get_height() - 1, y))
    data = pixbuf.get_pixels()
    offset = y * pixbuf.get_rowstride() + x * pixbuf.get_n_channels()
    return tuple(data[offset:offset + 3])


# ── Interfaces (GTK) ────────────────────────────────────────────────────────

def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    return Gdk, GLib, Gtk


def screenshot():
    Gdk, _, _ = _gtk()
    root = Gdk.get_default_root_window()
    return Gdk.pixbuf_get_from_window(root, 0, 0, root.get_width(), root.get_height())


def copy_text(text):
    Gdk, _, Gtk = _gtk()
    clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
    clipboard.set_text(text, -1)
    clipboard.store()


LOUPE_PIXELS = 15   # pixels montrés dans la loupe (impair : celui du centre est choisi)
LOUPE_ZOOM = 9


def pick():
    """Écran figé plein écran, loupe sous le pointeur ; renvoie (r, g, b) ou None."""
    Gdk, GLib, Gtk = _gtk()
    import cairo
    shot = screenshot()
    if shot is None: return None
    window = Gtk.Window(type=Gtk.WindowType.POPUP)
    window.set_title(_t("Pipette"))
    window.move(0, 0)
    window.resize(shot.get_width(), shot.get_height())
    area = Gtk.DrawingArea()
    area.add_events(Gdk.EventMask.POINTER_MOTION_MASK | Gdk.EventMask.BUTTON_PRESS_MASK)
    window.add(area)
    state = {"x": -1000, "y": -1000, "color": None}

    def draw(_, cr):
        Gdk.cairo_set_source_pixbuf(cr, shot, 0, 0)
        cr.paint()
        x, y = state["x"], state["y"]
        if x < -100: return
        size = LOUPE_PIXELS * LOUPE_ZOOM
        lx = x + 24 if x + 24 + size < shot.get_width() else x - 24 - size
        ly = y + 24 if y + 24 + size + 30 < shot.get_height() else y - 24 - size - 30
        half = LOUPE_PIXELS // 2
        cr.save()
        cr.arc(lx + size / 2, ly + size / 2, size / 2, 0, 6.2832)
        cr.clip()
        cr.translate(lx, ly)
        cr.scale(LOUPE_ZOOM, LOUPE_ZOOM)
        Gdk.cairo_set_source_pixbuf(cr, shot, -(x - half), -(y - half))
        cr.get_source().set_filter(cairo.FILTER_NEAREST)
        cr.paint()
        cr.restore()
        cr.set_line_width(2)
        cr.set_source_rgb(1, 1, 1)
        cr.arc(lx + size / 2, ly + size / 2, size / 2, 0, 6.2832)
        cr.stroke()
        cr.set_source_rgb(0, 0, 0)
        cr.rectangle(lx + half * LOUPE_ZOOM, ly + half * LOUPE_ZOOM, LOUPE_ZOOM, LOUPE_ZOOM)
        cr.stroke()
        rgb = pixel(shot, x, y)
        label = hex_text(rgb)
        cr.set_font_size(14)
        cr.rectangle(lx + size / 2 - 50, ly + size + 6, 100, 24)
        cr.set_source_rgb(0.13, 0.14, 0.19)
        cr.fill()
        cr.rectangle(lx + size / 2 - 44, ly + size + 12, 12, 12)
        cr.set_source_rgb(*(c / 255 for c in rgb))
        cr.fill()
        cr.set_source_rgb(0.93, 0.93, 0.96)
        cr.move_to(lx + size / 2 - 26, ly + size + 23)
        cr.show_text(label)

    def moved(_, event):
        state["x"], state["y"] = int(event.x), int(event.y)
        area.queue_draw()

    def pressed(_, event):
        if event.button == 1:
            state["color"] = pixel(shot, int(event.x), int(event.y))
        Gtk.main_quit()

    def key(_, event):
        step = {Gdk.KEY_Left: (-1, 0), Gdk.KEY_Right: (1, 0), Gdk.KEY_Up: (0, -1), Gdk.KEY_Down: (0, 1)}
        if event.keyval == Gdk.KEY_Escape:
            Gtk.main_quit()
        elif event.keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter, Gdk.KEY_space) and state["x"] >= 0:
            state["color"] = pixel(shot, state["x"], state["y"])
            Gtk.main_quit()
        elif event.keyval in step:
            # Flèches : déplacer le pointeur d'un pixel, pour viser juste.
            dx, dy = step[event.keyval]
            pointer = window.get_display().get_default_seat().get_pointer()
            _, px, py = pointer.get_position()[0:3]
            pointer.warp(window.get_screen(), px + dx, py + dy)
        return True

    area.connect("draw", draw)
    area.connect("motion-notify-event", moved)
    area.connect("button-press-event", pressed)
    window.connect("key-press-event", key)
    window.show_all()
    gdk_window = window.get_window()
    gdk_window.set_cursor(Gdk.Cursor.new_from_name(window.get_display(), "crosshair"))
    seat = window.get_display().get_default_seat()

    def grab():
        status = seat.grab(gdk_window, Gdk.SeatCapabilities.ALL, True, None, None, None, None)
        return status != Gdk.GrabStatus.SUCCESS and grab.tries.append(1) is None and len(grab.tries) < 20
    grab.tries = []
    if grab(): GLib.timeout_add(50, grab)
    seat_pointer = seat.get_pointer()
    _, px, py = seat_pointer.get_position()[0:3]
    state["x"], state["y"] = px, py
    Gtk.main()
    seat.ungrab()
    window.destroy()
    while Gtk.events_pending(): Gtk.main_iteration()
    return state["color"]


CSS = b"""
  .swatch { border-radius: 8px; border: 1px solid rgba(127,127,127,.4); }
  .value { font-family: monospace; font-size: 13px; }
  button.recent { min-width: 24px; min-height: 24px; padding: 0; border-radius: 4px; border: 1px solid rgba(127,127,127,.5); }
"""


def show(color=None):
    """Fenêtre de la couleur choisie, avec ses formats et les couleurs récentes."""
    Gdk, GLib, Gtk = _gtk()
    history = load_history()
    if color is None and history: color = parse_hex(history[0])
    window = Gtk.Window(title=_t("Pipette"))
    window.set_role("winlux-colorpicker")
    window.set_icon_name("color-select")
    window.set_keep_above(True)
    window.set_resizable(False)
    window.set_position(Gtk.WindowPosition.CENTER)
    css = Gtk.CssProvider()
    css.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(window.get_screen(), css, 600)
    outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=16)
    window.add(outer)
    top = Gtk.Box(spacing=12)
    outer.pack_start(top, False, False, 0)
    swatch = Gtk.DrawingArea()
    swatch.set_size_request(96, 96)
    swatch.get_style_context().add_class("swatch")
    top.pack_start(swatch, False, False, 0)
    grid = Gtk.Grid(row_spacing=6, column_spacing=8)
    top.pack_start(grid, True, True, 0)
    labels = []
    for row, name in enumerate(("HEX", "RGB", "HSL")):
        grid.attach(Gtk.Label(label=name, xalign=0), 0, row, 1, 1)
        value = Gtk.Entry(editable=False, width_chars=19)
        value.get_style_context().add_class("value")
        grid.attach(value, 1, row, 1, 1)
        button = Gtk.Button(label=_t("Copier"))
        button.connect("clicked", lambda b, v=value: (copy_text(v.get_text()), b.set_label(_t("Copié")),
                                                       GLib.timeout_add(1200, lambda: b.set_label(_t("Copier")) and False)))
        grid.attach(button, 2, row, 1, 1)
        labels.append(value)
    empty = Gtk.Label(label=_t("Aucune couleur pour l’instant : Windows + Maj + C pour en choisir une."), wrap=True,
                      max_width_chars=40)
    outer.pack_start(empty, False, False, 0)
    head = Gtk.Box(spacing=6)
    head.pack_start(Gtk.Label(label=_t("Couleurs récentes"), xalign=0), True, True, 0)
    clear = Gtk.Button(label=_t("Effacer"))
    head.pack_end(clear, False, False, 0)
    outer.pack_start(head, False, False, 0)
    recent = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=10, min_children_per_line=1,
                          halign=Gtk.Align.START, homogeneous=True)
    outer.pack_start(recent, False, False, 0)
    again = Gtk.Button(label=_t("Choisir une couleur"))
    outer.pack_start(again, False, False, 0)
    current = {"rgb": color}

    def paint(_, cr):
        rgb = current["rgb"] or (128, 128, 128)
        cr.set_source_rgb(*(c / 255 for c in rgb))
        cr.paint()

    def select(rgb):
        current["rgb"] = rgb
        for entry, (_, text) in zip(labels, formats(rgb) if rgb else [("", "")] * 3):
            entry.set_text(text)
        swatch.queue_draw()

    def fill_recent():
        for child in recent.get_children(): child.destroy()
        for hexa in history:
            button = Gtk.Button(tooltip_text=hexa)
            button.set_size_request(26, 26)
            button.get_style_context().add_class("recent")
            provider = Gtk.CssProvider()
            provider.load_from_data(f"button {{ background: {hexa}; }}".encode())
            button.get_style_context().add_provider(provider, 700)
            button.connect("clicked", lambda _, h=hexa: select(parse_hex(h)))
            recent.add(button)
        recent.show_all()
        clear.set_sensitive(bool(history))
        empty.set_visible(not history)

    def wipe(_):
        history.clear()
        save_history(history)
        fill_recent()

    def pick_again(_):
        window.hide()
        while Gtk.events_pending(): Gtk.main_iteration()
        rgb = pick()
        if rgb:
            history[:] = remember(history, hex_text(rgb))
            save_history(history)
            copy_text(hex_text(rgb))
            select(rgb)
            fill_recent()
        window.show()
        window.present()

    swatch.connect("draw", paint)
    clear.connect("clicked", wipe)
    again.connect("clicked", pick_again)
    window.connect("destroy", Gtk.main_quit)
    window.connect("key-press-event", lambda _, e: e.keyval == Gdk.KEY_Escape and (window.destroy() or True))
    window.show_all()
    select(color)
    fill_recent()
    Gtk.main()
    return 0


def main(argv):
    args = argv[1:]
    if args == ["history"]:
        print("\n".join(load_history()))
        return 0
    if args == ["clear"]:
        save_history([])
        return 0
    if len(args) == 2 and args[0] == "format":
        rgb = parse_hex(args[1])
        if rgb is None: return 2
        for name, text in formats(rgb): print(f"{name} {text}")
        return 0
    if len(args) == 3 and args[0] == "at":
        shot = screenshot()
        if shot is None: return 1
        print(hex_text(pixel(shot, int(args[1]), int(args[2]))))
        return 0
    if args == ["show"]:
        return show()
    if not args or args == ["pick"]:
        rgb = pick()
        if rgb is None: return 0
        save_history(remember(load_history(), hex_text(rgb)))
        copy_text(hex_text(rgb))
        return show(rgb)
    print(__doc__.strip().split("\n\n")[-2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
