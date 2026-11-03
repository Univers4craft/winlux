#!/usr/bin/python3
"""Clavier tactile Winlux, façon Windows 11.

Utilisable à la souris ou au doigt : dispositions française (AZERTY),
anglaise (QWERTY), allemande (QWERTZ) et espagnole (QWERTY avec Ñ), page de
chiffres et symboles, accents par appui long (é è ê ë…), touches Windows,
Ctrl, Alt et Maj « collantes » pour les combinaisons (Ctrl + C, Windows + E,
Alt + Tab…). La fenêtre ne prend jamais le focus : les touches vont à la
fenêtre où l'on tapait, par xdotool.

    winlux-osk               ouvre ou ferme le clavier (icône de la barre)
    winlux-osk show|hide     l'ouvre ou le ferme
    winlux-osk need          l'ouvre seulement sans clavier physique (champ de mot de passe)
    winlux-osk detect        keyboard=1 si un clavier physique est branché
    winlux-osk icon          icon=1 si l'icône de la barre doit apparaître

    winlux-osk set auto|always|never   réglage de l'icône (Paramètres)
    winlux-osk tray          sortie genmon de l'icône (greffon n° 12)
    winlux-osk panel         ajoute ou retire le greffon de la barre en cours

Réglage (Paramètres > Clavier, souris et pavé tactile) : OSK_ICON=auto|always|never
dans ~/.config/winlux/osk.conf ; « auto » montre l'icône sans clavier
physique (un clavier branché ou débranché est suivi toutes les 5 s).
Le widget Keyboard sert aussi à l'écran de verrouillage (winlux-lock), qui lui
passe sa propre fonction d'envoi.

WINLUX_OSK_ROOT déplace /proc et /etc (tests).
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(os.environ.get("WINLUX_OSK_ROOT", "/"))


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in (ROOT / "etc/winlux/locale.conf").read_text(encoding="utf-8").splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    "Clavier tactile": ("Touch keyboard", "Teclado táctil", "Bildschirmtastatur"),
    "Espace": ("Space", "Espacio", "Leertaste"),
    "Entrée": ("Enter", "Intro", "Eingabe"),
    "Échap": ("Esc", "Esc", "Esc"),
    "Fermer le clavier": ("Close the keyboard", "Cerrar el teclado", "Tastatur schließen"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── Détection d'un clavier physique ────────────────────────────────────────

# Lettres Q à P (16-25), A à L (30-38), Z à M (44-50) : un vrai clavier les a
# toutes ; un bouton d'alimentation, une télécommande ou des touches de
# volume n'en ont aucune.
LETTER_CODES = list(range(16, 26)) + list(range(30, 39)) + list(range(44, 51))
NOT_KEYBOARDS = ("xdotool", "virtual", "power button", "sleep button", "video bus", "lid switch",
                 "consumer control", "system control", "hotkeys", "wmi", "headset", "headphone")


def _has_letters(bitmap):
    words = bitmap.split()
    try:
        value = 0
        for word in words:
            value = (value << 64) | int(word, 16)
    except ValueError:
        return False
    return all(value >> code & 1 for code in LETTER_CODES)


def keyboards():
    """Noms des claviers physiques (d'après /proc/bus/input/devices)."""
    try:
        text = (ROOT / "proc/bus/input/devices").read_text(errors="replace")
    except OSError:
        return []
    found = []
    for block in text.split("\n\n"):
        name, handlers, keys = "", "", ""
        for line in block.splitlines():
            if line.startswith("N: Name="):
                name = line[8:].strip().strip('"')
            elif line.startswith("H: Handlers="):
                handlers = line[12:]
            elif line.startswith("B: KEY="):
                keys = line[7:]
        if "kbd" not in handlers.split() or not keys:
            continue
        if any(word in name.lower() for word in NOT_KEYBOARDS):
            continue
        if _has_letters(keys):
            found.append(name)
    return found


def config_file():
    home = Path(os.environ.get("HOME") or Path.home())
    return Path(os.environ.get("XDG_CONFIG_HOME") or home / ".config") / "winlux/osk.conf"


def setting():
    try:
        for line in config_file().read_text().splitlines():
            if line.startswith("OSK_ICON="):
                value = line[9:].strip()
                if value in ("auto", "always", "never"):
                    return value
    except OSError:
        pass
    return "auto"


def save_setting(value):
    if value not in ("auto", "always", "never"):
        raise SystemExit(2)
    path = config_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(f"OSK_ICON={value}\n")
    temporary.replace(path)


def icon_wanted():
    mode = setting()
    return mode == "always" or (mode == "auto" and not keyboards())


def layout_name():
    """Disposition du clavier choisie dans Paramètres (XKB_LAYOUT de input.conf)."""
    try:
        for line in (ROOT / "etc/winlux/input.conf").read_text().splitlines():
            key, _, value = line.partition("=")
            if key in ("XKB_LAYOUT", "KEYBOARD"):
                value = {"be": "fr", "gb": "us"}.get(value.strip()[:2], value.strip()[:2])
                if value in LAYOUTS:
                    return value
    except OSError:
        pass
    return "us" if _LANG == "en" else _LANG


# ── Dispositions ───────────────────────────────────────────────────────────

LAYOUTS = {
    "fr": ["azertyuiop", "qsdfghjklm", "wxcvbn,;:!"],
    "us": ["qwertyuiop", "asdfghjkl'", "zxcvbnm,.?"],
    "de": ["qwertzuiopü", "asdfghjklöä", "yxcvbnm,.-"],
    "es": ["qwertyuiop", "asdfghjklñ", "zxcvbnm,.-"],
}
LAYOUTS["en"] = LAYOUTS["us"]
SYMBOLS = ["1234567890", "@#€&_-()=%", "\"*'/:;!?+<", "[]{}\\|~^$>"]
ACCENTS = {"a": "àâäáã", "e": "éèêë€", "i": "îïíì", "o": "ôöóòõ", "u": "ùûüú", "c": "ç", "n": "ñ",
           "s": "ß", "y": "ÿý", "?": "¿", "!": "¡", "-": "–—", "'": "’«»"}
# Noms de touches X (keysym) des caractères qu'xdotool ne prend pas tels quels.
KEYSYMS = {" ": "space", ",": "comma", ".": "period", ";": "semicolon", ":": "colon", "!": "exclam",
           "?": "question", "'": "apostrophe", "-": "minus", "_": "underscore", "/": "slash",
           "\\": "backslash", "+": "plus", "=": "equal", "*": "asterisk", "(": "parenleft",
           ")": "parenright", "[": "bracketleft", "]": "bracketright", "{": "braceleft",
           "}": "braceright", "<": "less", ">": "greater", "@": "at", "#": "numbersign",
           "&": "ampersand", "%": "percent", "$": "dollar", "\"": "quotedbl", "|": "bar",
           "~": "asciitilde", "^": "asciicircum", "€": "EuroSign"}


def keysym(char):
    if char.isascii() and char.isalnum():
        return char
    return KEYSYMS.get(char, f"U{ord(char):04X}")


def xdotool_args(key, modifiers):
    """Commande xdotool d'une touche : caractère simple (« type ») ou
    combinaison/touche spéciale (« key »)."""
    if len(key) == 1 and not modifiers:
        return ["xdotool", "type", "--clearmodifiers", "--", key]
    name = keysym(key) if len(key) == 1 else key
    return ["xdotool", "key", "--clearmodifiers", "+".join(list(modifiers) + [name])]


def send_x11(key, modifiers):
    if shutil.which("xdotool"):
        subprocess.Popen(xdotool_args(key, modifiers), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


# ── Widget ─────────────────────────────────────────────────────────────────

def keyboard_widget(send=send_x11, layout=None, on_close=None):
    """Clavier tactile prêt à placer dans une fenêtre. send(touche, modificateurs)
    reçoit un caractère (« é ») ou un nom X (« BackSpace ») et la liste des
    modificateurs X tenus (ctrl, alt, super, shift)."""
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, GLib, Gtk

    install_css()
    letters = LAYOUTS.get(layout or layout_name(), LAYOUTS["fr"])
    state = {"shift": False, "caps": False, "symbols": False, "ctrl": False, "alt": False, "super": False}
    grid = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
    grid.set_name("winlux-osk")
    grid.get_style_context().add_class("winlux-osk")
    rows_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
    grid.pack_start(rows_box, True, True, 0)
    sticky_buttons = {}

    def release_sticky():
        changed = False
        for mod in ("ctrl", "alt", "super"):
            if state[mod]:
                state[mod] = False
                changed = True
        if state["shift"] and not state["caps"]:
            state["shift"] = False
            changed = True
        if changed:
            rebuild()

    def press_char(char):
        mods = [m for m in ("ctrl", "alt", "super") if state[m]]
        if mods and state["shift"]:
            mods.append("shift")
        if mods:
            send(char.lower() if char.isalpha() else char, mods)
        else:
            send(char, [])
        release_sticky()

    def press_special(name):
        mods = [m for m in ("ctrl", "alt", "super") if state[m]]
        if state["shift"] and name not in ("BackSpace",):
            mods.append("shift")
        send(name, mods)
        release_sticky()

    def toggle(mod):
        if mod == "super" and state["super"]:
            # Windows seul (sans autre touche) : ouvre le menu Démarrer.
            state["super"] = False
            send("Super_L", [])
        else:
            state[mod] = not state[mod]
        rebuild()

    def accent_popup(button, base):
        variants = ACCENTS.get(base.lower(), "")
        if not variants:
            return False
        popover = Gtk.Popover.new(button)
        line = Gtk.Box(spacing=4)
        for variant in variants:
            shown = variant.upper() if base.isupper() else variant
            item = Gtk.Button(label=shown)
            item.set_can_focus(False)
            item.get_style_context().add_class("osk-key")
            item.set_size_request(48, 48)
            item.connect("clicked", lambda _b, v=shown: (popover.popdown(), press_char(v)))
            line.pack_start(item, False, False, 0)
        popover.add(line)
        line.show_all()
        popover.popup()
        return True

    def char_key(char):
        shown = char.upper() if (state["shift"] or state["caps"]) and char.isalpha() else char
        button = Gtk.Button(label=shown)
        button.set_can_focus(False)
        button.get_style_context().add_class("osk-key")
        timer = {}

        def down(_button, event):
            # Frappes rapides : un deuxième appui en moins de 400 ms envoie aussi
            # un « double clic » (2BUTTON_PRESS). Il ne doit pas relancer de minuterie,
            # sinon la première, orpheline, ouvrait les accents et mangeait la suite.
            if event.type != Gdk.EventType.BUTTON_PRESS:
                return True
            if timer.get("id"):
                GLib.source_remove(timer["id"])
            timer["long"] = False
            timer["id"] = GLib.timeout_add(450, lambda: timer.update(long=accent_popup(button, shown), id=None) or False)
            return False

        def up(*_):
            if timer.get("id"):
                GLib.source_remove(timer["id"])
                timer["id"] = None
            if not timer.get("long"):
                press_char(shown)
            return True
        button.connect("button-press-event", down)
        button.connect("button-release-event", up)
        button.connect("touch-event", lambda *_: False)
        return button

    def special(label, action, width=1.5, active=False, tooltip=None):
        button = Gtk.Button(label=label)
        button.set_can_focus(False)
        context = button.get_style_context()
        context.add_class("osk-key")
        context.add_class("osk-special")
        if active:
            context.add_class("osk-active")
        if tooltip:
            button.set_tooltip_text(tooltip)
        button.connect("clicked", lambda *_: action())
        button.osk_width = width
        return button

    def row(widgets):
        # Grille de quarts de touche : chaque touche garde sa largeur relative
        # (Espace six fois une lettre) quelle que soit la largeur de l'écran.
        line = Gtk.Grid(column_homogeneous=True, column_spacing=6, hexpand=True, vexpand=True)
        column = 0
        for widget in widgets:
            span = max(1, round(getattr(widget, "osk_width", 1) * 4))
            widget.set_size_request(-1, 44)
            widget.set_hexpand(True)
            widget.set_vexpand(True)
            line.attach(widget, column, 0, span, 1)
            column += span
        rows_box.pack_start(line, True, True, 0)

    def rebuild():
        for child in rows_box.get_children():
            rows_box.remove(child)
        if state["symbols"]:
            top, middle, bottom, extra = SYMBOLS
            row([special("Tab", lambda: press_special("Tab"))] + [char_key(c) for c in top] +
                [special("⌫", lambda: press_special("BackSpace"), tooltip="Backspace")])
            row([char_key(c) for c in middle] + [special("↵", lambda: press_special("Return"), 2, tooltip=_t("Entrée"))])
            row([char_key(c) for c in bottom])
            row([char_key(c) for c in extra])
        else:
            row([char_key(c) for c in "1234567890"] + [special("⌫", lambda: press_special("BackSpace"), tooltip="Backspace")])
            row([special("Tab", lambda: press_special("Tab"))] + [char_key(c) for c in letters[0]])
            row([special("⇪", lambda: (state.update(caps=not state["caps"], shift=not state["caps"]), rebuild()),
                         1.6, state["caps"], "Caps Lock")]
                + [char_key(c) for c in letters[1]]
                + [special("↵", lambda: press_special("Return"), 2, tooltip=_t("Entrée"))])
            row([special("⇧", lambda: toggle("shift"), 2, state["shift"], "Shift")]
                + [char_key(c) for c in letters[2]]
                + [special("⇧", lambda: toggle("shift"), 1.5, state["shift"], "Shift")])
        bottom_row = [
            special("&123" if not state["symbols"] else "abc",
                    lambda: (state.update(symbols=not state["symbols"]), rebuild()), 1.5),
            special("Ctrl", lambda: toggle("ctrl"), 1.3, state["ctrl"]),
            special("⊞", lambda: toggle("super"), 1.3, state["super"], "Windows"),
            special("Alt", lambda: toggle("alt"), 1.3, state["alt"]),
            special(_t("Espace"), lambda: press_char(" "), 6),
            special("←", lambda: press_special("Left"), 1),
            special("↑", lambda: press_special("Up"), 1),
            special("↓", lambda: press_special("Down"), 1),
            special("→", lambda: press_special("Right"), 1),
            special(_t("Échap"), lambda: press_special("Escape"), 1.3),
        ]
        if on_close is not None:
            bottom_row.append(special("⌨✕", on_close, 1.3, tooltip=_t("Fermer le clavier")))
        row(bottom_row)
        rows_box.show_all()
        sticky_buttons.clear()

    rebuild()
    grid.osk_state = state
    grid.osk_press_char = press_char
    grid.osk_press_special = press_special
    grid.osk_toggle = toggle
    return grid


CSS = b"""
.winlux-osk { background: #1f2330; padding: 8px; }
.osk-key { background: #353b4d; color: #f2f3f8; border: none; border-radius: 6px; font-size: 17px;
           box-shadow: none; background-image: none; text-shadow: none; padding: 0; }
.osk-key label { color: #f2f3f8; }
.osk-key:hover { background: #454c62; }
.osk-key:active { background: #5a6280; }
.osk-special { background: #2a2f3e; font-size: 15px; }
.osk-active, .osk-active label { background: #8178b7; color: #ffffff; }
"""


_CSS_DONE = []


def install_css():
    """Style du clavier, une fois par écran (fenêtre du clavier ou écran de connexion)."""
    if _CSS_DONE:
        return
    from gi.repository import Gdk, Gtk
    screen = Gdk.Screen.get_default()
    if screen is None:
        return
    provider = Gtk.CssProvider()
    provider.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(screen, provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION + 1)
    _CSS_DONE.append(provider)


def run_window(start_hidden=False):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, Gio, Gtk

    class App(Gtk.Application):
        def __init__(self):
            super().__init__(application_id="org.winlux.Osk", flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE)
            self.window = None

        def ensure(self):
            if self.window is not None:
                return self.window
            window = Gtk.ApplicationWindow(application=self, title=_t("Clavier tactile"))
            window.set_icon_name("input-keyboard")
            window.set_accept_focus(False)
            window.set_focus_on_map(False)
            window.set_keep_above(True)
            window.set_skip_taskbar_hint(True)
            window.set_skip_pager_hint(True)
            window.set_type_hint(Gdk.WindowTypeHint.UTILITY)
            window.set_decorated(False)
            window.stick()
            window.add(keyboard_widget(on_close=lambda: window.hide()))
            window.connect("delete-event", lambda *_: window.hide() or True)
            monitor = Gdk.Display.get_default().get_primary_monitor() or Gdk.Display.get_default().get_monitor(0)
            area = monitor.get_workarea()
            height = max(240, min(int(area.height * 0.38), 360))
            window.set_default_size(area.width, height)
            window.move(area.x, area.y + area.height - height)
            self.window = window
            return window

        def do_command_line(self, command_line):
            args = command_line.get_arguments()[1:]
            verb = args[0] if args else "toggle"
            window = self.ensure()
            if verb == "hide" or (verb == "toggle" and window.get_visible()):
                window.hide()
            else:
                window.show_all()
                window.present_with_time(0)
            self.hold() if not getattr(self, "_held", False) else None
            self._held = True
            return 0

    return App().run(sys.argv[:1] + (["hide"] if start_hidden else sys.argv[1:]))


# ── Icône de la barre des tâches (greffon genmon n° 12) ────────────────────

PLUGIN = 12
ICON = ('<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">'
        '<rect x="2" y="6" width="20" height="12" rx="2" fill="none" stroke="#e7ddf5" stroke-width="1.5"/>'
        '<path d="M5.5 9.5h1m2.5 0h1m2.5 0h1m2.5 0h1m2.5 0h0M5.5 12h1m2.5 0h1m2.5 0h1m2.5 0h1M8 15h8" '
        'stroke="#e7ddf5" stroke-width="1.5" stroke-linecap="round"/></svg>')


def tray_icon():
    for directory in (Path("/run/winlux/tray"),
                      Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "winlux/tray"):
        try:
            directory.mkdir(parents=True, exist_ok=True)
            icon = directory / "touch-keyboard.svg"
            if not icon.exists() or icon.read_text() != ICON:
                icon.write_text(ICON)
            return icon
        except OSError:
            continue
    return None


def tray_output():
    """Sortie genmon : l'icône quand elle est voulue, rien sinon (greffon vide)."""
    icon = tray_icon() if icon_wanted() else None
    if icon is None:
        return "<txt></txt>"
    command = "/usr/bin/winlux-osk" if Path("/usr/bin/winlux-osk").exists() else f"python3 {Path(__file__).resolve()}"
    return f"<img>{icon}</img><tool>{_t('Clavier tactile')}</tool><click>{command} toggle</click>"


def _xfconf(*args):
    try:
        result = subprocess.run(["xfconf-query", "-c", "xfce4-panel", *args], capture_output=True, text=True, timeout=5)
        return result.stdout if result.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def ids_with_icon(ids):
    """Comme Windows 11 : juste avant les icônes réseau, son et batterie."""
    ids = [value for value in ids if value != PLUGIN]
    at = next((index for index, value in enumerate(ids) if value == 4), len(ids))
    return ids[:at] + [PLUGIN] + ids[at:]


def set_panel(enabled):
    """Ajoute (réglage auto ou toujours) ou retire (jamais) le greffon de la barre."""
    if not shutil.which("xfconf-query"):
        return False
    output = _xfconf("-p", "/panels/panel-1/plugin-ids") or ""
    ids = [int(line) for line in output.splitlines() if line.strip().isdigit()]
    if not ids:
        return False
    if enabled:
        command = "/usr/bin/winlux-osk tray" if Path("/usr/bin/winlux-osk").exists() else f"python3 {Path(__file__).resolve()} tray"
        for prop, kind, value in (("", "string", "genmon"), ("/command", "string", command),
                                  ("/use-label", "bool", "false"), ("/text", "string", ""),
                                  ("/update-period", "int", "5000")):
            path = f"/plugins/plugin-{PLUGIN}{prop}"
            if (_xfconf("-p", path) or "").strip() != value and _xfconf("-p", path, "-s", value) is None:
                _xfconf("-p", path, "-n", "-t", kind, "-s", value)
        wanted = ids_with_icon(ids)
    else:
        wanted = [value for value in ids if value != PLUGIN]
    if wanted != ids:
        args = ["-p", "/panels/panel-1/plugin-ids", "-a"]
        for value in wanted:
            args += ["-t", "int", "-s", str(value)]
        _xfconf(*args)
    if not enabled:
        _xfconf("-p", f"/plugins/plugin-{PLUGIN}", "-r", "-R")
    if wanted != ids and os.environ.get("WINLUX_OSK_NO_RESTART") != "1":
        restart_panel()
    return True


def restart_panel():
    """Barre déjà lancée (réglage changé dans Paramètres, clavier débranché) :
    xfce4-panel ne charge pas un greffon ajouté à chaud, on la relance."""
    if not shutil.which("xfce4-panel"):
        return False
    try:
        running = subprocess.run(["pgrep", "-u", str(os.getuid()), "-x", "xfce4-panel"],
                                 capture_output=True, timeout=3).returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False
    if running:
        subprocess.Popen(["xfce4-panel", "-r"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)
    return running


def main(argv):
    command = argv[1] if len(argv) > 1 else "toggle"
    if command == "detect":
        found = keyboards()
        print(f"keyboard={int(bool(found))}")
        for name in found:
            print(f"name={name}")
        return 0
    if command == "icon":
        print(f"icon={int(icon_wanted())}\nsetting={setting()}")
        return 0
    if command == "tray":
        print(tray_output())
        return 0
    if command == "panel":
        set_panel(setting() != "never")
        return 0
    if command == "set" and len(argv) > 2:
        save_setting(argv[2])
        set_panel(argv[2] != "never")
        return 0
    if command == "need":
        # Un mot de passe est demandé (Contrôle de compte, installation) :
        # sans clavier physique, le clavier tactile s'ouvre de lui-même.
        if keyboards():
            return 0
        sys.argv[1:] = ["show"]
        return run_window()
    if command in ("toggle", "show", "hide"):
        return run_window()
    print(__doc__.strip().split("\n\n")[2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
