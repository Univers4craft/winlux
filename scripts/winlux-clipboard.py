#!/usr/bin/python3
"""Historique du presse-papiers (Windows + V).

Un seul processus par session garde l'historique : les 25 derniers éléments
copiés (texte et images), que ce soit dans une application Linux ou dans un
programme Windows (le pont du presse-papiers, #161, les fait passer par le
presse-papiers X). Les éléments épinglés restent, même après « Effacer tout »
et après un redémarrage. Windows + V ouvre la liste près du pointeur ; un clic
remet l'élément dans le presse-papiers et le colle dans la fenêtre active.
Les copies marquées « mot de passe » par un gestionnaire de mots de passe ne
sont jamais retenues.

    winlux-clipboard daemon         surveille le presse-papiers (avec la session)
    winlux-clipboard show           ouvre la liste (Windows + V)
    winlux-clipboard status         état (clé=valeur)
    winlux-clipboard set HISTORY on|off
    winlux-clipboard clear          efface tout sauf les éléments épinglés
    winlux-clipboard list           éléments, un par ligne (tests)
    winlux-clipboard add-text TEXTE ajoute un élément (tests)

Réglage : ~/.config/winlux/clipboard.conf ; historique :
~/.local/state/winlux/clipboard/ (history.json et images PNG).
"""
import hashlib
import json
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
# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Presse-papiers': ('Clipboard', 'Portapapeles', 'Zwischenablage'),
    'Effacer tout': ('Clear all', 'Borrar todo', 'Alle löschen'),
    'Épingler': ('Pin', 'Anclar', 'Anheften'),
    'Désépingler': ('Unpin', 'Desanclar', 'Lösen'),
    'Supprimer': ('Delete', 'Eliminar', 'Löschen'),
    'Rien ici pour l’instant': ('Nothing here yet', 'Aún no hay nada', 'Noch nichts vorhanden'),
    'Copiez du texte ou une image : il apparaîtra ici.': ('Copy text or an image and it will appear here.', 'Copie texto o una imagen y aparecerá aquí.', 'Kopieren Sie Text oder ein Bild, dann erscheint es hier.'),
    'L’historique du presse-papiers est désactivé.': ('Clipboard history is turned off.', 'El historial del portapapeles está desactivado.', 'Der Zwischenablageverlauf ist deaktiviert.'),
    'Activer': ('Turn on', 'Activar', 'Einschalten'),
    'Image': ('Image', 'Imagen', 'Bild'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


MAX_ITEMS = 25
MAX_TEXT = 1 << 20          # 1 Mio de texte par élément
MAX_IMAGE_PIXELS = 4096 * 4096
PASSWORD_HINTS = ("x-kde-passwordManagerHint",)


def config_path():
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "winlux" / "clipboard.conf"


def store_dir():
    return Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state") / "winlux" / "clipboard"


def enabled():
    try:
        for line in config_path().read_text(encoding="utf-8").splitlines():
            if line.strip() == "HISTORY=off": return False
    except OSError:
        pass
    return True


def set_enabled(on):
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"HISTORY={'on' if on else 'off'}\n", encoding="utf-8")


