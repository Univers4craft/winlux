#!/usr/bin/python3
"""Copier le texte d'une image ou d'une partie de l'écran (reconnaissance de texte).

Windows + Maj + T fige l'écran : on encadre une zone à la souris, son texte
est copié dans le presse-papiers et montré dans une petite fenêtre où il peut
être retouché. « Copier le texte » de la visionneuse d'images, du clic droit
sur une image (Thunar) et de l'outil Capture appellent la même commande.

Le moteur (Tesseract, paquet Ubuntu) et ses données en français, anglais,
espagnol et allemand se téléchargent à la première utilisation, par le Store
(environ 15 Mo) : rien dans l'ISO. Paramètres › Système › Presse-papiers
montre l'état et permet de les retirer.

    winlux-ocr                 encadrer une zone de l'écran (Windows + Maj + T)
    winlux-ocr FICHIER         texte d'une image
    winlux-ocr text FICHIER    texte d'une image, écrit sur la sortie (tests)
    winlux-ocr status          installed=0|1, languages=fra,eng…
    winlux-ocr install         télécharge le moteur et les quatre langues
    winlux-ocr remove          retire le moteur et ses langues
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


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
    'Copier le texte': ('Copy text', 'Copiar texto', 'Text kopieren'),
    'Texte copié dans le presse-papiers.': ('Text copied to the clipboard.', 'Texto copiado en el portapapeles.', 'Text in die Zwischenablage kopiert.'),
    'Aucun texte trouvé dans cette image.': ('No text found in this image.', 'No se encontró texto en esta imagen.', 'In diesem Bild wurde kein Text gefunden.'),
    'Copier': ('Copy', 'Copiar', 'Kopieren'),
    'Fermer': ('Close', 'Cerrar', 'Schließen'),
    'Encadrez le texte à copier · Échap : annuler': ('Draw a box around the text to copy · Esc: cancel',
                                                    'Enmarque el texto que desea copiar · Esc: cancelar',
                                                    'Ziehen Sie einen Rahmen um den Text · Esc: abbrechen'),
    'La reconnaissance de texte n’est pas encore installée.': ('Text recognition is not installed yet.',
                                                               'El reconocimiento de texto aún no está instalado.',
                                                               'Die Texterkennung ist noch nicht installiert.'),
    'Télécharger (environ 15 Mo)': ('Download (about 15 MB)', 'Descargar (unos 15 MB)', 'Herunterladen (etwa 15 MB)'),
    'Téléchargement de la reconnaissance de texte…': ('Downloading text recognition…', 'Descargando el reconocimiento de texto…',
                                                     'Texterkennung wird heruntergeladen…'),
    'Téléchargement impossible pour l’instant.': ('Download not possible right now.', 'No se puede descargar ahora.',
                                                  'Download derzeit nicht möglich.'),
    'Lecture du texte…': ('Reading text…', 'Leyendo el texto…', 'Text wird gelesen…'),
    'Image illisible.': ('Unreadable image.', 'Imagen ilegible.', 'Bild nicht lesbar.'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# Langue de l'interface → données Tesseract ; la langue de l'interface passe en premier.
TESS_LANG = {"fr": "fra", "en": "eng", "es": "spa", "de": "deu"}
PACKAGES = ["tesseract-ocr", "tesseract-ocr-fra", "tesseract-ocr-spa", "tesseract-ocr-deu"]


def engine():
    return os.environ.get("WINLUX_OCR_ENGINE") or shutil.which("tesseract")


def installed_languages():
    tool = engine()
    if not tool: return []
    try:
        out = subprocess.run([tool, "--list-langs"], capture_output=True, text=True, timeout=20).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    return [line.strip() for line in out.splitlines()[1:] if re.fullmatch(r"[a-z_]{3,}", line.strip())]


def language_order(available, interface=_LANG):
    """'fra+eng+spa+deu' : la langue de l'interface d'abord, seulement les quatre langues prévues."""
    wanted = [TESS_LANG[interface]] + [code for code in TESS_LANG.values() if code != TESS_LANG[interface]]
    chosen = [code for code in wanted if code in available]
    return "+".join(chosen) or ("eng" if "eng" in available else "")


def clean(text):
    """Lignes sans espaces de fin, sans saut de page, au plus une ligne vide d'affilée."""
    lines = [line.rstrip() for line in text.replace("\f", "\n").splitlines()]
    out = []
    for line in lines:
        if not line and (not out or not out[-1]): continue
        out.append(line)
    while out and not out[-1]: out.pop()
    return "\n".join(out)


