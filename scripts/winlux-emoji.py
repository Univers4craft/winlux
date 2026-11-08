#!/usr/bin/python3
"""Sélecteur d'emojis et de symboles (Windows + . ou Windows + ;).

Trois onglets : emojis (par catégorie, récemment utilisés, recherche dans la
langue de l'interface et en anglais), kaomojis, symboles (ponctuation,
monnaies, lettres accentuées, grec, maths, flèches, divers). Le choix est tapé
dans la fenêtre qui avait le clavier, application Linux comme programme
Windows, puis le sélecteur se ferme. Échap le ferme sans rien taper.

Sans police d'emojis en couleur, un bandeau propose de la télécharger
(paquet fonts-noto-color-emoji, environ 10 Mo, par le Store) : rien de lourd
dans l'ISO.

    winlux-emoji              ouvre le sélecteur
    winlux-emoji --search MOT affiche les emojis trouvés (tests)

Données : /usr/share/winlux/native/emoji/emoji.tsv (WINLUX_EMOJI_DATA),
récents : ~/.local/state/winlux/emoji-recent.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unicodedata


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
    'Emojis': ('Emoji', 'Emojis', 'Emojis'),
    'Kaomojis': ('Kaomoji', 'Kaomojis', 'Kaomojis'),
    'Symboles': ('Symbols', 'Símbolos', 'Symbole'),
    'Rechercher': ('Search', 'Buscar', 'Suchen'),
    'Récemment utilisés': ('Most recently used', 'Usados recientemente', 'Zuletzt verwendet'),
    'Aucun résultat.': ('No results.', 'Sin resultados.', 'Keine Ergebnisse.'),
    'Smileys et émotions': ('Smileys and emotions', 'Caritas y emociones', 'Smileys und Emotionen'),
    'Personnes et corps': ('People and body', 'Personas y cuerpo', 'Menschen und Körper'),
    'Animaux et nature': ('Animals and nature', 'Animales y naturaleza', 'Tiere und Natur'),
    'Nourriture et boissons': ('Food and drink', 'Comida y bebida', 'Essen und Trinken'),
    'Voyages et lieux': ('Travel and places', 'Viajes y lugares', 'Reisen und Orte'),
    'Activités': ('Activities', 'Actividades', 'Aktivitäten'),
    'Objets': ('Objects', 'Objetos', 'Objekte'),
    'Drapeaux': ('Flags', 'Banderas', 'Flaggen'),
    'Ponctuation': ('Punctuation', 'Puntuación', 'Satzzeichen'),
    'Monnaies': ('Currency', 'Monedas', 'Währungen'),
    'Lettres accentuées': ('Accented letters', 'Letras acentuadas', 'Buchstaben mit Akzent'),
    'Grec': ('Greek', 'Griego', 'Griechisch'),
    'Mathématiques': ('Math', 'Matemáticas', 'Mathematik'),
    'Flèches': ('Arrows', 'Flechas', 'Pfeile'),
    'Divers': ('Miscellaneous', 'Varios', 'Verschiedenes'),
    'Emojis en couleur : télécharger (10 Mo)': ('Colour emoji: download (10 MB)', 'Emojis en color: descargar (10 MB)', 'Farbige Emojis: herunterladen (10 MB)'),
    'Téléchargement des emojis en couleur…': ('Downloading colour emoji…', 'Descargando emojis en color…', 'Farbige Emojis werden heruntergeladen…'),
    'Emojis en couleur installés.': ('Colour emoji installed.', 'Emojis en color instalados.', 'Farbige Emojis installiert.'),
    'Téléchargement impossible pour l’instant.': ('Download not possible right now.', 'No se puede descargar ahora.', 'Download derzeit nicht möglich.'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def fold(text):
    """Comparaison sans casse ni accents (comme toutes les recherches Winlux)."""
    text = unicodedata.normalize("NFKD", text.casefold())
    return "".join(char for char in text if not unicodedata.combining(char)).replace("ß", "ss").replace("œ", "oe").replace("æ", "ae")


# Groupes d'emojibase (le 2, « composants », n'est pas montré).
GROUPS = ((0, "😀", "Smileys et émotions"), (1, "👋", "Personnes et corps"), (3, "🐻", "Animaux et nature"),
          (4, "🍔", "Nourriture et boissons"), (5, "🚗", "Voyages et lieux"), (6, "⚽", "Activités"),
          (7, "💡", "Objets"), (8, "❤", "Symboles"), (9, "🏳", "Drapeaux"))
KAOMOJI = ("(◕‿◕)", "¯\\_(ツ)_/¯", "(╯°□°)╯︵ ┻━┻", "┬─┬ノ( º _ ºノ)", "( ͡° ͜ʖ ͡°)", "ヽ(•‿•)ノ", "(ಥ﹏ಥ)",
           "(ᵔᴥᵔ)", "ʕ•ᴥ•ʔ", "(｡◕‿◕｡)", "(≧▽≦)", "(^_^)", "(^o^)", "(T_T)", "(>_<)", "(o_O)", "(¬_¬)",
           "(•_•)", "( •_•)>⌐■-■", "(⌐■_■)", "♥‿♥", "(づ｡◕‿‿◕｡)づ", "٩(◕‿◕)۶", "(ノ◕ヮ◕)ノ*:･ﾟ✧",
           "(☞ﾟヮﾟ)☞", "ಠ_ಠ", "(ʘ‿ʘ)", "(✿◠‿◠)", "(*^‿^*)", "(╥﹏╥)", "┐(´ー｀)┌", "(ง •̀_•́)ง",
           "(◉_◉)", "ᕕ( ᐛ )ᕗ", "(￣▽￣)", "(-_-) zzZ", "(ﾉ´ヮ`)ﾉ*: ･ﾟ", "(´｡• ᵕ •｡`)", "(*≧ω≦*)", "(⊙_⊙)")
SYMBOLS = (
    ("Ponctuation", "… « » ‹ › “ ” ‘ ’ „ — – · • ¡ ¿ ‰ † ‡ § ¶ ¦ ‽ ⁂"),
    ("Monnaies", "€ £ ¥ ¢ $ ₹ ₽ ₩ ₺ ₿ ₴ ₪ ₫ ฿ ¤"),
    ("Lettres accentuées", "À Á Â Ã Ä Å Æ Ç È É Ê Ë Ì Í Î Ï Ð Ñ Ò Ó Ô Õ Ö Ø Œ Ù Ú Û Ü Ý Ÿ Þ ẞ "
                           "à á â ã ä å æ ç è é ê ë ì í î ï ð ñ ò ó ô õ ö ø œ ù ú û ü ý ÿ þ ß"),
    ("Grec", "Α Β Γ Δ Ε Ζ Η Θ Ι Κ Λ Μ Ν Ξ Ο Π Ρ Σ Τ Υ Φ Χ Ψ Ω α β γ δ ε ζ η θ ι κ λ μ ν ξ ο π ρ σ τ υ φ χ ψ ω"),
    ("Mathématiques", "± × ÷ ≠ ≈ ≡ ≤ ≥ ∞ √ ∛ ∑ ∏ ∫ ∂ ∆ ∇ ∈ ∉ ∩ ∪ ⊂ ⊃ ∀ ∃ ¬ ∧ ∨ ° ′ ″ ½ ⅓ ⅔ ¼ ¾ ⅛ ¹ ² ³ ⁿ µ ‰ ∝ ∅"),
    ("Flèches", "← ↑ → ↓ ↔ ↕ ↖ ↗ ↘ ↙ ⇐ ⇑ ⇒ ⇓ ⇔ ↩ ↪ ⤴ ⤵ ↻ ↺ ➜ ➔"),
    ("Divers", "© ® ™ ℃ ℉ № ℓ ♠ ♣ ♥ ♦ ★ ☆ ✓ ✔ ✗ ✘ ☐ ☑ ☒ ♪ ♫ ☀ ☁ ☂ ☎ ✉ ⌘ ⌥ ⏎ ⌫ ⏻ ♀ ♂ ⚠ ☺ ☹"),
)
STATE = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "winlux"
RECENT = STATE / "emoji-recent"
RECENT_MAX = 24


def data_path():
    if os.environ.get("WINLUX_EMOJI_DATA"): return Path(os.environ["WINLUX_EMOJI_DATA"])
    installed = Path("/usr/share/winlux/native/emoji/emoji.tsv")
    if installed.is_file(): return installed
    return Path(__file__).resolve().parent.parent / "integration/native/emoji/emoji.tsv"


def load_emoji():
    """[(caractère, groupe, mots dans la langue de l'interface, mots anglais)]."""
    column = {"en": 2, "fr": 3, "es": 4, "de": 5}[_LANG]
    items = []
    try:
        lines = data_path().read_text(encoding="utf-8").splitlines()
    except OSError:
        return items
    for line in lines:
        fields = line.split("\t")
        if len(fields) < 6: continue
        try: group = int(fields[1])
        except ValueError: continue
        local = fields[column].split("|")
        english = fields[2].split("|")
        items.append((fields[0], group, local, english))
    return items


def search(items, query, limit=120):
    """Emojis dont un mot commence par la saisie (nom d'abord), dans les deux langues."""
    words = fold(query).split()
    if not words: return []
    scored = []
    for position, (char, _, local, english) in enumerate(items):
        names = [fold(word) for word in local + english]
        if all(any(name.startswith(word) or f" {word}" in f" {name}" for name in names) for word in words):
            exact_name = fold(local[0]).startswith(words[0]) or fold(english[0]).startswith(words[0])
            scored.append((0 if exact_name else 1, position, char))
    scored.sort()
    return [char for _, _, char in scored[:limit]]


def name_of(items, char):
    for item in items:
        if item[0] == char: return item[2][0]
    return ""


def recent_load():
    try: return [line for line in RECENT.read_text(encoding="utf-8").splitlines() if line][:RECENT_MAX]
    except OSError: return []


def recent_add(char):
    chars = [char] + [item for item in recent_load() if item != char]
    try:
        STATE.mkdir(parents=True, exist_ok=True)
        RECENT.write_text("\n".join(chars[:RECENT_MAX]) + "\n", encoding="utf-8")
    except OSError:
        pass


def color_font_installed():
    if os.environ.get("WINLUX_EMOJI_FONT") in ("0", "1"): return os.environ["WINLUX_EMOJI_FONT"] == "1"
    try:
        result = subprocess.run(["fc-list", ":charset=1f600", "family"], capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return True
    return any("emoji" in line.lower() for line in result.stdout.splitlines())


def type_text(text, window):
    """Tape le texte dans la fenêtre qui avait le clavier avant l'ouverture."""
    tool = shutil.which("xdotool")
    if not tool: return False
    if window:
        subprocess.run([tool, "windowactivate", "--sync", window], capture_output=True, timeout=3)
    return subprocess.run([tool, "type", "--clearmodifiers", "--delay", "8", "--", text],
                          capture_output=True, timeout=10).returncode == 0


def active_window():
    tool = shutil.which("xdotool")
    if not tool: return ""
    try:
        result = subprocess.run([tool, "getactivewindow"], capture_output=True, text=True, timeout=3)
    except (OSError, subprocess.SubprocessError):
        return ""
    return result.stdout.strip() if result.returncode == 0 else ""


def run_picker():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk

    target = active_window()
    items = load_emoji()
    by_group = {}
    for char, group, _, _ in items: by_group.setdefault(group, []).append(char)

    class Picker(Gtk.Window):
        def __init__(self):
            super().__init__(title=_t("Emojis"))
            self.set_role("winlux-emoji")
            self.set_decorated(False)
            self.set_keep_above(True)
            self.set_skip_taskbar_hint(True)
            self.set_type_hint(Gdk.WindowTypeHint.DIALOG)
            self.set_default_size(380, 440)
            self.chosen = None
            css = Gtk.CssProvider()
            css.load_from_data(b"""
              window { background: #202430; color: #ecedf5; border: 1px solid #3c4458; border-radius: 8px; }
              entry { background: #171b25; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 6px; }
              button.cell { background: transparent; border: none; box-shadow: none; padding: 2px; min-width: 38px; min-height: 38px; }
              button.cell:hover, button.cell:focus { background: #30374a; }
              button.cell label { font-size: 20px; }
              button.wide label { font-size: 14px; }
              button.tab { background: transparent; border: none; box-shadow: none; padding: 4px 10px; color: #c5cbe0; }
              button.tab.on { border-bottom: 2px solid #b69cff; color: #ffffff; }
              .heading { color: #adb6c9; font-size: 12px; margin-top: 4px; }
              .banner { background: #2c3344; border-radius: 6px; }
            """)
            Gtk.StyleContext.add_provider_for_screen(self.get_screen(), css, 600)
            outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin=10)
            self.add(outer)
            tabs = Gtk.Box(spacing=2)
            outer.pack_start(tabs, False, False, 0)
            self.tab_buttons = {}
            for key, title in (("emoji", "😀  " + _t("Emojis")), ("kaomoji", ";-)  " + _t("Kaomojis")),
                               ("symbols", "Ω  " + _t("Symboles"))):
                button = Gtk.Button(label=title)
                button.get_style_context().add_class("tab")
                button.connect("clicked", lambda _, k=key: self.show_tab(k))
                tabs.pack_start(button, False, False, 0)
                self.tab_buttons[key] = button
            self.entry = Gtk.SearchEntry(placeholder_text=_t("Rechercher"))
            self.entry.connect("search-changed", lambda *_: self.refresh())
            self.entry.connect("activate", lambda *_: self.pick_first())
            outer.pack_start(self.entry, False, False, 0)
            self.banner = None
            if not color_font_installed() and shutil.which("winlux-store"):
                self.banner = Gtk.Button(label=_t("Emojis en couleur : télécharger (10 Mo)"))
                self.banner.get_style_context().add_class("banner")
                self.banner.connect("clicked", lambda *_: self.download_font())
                outer.pack_start(self.banner, False, False, 0)
            scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
            outer.pack_start(scroll, True, True, 0)
            self.content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            scroll.add(self.content)
            self.groups = Gtk.Box(spacing=0, homogeneous=True)
            outer.pack_start(self.groups, False, False, 0)
            self.group = None
            self.tab = "emoji"
            self.connect("key-press-event", self.key)
            self.connect("focus-out-event", lambda *_: GLib.timeout_add(150, self.lost_focus) and False)
            self.show_tab("emoji")

        # ── Construction ──

        def heading(self, text):
            label = Gtk.Label(label=text, xalign=0)
            label.get_style_context().add_class("heading")
            self.content.pack_start(label, False, False, 0)

        def grid(self, chars, wide=False):
            flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, homogeneous=not wide,
                               max_children_per_line=3 if wide else 8, min_children_per_line=2 if wide else 8,
                               row_spacing=0, column_spacing=0)
            for char in chars:
                button = Gtk.Button(label=char)
                button.get_style_context().add_class("cell")
                if wide: button.get_style_context().add_class("wide")
                tip = name_of(items, char) if not wide else ""
                if tip: button.set_tooltip_text(tip)
                button.connect("clicked", lambda _, c=char: self.choose(c))
                flow.add(button)
            self.content.pack_start(flow, False, False, 0)
            return flow

        def clear(self):
            for child in self.content.get_children(): child.destroy()

        def show_tab(self, tab):
            self.tab = tab
            for key, button in self.tab_buttons.items():
                style = button.get_style_context()
                (style.add_class if key == tab else style.remove_class)("on")
            for child in self.groups.get_children(): child.destroy()
            if tab == "emoji":
                for group, icon, title in GROUPS:
                    button = Gtk.Button(label=icon)
                    button.get_style_context().add_class("tab")
                    button.set_tooltip_text(_t(title))
                    button.connect("clicked", lambda _, g=group: self.show_group(g))
                    self.groups.pack_start(button, True, True, 0)
            self.group = None
            self.refresh()
            self.show_all()
            if self.banner is not None: self.banner.set_visible(tab == "emoji")
            self.entry.grab_focus()

        def show_group(self, group):
            self.group = group
            self.entry.set_text("")
            self.refresh()

        def refresh(self):
            self.clear()
            query = self.entry.get_text().strip()
            if self.tab == "kaomoji":
                chosen = [k for k in KAOMOJI if not query or fold(query) in fold(k)]
                self.grid(chosen, wide=True)
            elif self.tab == "symbols":
                for title, chars in SYMBOLS:
                    chars = chars.split()
                    if query:
                        chars = [c for c in chars if fold(query) in fold(c) or fold(query) in fold(unicodedata.name(c, ""))]
                        if not chars: continue
                    self.heading(_t(title))
                    self.grid(chars)
            elif query:
                found = search(items, query)
                if found: self.grid(found)
                else: self.heading(_t("Aucun résultat."))
            else:
                recent = recent_load()
                if recent and self.group is None:
                    self.heading(_t("Récemment utilisés"))
                    self.grid(recent)
                for group, _, title in GROUPS:
                    if self.group is not None and group != self.group: continue
                    self.heading(_t(title))
                    self.grid(by_group.get(group, []))
                    if self.group is None: break
            self.content.show_all()

        # ── Choix ──

        def pick_first(self):
            for child in self.content.get_children():
                if isinstance(child, Gtk.FlowBox) and child.get_children():
                    self.choose(child.get_children()[0].get_child().get_label())
                    return

        def choose(self, char):
            self.chosen = char
            recent_add(char)
            self.hide()
            # Laisser le gestionnaire de fenêtres rendre le clavier avant de taper.
            GLib.timeout_add(80, self.finish)

        def finish(self):
            type_text(self.chosen, target)
            Gtk.main_quit()
            return False

        def key(self, _, event):
            if event.keyval == Gdk.KEY_Escape:
                Gtk.main_quit()
                return True
            return False

        def lost_focus(self):
            # Clic ailleurs : fermé, comme sous Windows (sauf pendant un téléchargement).
            if self.chosen is None and not self.is_active() and not getattr(self, "downloading", False):
                Gtk.main_quit()
            return False

        def download_font(self):
            self.downloading = True
            self.banner.set_label(_t("Téléchargement des emojis en couleur…"))
            self.banner.set_sensitive(False)
            try:
                process = subprocess.Popen(["winlux-store", "install", "apt:fonts-noto-color-emoji"],
                                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except OSError:
                self.banner.set_label(_t("Téléchargement impossible pour l’instant."))
                return

            def wait():
                if process.poll() is None: return True
                self.downloading = False
                if process.returncode == 0:
                    subprocess.run(["fc-cache", "-f"], capture_output=True, timeout=60)
                    self.banner.set_label(_t("Emojis en couleur installés."))
                    self.refresh()
                else:
                    self.banner.set_label(_t("Téléchargement impossible pour l’instant."))
                    self.banner.set_sensitive(True)
                return False
            GLib.timeout_add(500, wait)

    window = Picker()
    # Près du pointeur, sans dépasser de l'écran.
    display = Gdk.Display.get_default()
    seat = display.get_default_seat() if display else None
    if seat is not None:
        _, x, y = seat.get_pointer().get_position()
        monitor = display.get_monitor_at_point(x, y).get_workarea()
        width, height = window.get_size()
        window.move(max(monitor.x, min(x - width // 2, monitor.x + monitor.width - width)),
                    max(monitor.y, min(y + 16, monitor.y + monitor.height - height)))
    window.present()
    Gtk.main()
    return 0


def main(argv):
    if len(argv) == 3 and argv[1] == "--search":
        print(" ".join(search(load_emoji(), argv[2])))
        return 0
    if len(argv) > 1:
        print(__doc__.strip().split("\n\n")[-2], file=sys.stderr)
        return 2
    return run_picker()


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
