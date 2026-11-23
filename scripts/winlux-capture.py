#!/usr/bin/python3
"""Capture d'écran (Windows + Maj + S, Impr. écran).

L'écran se fige et s'assombrit ; une barre en haut choisit le mode
(rectangle, fenêtre, plein écran). La capture est copiée dans le
presse-papiers et enregistrée en PNG dans Images > Captures d'écran, puis
une notification le dit. Rien ne reste chargé : le programme se ferme
aussitôt la capture faite (xclip garde seul l'image dans le presse-papiers,
comme un copier ordinaire, jusqu'au copier suivant).

    winlux-capture                 choisir une zone, une fenêtre ou tout l'écran
    winlux-capture ecran           tout l'écran, sans question (Windows + Impr. écran)
    winlux-capture fenetre         la fenêtre active, sans question (Alt + Impr. écran)
    winlux-capture dossier         dossier des captures (tests)
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


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
    'Captures d’écran': ('Screenshots', 'Capturas de pantalla', 'Bildschirmfotos'),
    'Capture d’écran': ('Screenshot', 'Captura de pantalla', 'Bildschirmfoto'),
    'Rectangle': ('Rectangle', 'Rectángulo', 'Rechteck'),
    'Fenêtre': ('Window', 'Ventana', 'Fenster'),
    'Plein écran': ('Full screen', 'Pantalla completa', 'Vollbild'),
    'Tracez la zone à capturer · Échap : annuler': (
        'Drag to select an area · Esc: cancel', 'Arrastre para elegir una zona · Esc: cancelar',
        'Bereich aufziehen · Esc: abbrechen'),
    'Cliquez sur la fenêtre à capturer · Échap : annuler': (
        'Click the window to capture · Esc: cancel', 'Haga clic en la ventana que desea capturar · Esc: cancelar',
        'Auf das aufzunehmende Fenster klicken · Esc: abbrechen'),
    'Capture copiée dans le Presse-papiers': (
        'Screenshot copied to clipboard', 'Captura copiada en el Portapapeles', 'Bildschirmfoto in die Zwischenablage kopiert'),
    'Enregistrée dans {path}': ('Saved to {path}', 'Guardada en {path}', 'Gespeichert unter {path}'),
    'Capture impossible : {error}': ('Screenshot failed: {error}', 'No se pudo capturar: {error}',
                                     'Bildschirmfoto fehlgeschlagen: {error}'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── Logique pure (testée sans écran) ────────────────────────────────────────

def pictures_dir(home=None, user_dirs=None):
    """Dossier Images du compte (XDG_PICTURES_DIR de user-dirs.dirs), sinon ~/Images."""
    home = Path(home or Path.home())
    path = Path(user_dirs) if user_dirs else home / ".config/user-dirs.dirs"
    try:
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.startswith("XDG_PICTURES_DIR="):
                value = line.split("=", 1)[1].strip().strip('"').replace("$HOME", str(home))
                if value.startswith("/") and Path(value) != home: return Path(value)
    except OSError:
        pass
    return home / "Images"


def captures_dir(home=None, user_dirs=None):
    return pictures_dir(home, user_dirs) / _t("Captures d’écran")


def file_name(moment, folder):
    """« Capture d’écran 2026-11-22 203015.png », suffixe (2), (3)… si le nom est pris."""
    stem = f"{_t('Capture d’écran')} {time.strftime('%Y-%m-%d %H%M%S', moment)}"
    candidate, n = folder / f"{stem}.png", 2
    while candidate.exists():
        candidate, n = folder / f"{stem} ({n}).png", n + 1
    return candidate


def normalized(start, end):
    """Rectangle (x, y, largeur, hauteur) de deux coins quelconques."""
    (x1, y1), (x2, y2) = start, end
    return min(x1, x2), min(y1, y2), abs(x2 - x1), abs(y2 - y1)


def clipped(rect, width, height):
    """Rectangle borné à l'écran ; None s'il est vide (fenêtre hors écran)."""
    x, y, w, h = rect
    x0, y0, x1, y1 = max(0, x), max(0, y), min(width, x + w), min(height, y + h)
    return (x0, y0, x1 - x0, y1 - y0) if x1 - x0 >= 1 and y1 - y0 >= 1 else None


def window_at(windows, x, y):
    """La fenêtre visible la plus haute sous (x, y) ; `windows` va du bas vers le haut."""
    for rect in reversed(windows):
        wx, wy, ww, wh = rect
        if wx <= x < wx + ww and wy <= y < wy + wh: return rect
    return None


# ── Écran ───────────────────────────────────────────────────────────────────

def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    return Gdk, GLib, Gtk


def screen_shot():
    Gdk, _, _ = _gtk()
    root = Gdk.get_default_root_window()
    return Gdk.pixbuf_get_from_window(root, 0, 0, root.get_width(), root.get_height())


def window_rects():
    """Cadres des fenêtres visibles, du bas vers le haut de la pile."""
    Gdk, _, _ = _gtk()
    rects = []
    for window in Gdk.Screen.get_default().get_window_stack() or []:
        if not window.is_visible() or window.get_type_hint() == Gdk.WindowTypeHint.DESKTOP: continue
        frame = window.get_frame_extents()
        rects.append((frame.x, frame.y, frame.width, frame.height))
    return rects


def active_rect():
    Gdk, _, _ = _gtk()
    window = Gdk.Screen.get_default().get_active_window()
    if window is None: return None
    frame = window.get_frame_extents()
    return frame.x, frame.y, frame.width, frame.height


def crop(shot, rect):
    rect = clipped(rect, shot.get_width(), shot.get_height())
    return shot.new_subpixbuf(*rect).copy() if rect else None


def select(shot):
    """Écran figé et barre des modes ; renvoie le GdkPixbuf choisi ou None."""
    Gdk, GLib, Gtk = _gtk()
    windows = window_rects()
    width, height = shot.get_width(), shot.get_height()
    window = Gtk.Window(type=Gtk.WindowType.POPUP)
    window.move(0, 0)
    window.resize(width, height)
    area = Gtk.DrawingArea()
    area.add_events(Gdk.EventMask.POINTER_MOTION_MASK | Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.BUTTON_RELEASE_MASK)
    window.add(area)
    modes = [("rect", _t("Rectangle")), ("window", _t("Fenêtre")), ("full", _t("Plein écran"))]
    state = {"mode": "rect", "start": None, "end": None, "hover": None, "done": None, "buttons": []}

    def finish(rect):
        state["done"] = crop(shot, rect) if rect else None
        Gtk.main_quit()

    def toolbar(cr):
        cr.set_font_size(14)
        sizes = [cr.text_extents(label).x_advance + 28 for _, label in modes]
        total = sum(sizes) + 8 * (len(sizes) + 1)
        x, y, bar_h = (width - total) / 2, 16, 40
        cr.set_source_rgba(0.14, 0.13, 0.22, 0.96)
        cr.rectangle(x, y, total, bar_h)
        cr.fill()
        state["buttons"] = []
        bx = x + 8
        for (key, label), size in zip(modes, sizes):
            if key == state["mode"]:
                cr.set_source_rgb(0x6A / 255, 0x3D / 255, 0xF0 / 255)
                cr.rectangle(bx, y + 6, size, bar_h - 12)
                cr.fill()
            cr.set_source_rgb(0.97, 0.96, 1)
            cr.move_to(bx + 14, y + 25)
            cr.show_text(label)
            state["buttons"].append((key, bx, y, size, bar_h))
            bx += size + 8
        hint = _t("Cliquez sur la fenêtre à capturer · Échap : annuler") if state["mode"] == "window" \
            else _t("Tracez la zone à capturer · Échap : annuler")
        extents = cr.text_extents(hint)
        cr.set_source_rgba(0.14, 0.13, 0.22, 0.9)
        cr.rectangle((width - extents.width) / 2 - 12, y + bar_h + 8, extents.width + 24, 30)
        cr.fill()
        cr.set_source_rgb(0.97, 0.96, 1)
        cr.move_to((width - extents.width) / 2, y + bar_h + 28)
        cr.show_text(hint)

    def draw(_, cr):
        Gdk.cairo_set_source_pixbuf(cr, shot, 0, 0)
        cr.paint()
        cr.set_source_rgba(0, 0, 0, 0.45)
        cr.rectangle(0, 0, width, height)
        light = None
        if state["mode"] == "rect" and state["start"] and state["end"]:
            light = normalized(state["start"], state["end"])
        elif state["mode"] == "window" and state["hover"]:
            light = clipped(state["hover"], width, height)
        if light:
            x, y, w, h = light
            cr.rectangle(x, y + h, w, -h)      # sens inverse : la zone reste claire
        cr.fill()
        if light:
            cr.set_source_rgb(0xB6 / 255, 0x9C / 255, 1)
            cr.set_line_width(2)
            cr.rectangle(light[0], light[1], light[2], light[3])
            cr.stroke()
        toolbar(cr)

    def button_at(x, y):
        for key, bx, by, bw, bh in state["buttons"]:
            if bx <= x < bx + bw and by <= y < by + bh: return key
        return None

    def pressed(_, event):
        if event.button != 1:
            finish(None)
            return
        key = button_at(event.x, event.y)
        if key == "full":
            finish((0, 0, width, height))
        elif key:
            state["mode"], state["start"], state["end"] = key, None, None
            area.queue_draw()
        elif state["mode"] == "window":
            rect = window_at(windows, int(event.x), int(event.y))
            if rect: finish(rect)
        else:
            state["start"] = state["end"] = (int(event.x), int(event.y))

    def moved(_, event):
        if state["mode"] == "window":
            rect = window_at(windows, int(event.x), int(event.y))
            if rect != state["hover"]:
                state["hover"] = rect
                area.queue_draw()
        elif state["start"]:
            state["end"] = (int(event.x), int(event.y))
            area.queue_draw()

    def released(_, event):
        if event.button != 1 or state["mode"] != "rect" or not state["start"]: return
        state["end"] = (int(event.x), int(event.y))
        rect = normalized(state["start"], state["end"])
        if rect[2] >= 4 and rect[3] >= 4: finish(rect)
        else: state["start"] = state["end"] = None

    area.connect("draw", draw)
    area.connect("button-press-event", pressed)
    area.connect("motion-notify-event", moved)
    area.connect("button-release-event", released)
    window.connect("key-press-event", lambda _, e: e.keyval == Gdk.KEY_Escape and (finish(None) or True))
    window.show_all()
    gdk_window = window.get_window()
    gdk_window.set_cursor(Gdk.Cursor.new_from_name(window.get_display(), "crosshair"))
    seat = window.get_display().get_default_seat()
    tries = []

    def grab():
        ok = seat.grab(gdk_window, Gdk.SeatCapabilities.ALL, True, None, None, None, None) == Gdk.GrabStatus.SUCCESS
        tries.append(1)
        return not ok and len(tries) < 20
    if grab(): GLib.timeout_add(50, grab)
    Gtk.main()
    seat.ungrab()
    window.destroy()
    while Gtk.events_pending(): Gtk.main_iteration()
    return state["done"]


# ── Résultat ────────────────────────────────────────────────────────────────

def save(image, folder=None):
    folder = Path(folder or captures_dir())
    folder.mkdir(parents=True, exist_ok=True)
    path = file_name(time.localtime(), folder)
    image.savev(str(path), "png", [], [])
    return path


def copy_image(path):
    """Image PNG au presse-papiers. xclip (≈ 1 Mo) la sert puis se ferme au
    copier suivant ; sans xclip, ce programme la garde jusque-là (10 min au plus)."""
    xclip = shutil.which("xclip")
    if xclip:
        try:
            subprocess.run([xclip, "-selection", "clipboard", "-t", "image/png", "-i", str(path)],
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                           timeout=5, check=True)
            return True
        except (OSError, subprocess.SubprocessError):
            pass
    try:
        Gdk, GLib, Gtk = _gtk()
        from gi.repository import GdkPixbuf
        clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
        clipboard.set_image(GdkPixbuf.Pixbuf.new_from_file(str(path)))
        clipboard.store()
        started = time.monotonic()
        clipboard.connect("owner-change", lambda *_: time.monotonic() - started > 0.5 and Gtk.main_quit())
        GLib.timeout_add_seconds(600, Gtk.main_quit)
        Gtk.main()
        return True
    except Exception:  # pas de GTK : l'image reste enregistrée
        return False


def notify(title, body, icon="winlux-capture"):
    try:
        from gi.repository import Gio, GLib
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                      "org.freedesktop.Notifications", "Notify",
                      GLib.Variant("(susssasa{sv}i)", ("Winlux", 0, icon, title, body, [], {}, -1)),
                      None, Gio.DBusCallFlags.NONE, 2000, None)
    except Exception:  # pas de bus ni de serveur de notifications
        pass


def deliver(image):
    if image is None: return 0
    try:
        path = save(image)
    except Exception as error:
        notify(_t("Capture impossible : {error}").format(error=error), "", "dialog-error")
        return 1
    print(path, flush=True)
    # Notification d'abord : la copie sans xclip garde la main jusqu'au copier suivant.
    notify(_t("Capture copiée dans le Presse-papiers"), _t("Enregistrée dans {path}").format(path=path.parent))
    copy_image(path)
    return 0


def main(argv):
    args = argv[1:]
    if args == ["dossier"]:
        print(captures_dir())
        return 0
    if args and args[0] not in ("ecran", "fenetre"):
        print(__doc__.strip().split("\n\n")[2], file=sys.stderr)
        return 2
    shot = screen_shot()
    if shot is None: return 1
    if args == ["ecran"]: return deliver(shot)
    if args == ["fenetre"]:
        rect = active_rect()
        return deliver(crop(shot, rect) if rect else shot)
    return deliver(select(shot))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