def recognize(path, languages=None):
    """Texte d'une image (chaîne vide si rien) ; None si le moteur manque ou échoue."""
    tool = engine()
    if not tool: return None
    languages = languages or language_order(installed_languages())
    args = [tool, str(path), "stdout", "--psm", "3"]
    if languages: args += ["-l", languages]
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.SubprocessError):
        return None
    return clean(result.stdout) if result.returncode == 0 else None


def prepared(path):
    """Copie agrandie (×2 sous 1500 px de large) : le texte d'écran est petit pour le moteur."""
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf, GLib
    try:
        image = GdkPixbuf.Pixbuf.new_from_file(str(path))
    except GLib.Error:
        return None
    return scaled_copy(image)


def scaled_copy(image):
    from gi.repository import GdkPixbuf
    if image.get_width() < 1500:
        image = image.scale_simple(image.get_width() * 2, image.get_height() * 2, GdkPixbuf.InterpType.BILINEAR)
    handle, name = tempfile.mkstemp(prefix="winlux-ocr-", suffix=".png")
    os.close(handle)
    image.savev(name, "png", [], [])
    return name


def status():
    languages = installed_languages()
    print(f"installed={int(bool(engine()))}")
    print("languages=" + ",".join(code for code in TESS_LANG.values() if code in languages))
    return 0


def install():
    store = shutil.which("winlux-store")
    if not store: return 1
    for package in PACKAGES:
        if subprocess.run([store, "install", "apt:" + package], stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL).returncode != 0:
            return 1
    return 0


def remove():
    store = shutil.which("winlux-store")
    if not store: return 1
    failed = 0
    for package in reversed(PACKAGES):
        failed |= subprocess.run([store, "remove", "apt:" + package], stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL).returncode
    return 1 if failed else 0


# ── Interfaces (GTK) ────────────────────────────────────────────────────────

def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    return Gdk, GLib, Gtk


def select_region():
    """Écran figé, zone tracée à la souris ; renvoie le GdkPixbuf de la zone ou None."""
    Gdk, GLib, Gtk = _gtk()
    root = Gdk.get_default_root_window()
    shot = Gdk.pixbuf_get_from_window(root, 0, 0, root.get_width(), root.get_height())
    if shot is None: return None
    window = Gtk.Window(type=Gtk.WindowType.POPUP)
    window.move(0, 0)
    window.resize(shot.get_width(), shot.get_height())
    area = Gtk.DrawingArea()
    area.add_events(Gdk.EventMask.POINTER_MOTION_MASK | Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.BUTTON_RELEASE_MASK)
    window.add(area)
    state = {"start": None, "end": None, "done": None}

    def rect():
        (x1, y1), (x2, y2) = state["start"], state["end"]
        return min(x1, x2), min(y1, y2), abs(x2 - x1), abs(y2 - y1)

    def draw(_, cr):
        Gdk.cairo_set_source_pixbuf(cr, shot, 0, 0)
        cr.paint()
        cr.set_source_rgba(0, 0, 0, 0.45)
        cr.rectangle(0, 0, shot.get_width(), shot.get_height())
        if state["start"] and state["end"]:
            x, y, w, h = rect()
            cr.rectangle(x, y + h, w, -h)      # sens inverse : la zone reste claire
        cr.fill()
        if state["start"] and state["end"]:
            x, y, w, h = rect()
            cr.set_source_rgb(0.51, 0.47, 0.72)
            cr.set_line_width(2)
            cr.rectangle(x, y, w, h)
            cr.stroke()
        else:
            label = _t("Encadrez le texte à copier · Échap : annuler")
            cr.set_font_size(16)
            extents = cr.text_extents(label)
            x = (shot.get_width() - extents.width) / 2
            cr.set_source_rgba(0.13, 0.14, 0.19, 0.9)
            cr.rectangle(x - 14, 30, extents.width + 28, 36)
            cr.fill()
            cr.set_source_rgb(0.93, 0.93, 0.96)
            cr.move_to(x, 54)
            cr.show_text(label)

    def pressed(_, event):
        if event.button != 1:
            Gtk.main_quit()
            return
        state["start"] = state["end"] = (int(event.x), int(event.y))

    def moved(_, event):
        if state["start"]:
            state["end"] = (int(event.x), int(event.y))
            area.queue_draw()

    def released(_, event):
        if event.button != 1 or not state["start"]: return
        state["end"] = (int(event.x), int(event.y))
        x, y, w, h = rect()
        if w >= 4 and h >= 4:
            state["done"] = shot.new_subpixbuf(x, y, w, h).copy()
        Gtk.main_quit()

    area.connect("draw", draw)
    area.connect("button-press-event", pressed)
    area.connect("motion-notify-event", moved)
    area.connect("button-release-event", released)
    window.connect("key-press-event", lambda _, e: e.keyval == Gdk.KEY_Escape and (Gtk.main_quit() or True))
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