def load():
    try:
        items = json.loads((store_dir() / "history.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return []
    return [item for item in items if isinstance(item, dict) and item.get("kind") in ("text", "image")]


def save(items):
    folder = store_dir()
    folder.mkdir(parents=True, exist_ok=True)
    os.chmod(folder, 0o700)
    temporary = folder / "history.json.tmp"
    temporary.write_text(json.dumps(items, ensure_ascii=False), encoding="utf-8")
    os.replace(temporary, folder / "history.json")
    # Images qui ne sont plus référencées : supprimées.
    keep = {item.get("file") for item in items if item.get("kind") == "image"}
    for image in folder.glob("*.png"):
        if image.name not in keep:
            try: image.unlink()
            except OSError: pass


def trim(items):
    """25 éléments au plus ; les épinglés ne sont jamais retirés."""
    pinned = sum(1 for item in items if item.get("pinned"))
    result, unpinned = [], 0
    for item in items:
        if item.get("pinned"):
            result.append(item)
        elif unpinned < max(0, MAX_ITEMS - pinned):
            result.append(item)
            unpinned += 1
    return result


def add(items, item):
    """Nouvel élément en tête ; un doublon remonte (et garde son épingle)."""
    same = next((old for old in items if old.get("key") == item["key"]), None)
    if same is not None:
        items.remove(same)
        item["pinned"] = same.get("pinned", False)
        if same.get("kind") == "image": item["file"] = same.get("file")
    items.insert(0, item)
    return trim(items)


def text_item(text):
    return {"kind": "text", "text": text[:MAX_TEXT], "key": "t:" + hashlib.sha256(text.encode("utf-8", "replace")).hexdigest(),
            "time": int(time.time()), "pinned": False}


def clear(items):
    return [item for item in items if item.get("pinned")]


def toggle_pin(items, key):
    for item in items:
        if item.get("key") == key: item["pinned"] = not item.get("pinned", False)
    return items


def remove(items, key):
    return [item for item in items if item.get("key") != key]


def preview(text, lines=3, width=60):
    rows = [row.strip() for row in text.strip().splitlines() if row.strip()][:lines]
    shown = "\n".join(row if len(row) <= width else row[:width - 1] + "…" for row in rows)
    return shown or text[:width]


# ── Interface (GTK) ─────────────────────────────────────────────────────────

def run_app(argv):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GdkPixbuf, Gio, GLib, Gtk

    class History(Gtk.Application):
        def __init__(self):
            super().__init__(application_id="org.winlux.ClipboardHistory",
                             flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE)
            self.window = None
            self.target = ""
            self.ignore_until = 0.0

        def do_startup(self):
            Gtk.Application.do_startup(self)
            self.hold()   # reste en vie pour surveiller le presse-papiers
            self.clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
            self.clipboard.connect("owner-change", self.changed)

        def do_command_line(self, line):
            args = line.get_arguments()[1:]
            if args and args[0] == "show": self.show()
            return 0

        # ── Surveillance ──

        def changed(self, *_):
            if not enabled() or time.monotonic() < self.ignore_until: return
            self.clipboard.request_targets(self.got_targets)

        def got_targets(self, _, atoms, *rest):
            names = [atom.name() for atom in (atoms or [])]
            if any(hint in names for hint in PASSWORD_HINTS): return
            if any(name.startswith("image/") for name in names) and not any(name in ("UTF8_STRING", "text/plain;charset=utf-8") for name in names):
                self.clipboard.request_image(self.got_image)
            else:
                self.clipboard.request_text(self.got_text)

        def got_text(self, _, text, *rest):
            if not text or not text.strip(): return
            save(add(load(), text_item(text)))
            self.refresh()

        def got_image(self, _, pixbuf, *rest):
            if pixbuf is None or pixbuf.get_width() * pixbuf.get_height() > MAX_IMAGE_PIXELS: return
            raw = pixbuf.get_pixels()
            key = "i:" + hashlib.sha256(raw).hexdigest()
            folder = store_dir()
            folder.mkdir(parents=True, exist_ok=True)
            name = key[2:18] + ".png"
            try: pixbuf.savev(str(folder / name), "png", [], [])
            except GLib.Error: return
            save(add(load(), {"kind": "image", "file": name, "key": key, "time": int(time.time()),
                              "width": pixbuf.get_width(), "height": pixbuf.get_height(), "pinned": False}))
            self.refresh()

        # ── Liste (Windows + V) ──

        def show(self):
            self.target = active_window()
            if self.window is None:
                self.window = Gtk.ApplicationWindow(application=self, title=_t("Presse-papiers"))
                self.window.set_role("winlux-clipboard")
                self.window.set_decorated(False)
                self.window.set_keep_above(True)
                self.window.set_skip_taskbar_hint(True)
                self.window.set_default_size(380, 460)
                self.window.connect("delete-event", lambda *_: self.window.hide() or True)
                self.window.connect("key-press-event", self.key)
                self.window.connect("focus-out-event", lambda *_: GLib.timeout_add(150, self.lost_focus) and False)
                css = Gtk.CssProvider()
                css.load_from_data(b"""
                  window { background: #202430; color: #ecedf5; border: 1px solid #3c4458; border-radius: 8px; }
                  .card { background: #2a3040; border-radius: 6px; padding: 10px; }
                  .card:hover { background: #343b4e; }
                  .heading { font-weight: bold; font-size: 15px; }
                  .subtitle { color: #adb6c9; }
                  button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; box-shadow: none; }
                  button.flat { background: transparent; border: none; padding: 2px 6px; }
                  button.flat:hover { background: #414b64; }
                  button.pinned { color: #b8b0ff; }
                """)
                Gtk.StyleContext.add_provider_for_screen(self.window.get_screen(), css, 600)
                outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=12)
                self.window.add(outer)
                head = Gtk.Box(spacing=8)
                title = Gtk.Label(label=_t("Presse-papiers"), xalign=0)
                title.get_style_context().add_class("heading")
                head.pack_start(title, True, True, 0)
                self.clear_button = Gtk.Button(label=_t("Effacer tout"))
                self.clear_button.connect("clicked", lambda *_: (save(clear(load())), self.refresh()))
                head.pack_start(self.clear_button, False, False, 0)
                outer.pack_start(head, False, False, 0)
                scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
                outer.pack_start(scroll, True, True, 0)
                self.list = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
                scroll.add(self.list)
            self.refresh()
            place_near_pointer(self.window)
            self.window.show_all()
            self.window.present()

        def refresh(self):
            if self.window is None: return
            for child in self.list.get_children(): child.destroy()
            if not enabled():
                self.message(_t("L’historique du presse-papiers est désactivé."), None)
                turn_on = Gtk.Button(label=_t("Activer"), halign=Gtk.Align.START)
                turn_on.connect("clicked", lambda *_: (set_enabled(True), self.refresh()))
                self.list.pack_start(turn_on, False, False, 0)
                self.clear_button.set_sensitive(False)
                self.list.show_all()
                return
            items = load()
            self.clear_button.set_sensitive(any(not item.get("pinned") for item in items))
            if not items:
                self.message(_t("Rien ici pour l’instant"), _t("Copiez du texte ou une image : il apparaîtra ici."))
            for item in items:
                self.list.pack_start(self.card(item), False, False, 0)
            self.list.show_all()

        def message(self, title, text):
            label = Gtk.Label(label=title, xalign=0, wrap=True)
            label.get_style_context().add_class("heading")
            self.list.pack_start(label, False, False, 0)
            if text:
                hint = Gtk.Label(label=text, xalign=0, wrap=True)
                hint.get_style_context().add_class("subtitle")
                self.list.pack_start(hint, False, False, 0)

        def card(self, item):
            event = Gtk.EventBox()
            box = Gtk.Box(spacing=8)
            box.get_style_context().add_class("card")
            event.add(box)
            if item["kind"] == "image":
                try:
                    width, height = item.get("width") or 280, item.get("height") or 120
                    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(store_dir() / item["file"]),
                                                                     min(280, width), min(120, height), True)
                    content = Gtk.Image.new_from_pixbuf(pixbuf)
                except GLib.Error:
                    content = Gtk.Label(label=_t("Image"), xalign=0)
                content.set_halign(Gtk.Align.START)
            else:
                content = Gtk.Label(label=preview(item["text"]), xalign=0, wrap=True, max_width_chars=40)
                content.set_line_wrap_mode(2)   # PANGO_WRAP_WORD_CHAR
            box.pack_start(content, True, True, 0)
            tools = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.START)
            # Icônes du thème, pas des emojis : sans police d'emojis en couleur
            # (absente de l'image), 📌 et 🗑 s'affichaient en codes « 01F4CD ».
            pin = Gtk.Button.new_from_icon_name("starred-symbolic" if item.get("pinned") else "non-starred-symbolic",
                                                Gtk.IconSize.BUTTON)
            pin.get_style_context().add_class("flat")
            if item.get("pinned"): pin.get_style_context().add_class("pinned")
            pin.set_tooltip_text(_t("Désépingler") if item.get("pinned") else _t("Épingler"))
            pin.connect("clicked", lambda *_, k=item["key"]: (save(toggle_pin(load(), k)), self.refresh()))
            delete = Gtk.Button.new_from_icon_name("user-trash-symbolic", Gtk.IconSize.BUTTON)
            delete.get_style_context().add_class("flat")
            delete.set_tooltip_text(_t("Supprimer"))
            delete.connect("clicked", lambda *_, k=item["key"]: (save(remove(load(), k)), self.refresh()))
            tools.pack_start(pin, False, False, 0)
            tools.pack_start(delete, False, False, 0)
            box.pack_start(tools, False, False, 0)
            event.connect("button-release-event", lambda *_, chosen=item: self.paste(chosen) or True)
            return event

        def paste(self, item):
            # Remis dans le presse-papiers (sans se réenregistrer), puis collé dans la fenêtre d'avant.
            self.ignore_until = time.monotonic() + 0.5
            if item["kind"] == "image":
                try: self.clipboard.set_image(GdkPixbuf.Pixbuf.new_from_file(str(store_dir() / item["file"])))
                except GLib.Error: return
            else:
                self.clipboard.set_text(item["text"], -1)
            self.clipboard.store()
            save(add(load(), dict(item, time=int(time.time()))))
            self.window.hide()
            GLib.timeout_add(80, self.send_paste)

        def send_paste(self):
            tool = shutil.which("xdotool")
            if tool:
                if self.target:
                    subprocess.run([tool, "windowactivate", "--sync", self.target], capture_output=True, timeout=3)
                subprocess.run([tool, "key", "--clearmodifiers", "ctrl+v"], capture_output=True, timeout=3)
            return False

        def key(self, _, event):
            if event.keyval == Gdk.KEY_Escape:
                self.window.hide()
                return True
            return False

        def lost_focus(self):
            if self.window is not None and not self.window.is_active(): self.window.hide()
            return False

    def place_near_pointer(window):
        display = Gdk.Display.get_default()
        seat = display.get_default_seat() if display else None
        if seat is None: return
        _, x, y = seat.get_pointer().get_position()
        area = display.get_monitor_at_point(x, y).get_workarea()
        width, height = window.get_size()
        window.move(max(area.x, min(x - width // 2, area.x + area.width - width)),
                    max(area.y, min(y + 16, area.y + area.height - height)))

    return History().run(argv)


def active_window():
    tool = shutil.which("xdotool")
    if not tool: return ""
    try:
        result = subprocess.run([tool, "getactivewindow"], capture_output=True, text=True, timeout=3)
    except (OSError, subprocess.SubprocessError):
        return ""
    return result.stdout.strip() if result.returncode == 0 else ""


def main(argv):
    command = argv[1] if len(argv) > 1 else "show"
    if command == "status":
        items = load()
        print(f"history={'on' if enabled() else 'off'}\nitems={len(items)}\npinned={sum(1 for i in items if i.get('pinned'))}")
        return 0
    if command == "set" and len(argv) == 4 and argv[2].upper() == "HISTORY" and argv[3] in ("on", "off"):
        set_enabled(argv[3] == "on")
        return 0
    if command == "clear":
        save(clear(load()))
        return 0
    if command == "list":
        for item in load():
            shown = item["text"].replace("\n", "\\n") if item["kind"] == "text" else f"[image {item.get('width')}x{item.get('height')}]"
            print(("* " if item.get("pinned") else "  ") + shown)
        return 0
    if command == "add-text" and len(argv) == 3:
        save(add(load(), text_item(argv[2])))
        return 0
    if command == "pin" and len(argv) == 3:
        items = load()
        match = next((item for item in items if item["kind"] == "text" and item["text"] == argv[2]), None)
        if match is None: return 1
        save(toggle_pin(items, match["key"]))
        return 0
    if command in ("daemon", "show"):
        return run_app([argv[0]] + ([] if command == "daemon" else ["show"]))
    print(__doc__.strip().split("\n\n")[-2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