def copy_text(text):
    Gdk, _, Gtk = _gtk()
    clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
    clipboard.set_text(text, -1)
    clipboard.store()


def result_window(image_path, temporary=False):
    """Lit le texte (ou propose le téléchargement), le copie et le montre."""
    Gdk, GLib, Gtk = _gtk()
    window = Gtk.Window(title=_t("Copier le texte"))
    window.set_role("winlux-ocr")
    window.set_icon_name("edit-copy")
    window.set_default_size(560, 360)
    window.set_keep_above(True)
    window.set_position(Gtk.WindowPosition.CENTER)
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=14)
    window.add(box)
    status_label = Gtk.Label(xalign=0, wrap=True)
    box.pack_start(status_label, False, False, 0)
    scroll = Gtk.ScrolledWindow()
    view = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, left_margin=8, right_margin=8, top_margin=6, bottom_margin=6)
    scroll.add(view)
    box.pack_start(scroll, True, True, 0)
    buttons = Gtk.Box(spacing=8)
    box.pack_start(buttons, False, False, 0)
    download = Gtk.Button(label=_t("Télécharger (environ 15 Mo)"))
    buttons.pack_start(download, False, False, 0)
    close = Gtk.Button(label=_t("Fermer"))
    copy = Gtk.Button(label=_t("Copier"))
    copy.get_style_context().add_class("suggested-action")
    buttons.pack_end(copy, False, False, 0)
    buttons.pack_end(close, False, False, 0)

    def copy_now(_=None):
        buffer = view.get_buffer()
        copy_text(buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False))
        status_label.set_text(_t("Texte copié dans le presse-papiers."))

    def read():
        if not engine():
            status_label.set_text(_t("La reconnaissance de texte n’est pas encore installée."))
            download.show()
            scroll.hide()
            copy.hide()
            return False
        download.hide()
        status_label.set_text(_t("Lecture du texte…"))
        while Gtk.events_pending(): Gtk.main_iteration()
        text = recognize(image_path)
        if text is None:
            status_label.set_text(_t("Image illisible."))
            return False
        if not text:
            status_label.set_text(_t("Aucun texte trouvé dans cette image."))
            return False
        view.get_buffer().set_text(text)
        scroll.show()
        copy.show()
        copy_now()
        return False

    def start_download(_):
        download.set_sensitive(False)
        status_label.set_text(_t("Téléchargement de la reconnaissance de texte…"))
        try:
            process = subprocess.Popen([sys.executable, os.path.abspath(__file__), "install"],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError:
            status_label.set_text(_t("Téléchargement impossible pour l’instant."))
            return

        def wait():
            if process.poll() is None: return True
            download.set_sensitive(True)
            if process.returncode == 0 and engine(): read()
            else: status_label.set_text(_t("Téléchargement impossible pour l’instant."))
            return False
        GLib.timeout_add(500, wait)

    def finish(_):
        if temporary:
            try: os.unlink(image_path)
            except OSError: pass
        Gtk.main_quit()

    copy.connect("clicked", copy_now)
    close.connect("clicked", lambda _: window.destroy())
    download.connect("clicked", start_download)
    window.connect("destroy", finish)
    window.connect("key-press-event", lambda _, e: e.keyval == Gdk.KEY_Escape and (window.destroy() or True))
    window.show_all()
    GLib.idle_add(read)
    Gtk.main()
    return 0


def linux_path(value):
    """C:\\ est le dossier personnel, Z:\\ la racine (comme le runtime) ; file:// accepté."""
    if value.startswith("file://"):
        from urllib.parse import unquote
        return unquote(value[7:])
    if len(value) >= 2 and value[1] == ":" and value[0].isalpha():
        rest = value[2:].replace("\\", "/").lstrip("/")
        if value[0].upper() == "Z": return "/" + rest
        if value[0].upper() == "C": return str(Path.home() / rest)
    return value


def main(argv):
    args = argv[1:]
    if args == ["status"]: return status()
    if args == ["install"]: return install()
    if args == ["remove"]: return remove()
    if len(args) == 2 and args[0] == "text":
        text = recognize(args[1])
        if text is None: return 1
        print(text)
        return 0
    if not args:
        region = select_region()
        if region is None: return 0
        return result_window(scaled_copy(region), temporary=True)
    if len(args) == 1 and not args[0].startswith("-"):
        path = linux_path(args[0])
        if not Path(path).is_file(): return 2
        image = prepared(path)
        return result_window(image or path, temporary=bool(image))
    print(__doc__.strip().split("\n\n")[3], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
