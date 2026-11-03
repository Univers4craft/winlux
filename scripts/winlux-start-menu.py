#!/usr/bin/python3
"""Windows 11 inspired native Start: real XDG apps, pins and context actions."""
import hashlib
import json
import os
import pwd
import re
from pathlib import Path
import shutil
from string import Template
import subprocess
import threading
import time
import unicodedata
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, Gio, GLib


# ── Recherche : ni casse ni accents (« parametres » trouve « Paramètres ») ──

_LIGATURES = str.maketrans({"œ": "oe", "æ": "ae", "ø": "o", "đ": "d", "ł": "l", "ı": "i"})


def account_name():
    """Nom du compte affiché en bas du menu, comme sous Windows : le nom
    complet saisi à l'installation, sinon l'identifiant ; « Winlux » pour la
    session live."""
    try:
        entry = pwd.getpwuid(os.getuid())
    except KeyError:
        return "Winlux"
    if entry.pw_uid == 0:
        # Système installé : le bureau tourne encore sous root, le compte
        # créé à l'installation est noté dans install-config.json.
        try:
            config = json.loads(Path("/etc/winlux/install-config.json").read_text(encoding="utf-8"))
            return str(config.get("fullname") or config.get("username") or "Winlux")
        except (OSError, ValueError, AttributeError):
            return "Winlux"
    return entry.pw_gecos.split(",")[0].strip() or entry.pw_name


def fold(text):
    """Texte replié pour comparer : minuscules, sans accents ni cédilles,
    ß → ss, œ → oe. Vaut pour les quatre langues (fr, en, es, de)."""
    text = unicodedata.normalize("NFKD", text.casefold().translate(_LIGATURES))
    return "".join(c for c in text if not unicodedata.combining(c))


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Système et alimentation': ('System and power', 'Sistema y energía', 'System und Energie'),
    'Applications et démarrage': ('Apps and startup', 'Aplicaciones e inicio', 'Apps und Autostart'),
    'Réseau Wi-Fi': ('Wi-Fi network', 'Red Wi-Fi', 'WLAN-Netzwerk'),
    'Bluetooth et appareils': ('Bluetooth and devices', 'Bluetooth y dispositivos', 'Bluetooth und Geräte'),
    'Son': ('Sound', 'Sonido', 'Sound'),
    'Clavier tactile': ('Touch keyboard', 'Teclado táctil', 'Bildschirmtastatur'),
    'Alimentation batterie veille': ('Power battery sleep', 'Energía batería suspensión', 'Energie Akku Standby'),
    'Clavier': ('Keyboard', 'Teclado', 'Tastatur'),
    'Souris': ('Mouse', 'Ratón', 'Maus'),
    'Personnalisation fond écran thème': ('Personalization wallpaper theme', 'Personalización fondo pantalla tema', 'Personalisierung Hintergrund Design'),
    'Mises à jour': ('Updates', 'Actualizaciones', 'Updates'),
    'Écran résolution': ('Display resolution', 'Pantalla resolución', 'Bildschirm Auflösung'),
    'À propos version': ('About version', 'Acerca de versión', 'Info Version'),
    'Navigateurs': ('Browsers', 'Navegadores', 'Browser'),
    'Boutiques de logiciels': ('Software stores', 'Tiendas de programas', 'Software-Stores'),
    'Partage de dossiers et transfert des réglages': ('Folder sharing and settings transfer', 'Compartir carpetas y transferir ajustes', 'Ordnerfreigabe und Übertragung der Einstellungen'),
    'Paramètres': ('Settings', 'Configuración', 'Einstellungen'),
    'Bureautique': ('Office', 'Oficina', 'Büro'),
    'Graphisme': ('Graphics', 'Gráficos', 'Grafik'),
    'Audio et vidéo': ('Audio and video', 'Audio y vídeo', 'Audio und Video'),
    'Jeux': ('Games', 'Juegos', 'Spiele'),
    'Jeux bibliothèque temps de jeu': ('Games library play time', 'Juegos biblioteca tiempo de juego', 'Spiele Bibliothek Spielzeit'),
    'Développement': ('Development', 'Desarrollo', 'Entwicklung'),
    'Système': ('System', 'Sistema', 'System'),
    'Accessoires': ('Accessories', 'Accesorios', 'Zubehör'),
    'Autres applications': ('Other apps', 'Otras aplicaciones', 'Weitere Apps'),
    'Démarrer Winlux': ('Winlux Start', 'Inicio de Winlux', 'Winlux-Start'),
    'Applications, fichiers et paramètres': ('Apps, files and settings', 'Aplicaciones, archivos y configuración', 'Apps, Dateien und Einstellungen'),
    'Épinglé': ('Pinned', 'Anclado', 'Angeheftet'),
    'Toutes les applications  ›': ('All apps  ›', 'Todas las aplicaciones  ›', 'Alle Apps  ›'),
    'Options d’alimentation': ('Power options', 'Opciones de energía', 'Energieoptionen'),
    'Impossible d’ouvrir l’application': ("Couldn't open the app", 'No se puede abrir la aplicación', 'Die App konnte nicht geöffnet werden'),
    'Résultats': ('Results', 'Resultados', 'Ergebnisse'),
    'Toutes les applications': ('All apps', 'Todas las aplicaciones', 'Alle Apps'),
    '‹  Retour': ('‹  Back', '‹  Volver', '‹  Zurück'),
    'Paramètres · ': ('Settings · ', 'Configuración · ', 'Einstellungen · '),
    'Fichier · ': ('File · ', 'Archivo · ', 'Datei · '),
    '30 premiers fichiers affichés. Précisez la recherche.': ('First 30 files shown. Refine your search.', 'Se muestran los 30 primeros archivos. Precise la búsqueda.', 'Die ersten 30 Dateien werden angezeigt. Verfeinern Sie die Suche.'),
    'Aucune application trouvée': ('No apps found', 'No se encontró ninguna aplicación', 'Keine Apps gefunden'),
    'Recommandations': ('Recommended', 'Recomendaciones', 'Empfohlen'),
    'Vos applications récemment ouvertes apparaîtront ici.': ('Your recently opened apps will show up here.', 'Sus aplicaciones abiertas recientemente aparecerán aquí.', 'Ihre zuletzt geöffneten Apps werden hier angezeigt.'),
    'Impossible d’ouvrir ce fichier': ("Couldn't open this file", 'No se puede abrir este archivo', 'Diese Datei konnte nicht geöffnet werden'),
    'Ouvrir': ('Open', 'Abrir', 'Öffnen'),
    'Détacher du menu Démarrer': ('Unpin from Start', 'Desanclar de Inicio', 'Von Start lösen'),
    'Épingler au menu Démarrer': ('Pin to Start', 'Anclar a Inicio', 'An Start anheften'),
    'Créer un raccourci sur le bureau': ('Create desktop shortcut', 'Crear acceso directo en el escritorio', 'Desktopverknüpfung erstellen'),
    'Redémarrer': ('Restart', 'Reiniciar', 'Neu starten'),
    'Verrouiller': ('Lock', 'Bloquear', 'Sperren'),
    'Se déconnecter': ('Sign out', 'Cerrar sesión', 'Abmelden'),
    'Changer d’utilisateur': ('Switch user', 'Cambiar de usuario', 'Benutzer wechseln'),
    'Mettre en veille': ('Sleep', 'Suspender', 'Energie sparen'),
    'Arrêter': ('Shut down', 'Apagar', 'Herunterfahren'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


STATE = Path.home() / ".config/winlux/start.json"
DEFAULTS = ["winlux-explorer.desktop", "winlux-browser.desktop", "winlux-store.desktop", "org.xfce.mousepad.desktop",
            "winlux-settings.desktop", "winlux-terminal.desktop", "winlux-updates.desktop"]
SETTINGS = [(_t("Système et alimentation"), 0), (_t("Applications et démarrage"), 1), (_t("Réseau Wi-Fi"), 35), (_t("Bluetooth et appareils"), 22),
            (_t("Son"), 16), (_t("Alimentation batterie veille"), 23), (_t("Clavier"), 4), (_t("Souris"), 4), (_t("Personnalisation fond écran thème"), 21),
            (_t("Mises à jour"), 7), (_t("Écran résolution"), 8), (_t("À propos version"), 9), (_t("Navigateurs"), 11), (_t("Boutiques de logiciels"), 1),
            (_t("Partage de dossiers et transfert des réglages"), 28), (_t("Jeux bibliothèque temps de jeu"), 29)]
# Une seule application Paramètres : les panneaux de réglages du bureau XFCE et
# les raccourcis vers une page des Paramètres ne s'affichent pas à part (la
# recherche trouve les pages ci-dessus).
SETTINGS_KEPT = {"winlux-settings.desktop", "winlux-updates.desktop"}


def listed(app):
    if not app.should_show() or not app.get_id(): return False
    if app.get_id() in SETTINGS_KEPT: return True
    categories = set((app.get_categories() or "").split(";")) if hasattr(app, "get_categories") else set()
    return "Settings" not in categories

def category(app):
    """Use installed desktop metadata, including generated Windows launchers."""
    categories = set((app.get_categories() or "").split(";")) if hasattr(app, "get_categories") else set()
    for key, label in [("Settings", _t("Paramètres")), ("Network", "Internet"),
                       ("Office", _t("Bureautique")), ("Graphics", _t("Graphisme")),
                       ("AudioVideo", _t("Audio et vidéo")), ("Game", _t("Jeux")),
                       ("Development", _t("Développement")), ("System", _t("Système")),
                       ("Utility", _t("Accessoires"))]:
        if key in categories: return label
    return _t("Autres applications")

# Services de la session, et Thunar : l'Explorateur Winlux le remplace (« par »
# proposait « Gestionnaire de fichiers Thunar » juste après Paramètres).
SESSION_SERVICES = {"picom.desktop", "thunar.desktop", "org.xfce.thunar.desktop", "Thunar.desktop",
                    "Thunar-folder-handler.desktop", "thunar-folder-handler.desktop"}

# Couleurs du menu Démarrer : texte/fond au moins 4,5:1 (WCAG AA) dans les deux
# thèmes, vérifié par scripts/test-shell-contrast.py.
PALETTES = {
    "dark": {"bg": "#242238", "fg": "#f7f5ff", "border": "#6b5f92", "field": "#171624", "dim": "#b8b1d6",
             "hover": "#433b60", "footer": "#1d1a2d", "accent": "#b19af9", "selected": "#7c4dff", "on_selected": "#ffffff"},
    "light": {"bg": "#f6f4fc", "fg": "#1f1b2e", "border": "#c9c0e6", "field": "#ffffff", "dim": "#5e5778",
              "hover": "#e4ddf7", "footer": "#ebe7f6", "accent": "#7c4dff", "selected": "#7c4dff", "on_selected": "#ffffff"},
}
MENU_CSS = Template("""
  #winlux-start { background: $bg; color: $fg; border: 1px solid $border; border-radius: 8px; }
  #winlux-start box, #winlux-start scrolledwindow, #winlux-start viewport { background: transparent; }
  #winlux-start label, #winlux-start image, #winlux-start button { color: $fg; text-shadow: none; -gtk-icon-shadow: none; }
  #winlux-start entry { border-radius: 5px; background: $field; color: $fg; padding: 9px; border: 1px solid $border;
                        border-bottom: 2px solid $accent; box-shadow: none; }
  #winlux-start entry image { color: $dim; }
  #winlux-start entry selection { background: $selected; color: $on_selected; }
  #winlux-start button { background: transparent; border: none; box-shadow: none; border-radius: 5px; padding: 7px; }
  #winlux-start button:hover, #winlux-start button:focus { background: $hover; }
  #winlux-start #winlux-start-osk label { font-size: 20px; }
  /* Navigation au clavier (#28) : cadre visible seulement au clavier, comme Windows. */
  #winlux-start button { outline: 2px solid $accent; outline-offset: -2px; }
  #winlux-start .heading { font-weight: bold; font-size: 12pt; }
  #winlux-start .footer { background: $footer; padding: 12px 25px; }
  #winlux-start .app-name { font-size: 9pt; }
  menu.winlux-menu, .winlux-menu.menu { background: $bg; border: 1px solid $border; border-radius: 8px; padding: 4px; }
  menu.winlux-menu menuitem { color: $fg; padding: 8px 18px 8px 12px; border-radius: 4px; min-width: 150px; }
  menu.winlux-menu menuitem label, menu.winlux-menu menuitem image { color: $fg; -gtk-icon-shadow: none; }
  menu.winlux-menu menuitem:hover { background: $hover; }
""")


def dark_theme():
    """Thème sombre demandé : préférence GTK, thème « …-dark » ou GTK_THEME=…:dark."""
    forced = os.environ.get("GTK_THEME")
    if forced: return "dark" in forced.casefold()  # GTK ignore alors la préférence
    settings = Gtk.Settings.get_default()
    name = (settings.get_property("gtk-theme-name") or "").casefold()
    if name.startswith("winlux-"): return "-dark" in name  # thème de winlux-personalize
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name


def on_click(button, callback):
    """Clic fiable pendant la saisie du pointeur du menu.

    Sous cette saisie, GTK ne reçoit pas toujours l'entrée du pointeur dans le
    bouton : il n'est alors pas « survolé » et « clicked » n'est jamais émis
    (Paramètres ne s'ouvrait pas un essai sur deux). Le relâchement du bouton
    gauche dans ses limites suffit donc ; Entrée/Espace passent par clicked.
    """
    pending = []
    def run():
        pending.clear()
        callback()
        return False
    def released(widget, event):
        inside = 0 <= event.x < widget.get_allocated_width() and 0 <= event.y < widget.get_allocated_height()
        if event.button == 1 and inside and not pending:
            # Après le traitement GTK : un « clicked » émis entre-temps est ignoré.
            pending.append(True)
            GLib.idle_add(run)
        return False
    def clicked(*_):
        if not pending: callback()
    button.connect("button-release-event", released)
    button.connect("clicked", clicked)


def osk_module():
    """Clavier tactile (/usr/bin/winlux-osk, ou le script voisin), ou None."""
    import importlib.machinery
    import importlib.util
    for path in (Path("/usr/bin/winlux-osk"), Path(__file__).resolve().with_name("winlux-osk.py")):
        if path.is_file():
            try:
                loader = importlib.machinery.SourceFileLoader("winlux_osk", str(path))
                spec = importlib.util.spec_from_loader("winlux_osk", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


class Start(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="org.winlux.Start", flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.window = None
        self.context = None
        self.all_apps = False
        self.file_generation = 0
        self.file_query = ""
        self.file_results = []
        try: self.state = json.loads(STATE.read_text())
        except (OSError, ValueError): self.state = {"pins": DEFAULTS, "recent": []}
        if not isinstance(self.state, dict): self.state = {"pins": DEFAULTS, "recent": []}
        self.state["pins"] = migrate_pins(self.state.get("pins", DEFAULTS))

    def save(self):
        STATE.parent.mkdir(parents=True, exist_ok=True)
        temporary = STATE.with_suffix(".tmp")
        temporary.write_text(json.dumps(self.state, ensure_ascii=False))
        temporary.replace(STATE)

    def do_activate(self):
        if self.window and self.window.get_visible():
            self.hide()
            return
        if not self.window: self.build()
        # Épingles ajoutées par une autre application (Jeux) depuis la
        # dernière ouverture : relues à chaque ouverture.
        try: self.state = json.loads(STATE.read_text())
        except (OSError, ValueError): pass
        # Services de la session livrés avec une entrée de menu par leur paquet.
        self.apps = {app.get_id(): app for app in Gio.AppInfo.get_all()
                     if listed(app) and app.get_id() not in SESSION_SERVICES}
        if auto_pin(self.state, self.apps): self.save()
        self.all_apps = False
        self.search.set_text("")
        self.render()
        # Tablette sans clavier : le clavier tactile du menu s'ouvre avec lui.
        # Le clavier de la barre des tâches, fenêtre à part, fermait le menu au
        # premier appui et se trouvait en partie dessous.
        keyboard_shown = self.osk_widget is not None and self.osk_auto()
        if keyboard_shown:
            try: subprocess.Popen(["winlux-osk", "hide"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            except OSError: pass
        _, pointer_x, pointer_y = self.window.get_display().get_default_seat().get_pointer().get_position()
        area = self.workarea()
        # Comme Windows 11 : barre alignée à gauche, le menu s'ouvre au-dessus
        # du bouton Démarrer ; barre centrée ou ouverture au clavier, il est
        # centré.
        self.menu_left = pointer_y >= area.y + area.height and area.x <= pointer_x < area.x + area.width // 3
        self.place(keyboard_shown)
        self.window.show_all()
        self.window.present()
        self.search.grab_focus()

    def workarea(self):
        display = self.window.get_display()
        monitor = display.get_primary_monitor() or display.get_monitor(0)
        return monitor.get_workarea()

    def place(self, keyboard_shown):
        """Taille et position du menu, avec ou sans son clavier tactile."""
        area = self.workarea()
        width, height = 660, 650
        # Clavier ouvert : marges resserrées, et touches moins hautes sur un
        # petit écran. En 1366x768 la 2e ligne des épinglés perdait son libellé
        # sous le clavier.
        compact = keyboard_shown
        self.body.set_spacing(6 if compact else 18)
        self.body.set_margin_top(8 if compact else 28)
        self.body.set_margin_bottom(4 if compact else 28)
        if self.osk_widget is not None:
            self.osk_height = 180 if area.height < 900 else 220
            self.osk_widget.set_size_request(-1, self.osk_height)
            self.osk_widget.set_visible(keyboard_shown)
            if keyboard_shown:
                self.osk_widget.show_all()
                width = max(width, self.osk_widget.get_preferred_width()[1] + 24)
                height += self.osk_height + 12
        width, height = min(width, area.width - 24), min(height, area.height - 24)
        self.window.set_default_size(width, height)
        self.window.set_size_request(width, height)
        self.window.resize(width, height)
        x = area.x + 12 if getattr(self, "menu_left", False) else area.x + (area.width - width) // 2
        self.window.move(x, area.y + area.height - height - 10)
        self.render()

    # ── Clavier tactile intégré (tablette sans clavier) ──

    def keyboard_open(self):
        return getattr(self, "osk_widget", None) is not None and self.osk_widget.get_visible()

    def osk_auto(self):
        try: return not self.osk.keyboards()
        except Exception: return False

    def osk_toggle(self):
        if self.osk_widget is None: return
        # Le menu change de taille et de place : la perte de focus ou le clic
        # vus pendant ce temps ne doivent pas le fermer (⌨ avec une recherche
        # remplie fermait tout le menu).
        self.resized_at = time.monotonic()
        self.place(not self.osk_widget.get_visible())
        # Curseur en fin de texte : grab_focus() sélectionnait toute la
        # recherche et la touche suivante remplaçait « par ».
        self.search.grab_focus_without_selecting()
        self.search.set_position(-1)

    def osk_send(self, key, _modifiers=()):
        """Touches du clavier intégré vers la recherche du menu."""
        entry = self.search
        position = entry.get_position()
        if key == "BackSpace":
            if position > 0: entry.delete_text(position - 1, position)
        elif key == "Return":
            self.first_result()
        elif key == "Escape":
            self.hide()
        elif key in ("Left", "Right"):
            entry.set_position(max(0, position + (1 if key == "Right" else -1)))
        elif len(key) == 1:
            entry.insert_text(key, position)
            entry.set_position(position + 1)

    def build(self):
        self.window = Gtk.ApplicationWindow(application=self, title=_t("Démarrer Winlux"))
        self.window.set_decorated(False)
        self.window.set_resizable(False)
        self.window.set_skip_taskbar_hint(True)
        self.window.set_skip_pager_hint(True)
        self.window.set_type_hint(Gdk.WindowTypeHint.UTILITY)
        self.window.set_keep_above(True)
        self.window.connect("delete-event", lambda *_: self.hide())
        self.window.connect("focus-out-event", self.blur)
        self.window.connect("key-press-event", self.key)
        self.window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        self.window.connect("button-press-event", self.outside_click)
        # La saisie du pointeur n'aboutit que sur une fenêtre réellement
        # visible ; openbox mappe d'abord la fenêtre dans un cadre encore
        # caché (NOT_VIEWABLE), d'où quelques nouvelles tentatives.
        self.window.connect("map-event", lambda *_: self.grab_soon())
        self.css = Gtk.CssProvider()
        # Au-dessus du gtk.css utilisateur (winlux-personalize) : le menu garde
        # sa propre palette, lisible dans les deux thèmes.
        Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), self.css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)
        settings = Gtk.Settings.get_default()
        for name in ("gtk-theme-name", "gtk-application-prefer-dark-theme"):
            settings.connect("notify::" + name, lambda *_: self.apply_theme())
        self.apply_theme()
        self.window.set_name("winlux-start")
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.window.add(outer)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18, margin=28)
        self.body = body
        outer.pack_start(body, True, True, 0)
        self.search = Gtk.SearchEntry(placeholder_text=_t("Applications, fichiers et paramètres"))
        self.search.connect("search-changed", lambda *_: self.render())
        self.search.connect("activate", self.first_result)
        search_row = Gtk.Box(spacing=8)
        search_row.pack_start(self.search, True, True, 0)
        body.pack_start(search_row, False, False, 0)
        self.osk = osk_module()
        self.osk_widget, self.osk_height = None, 0
        if self.osk is not None:
            try:
                self.osk_widget = self.osk.keyboard_widget(send=self.osk_send, on_close=self.osk_toggle)
            except Exception:
                self.osk_widget = None
        if self.osk_widget is not None:
            self.osk_height = 220
            self.osk_widget.set_size_request(-1, self.osk_height)
            self.osk_widget.set_margin_start(12)
            self.osk_widget.set_margin_end(12)
            self.osk_widget.set_no_show_all(True)
            # Bouton clavier à droite de la recherche, toujours visible : l'ouvre
            # ou le ferme, avec ou sans clavier physique. (Une icône dans le
            # champ ne tenait pas : GtkSearchEntry y met et enlève sa croix
            # d'effacement.)
            self.osk_button = Gtk.Button(label="⌨")
            self.osk_button.set_name("winlux-start-osk")
            self.osk_button.set_can_focus(False)
            self.osk_button.set_tooltip_text(_t("Clavier tactile"))
            self.osk_button.set_size_request(48, -1)
            self.osk_button.connect("clicked", lambda *_: self.osk_toggle())
            search_row.pack_end(self.osk_button, False, False, 0)
        header = Gtk.Box()
        self.heading = Gtk.Label(label=_t("Épinglé"), xalign=0)
        self.heading.get_style_context().add_class("heading")
        header.pack_start(self.heading, True, True, 0)
        self.toggle = Gtk.Button(label=_t("Toutes les applications  ›"))
        on_click(self.toggle, self.toggle_list)
        header.pack_end(self.toggle, False, False, 0)
        body.pack_start(header, False, False, 0)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        self.content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        scroll.add(self.content)
        body.pack_start(scroll, True, True, 0)
        footer = Gtk.Box(spacing=12)
        footer.get_style_context().add_class("footer")
        footer.pack_start(Gtk.Image.new_from_icon_name("avatar-default-symbolic", Gtk.IconSize.LARGE_TOOLBAR), False, False, 0)
        footer.pack_start(Gtk.Label(label=account_name(), xalign=0), True, True, 0)
        power = Gtk.Button.new_from_icon_name("system-shutdown-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
        power.set_tooltip_text(_t("Options d’alimentation"))
        on_click(power, lambda: self.power(power))
        footer.pack_end(power, False, False, 0)
        outer.pack_end(footer, False, False, 0)
        if self.osk_widget is not None:
            outer.pack_end(self.osk_widget, False, False, 0)

    def apply_theme(self):
        """Menu clair en thème clair, sombre en thème sombre, comme Windows 11.

        Chaque nœud qui porte du texte ou une icône symbolique reçoit sa couleur :
        sinon le thème GTK impose la sienne (Adwaita clair : texte gris foncé des
        boutons et des champs) sur le fond du menu et le texte disparaît."""
        self.css.load_from_data(MENU_CSS.substitute(PALETTES["dark" if dark_theme() else "light"]).encode())

    def hide(self):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        self.window.hide()
        return True

    def grab_soon(self):
        self.grab_attempts = 0
        GLib.timeout_add(30, self.grab_pointer)
        return False

    def grab_pointer(self):
        if self.window.get_visible() and not self.context:
            # owner_events=True : les clics dans le menu restent livrés aux boutons ;
            # seuls les clics hors de nos fenêtres reviennent à outside_click.
            # Avec False, chaque clic partait vers la fenêtre principale et le
            # menu ne réagissait que lorsque la saisie avait échoué (1 fois sur 2).
            self.pointer_status=self.window.get_display().get_default_seat().grab(self.window.get_window(),Gdk.SeatCapabilities.POINTER,
                True,None,None,None,None)
            if self.pointer_status==Gdk.GrabStatus.SUCCESS:self.window.grab_add()
            elif self.pointer_status==Gdk.GrabStatus.NOT_VIEWABLE:
                self.grab_attempts = getattr(self, "grab_attempts", 0) + 1
                return self.grab_attempts < 30
        return False

    def settling(self):
        return time.monotonic() - getattr(self, "resized_at", 0) < 0.8

    def outside_click(self, _, event):
        if self.settling(): return False
        x,y=self.window.get_position();width,height=self.window.get_size()
        if not (x<=event.x_root<x+width and y<=event.y_root<y+height):return self.hide()
        return False

    def blur(self, *_):
        def later():
            if not self.window.is_active() and not self.context and not self.settling(): self.hide()
            return False
        GLib.timeout_add(150, later)
        return False

    def key(self, _, event):
        return self.hide() if event.keyval == Gdk.KEY_Escape else False

    def toggle_list(self, *_):
        # « Retour » depuis une recherche : effacer la recherche (le texte
        # tapé gardait sinon les résultats affichés, bouton sans effet).
        if self.search.get_text().strip():
            self.all_apps = False
            self.search.set_text("")
        else:
            self.all_apps = not self.all_apps
        self.render()

    def first_result(self, *_):
        # Même ordre que la liste : applications, Paramètres, fichiers.
        query=fold(self.search.get_text().strip())
        found=settings_for(query) if query else []
        if self.results:self.launch(self.results[0])
        elif found:self.open_setting(found[0][1])
        elif self.file_results:self.open_file(self.file_results[0])

    def launch(self, app):
        try:
            app.launch([], self.window.get_display().get_app_launch_context())
            self.state["recent"] = ([app.get_id()] + [value for value in self.state.get("recent", []) if value != app.get_id()])[:6]
            self.save()
            self.hide()
        except GLib.Error as error:
            dialog = Gtk.MessageDialog(transient_for=self.window, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text=_t("Impossible d’ouvrir l’application"))
            dialog.format_secondary_text(str(error))
            dialog.run()
            dialog.destroy()

    def app_button(self, app, grid=False):
        button = Gtk.Button()
        if grid: button.set_size_request(88, 82)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL if grid else Gtk.Orientation.HORIZONTAL, spacing=6)
        icon = app.get_icon() or Gio.ThemedIcon.new("application-x-executable")
        image = Gtk.Image.new_from_gicon(icon, Gtk.IconSize.DIALOG if grid else Gtk.IconSize.LARGE_TOOLBAR)
        image.set_pixel_size(32 if grid else 24)
        box.pack_start(image, False, False, 0)
        label = Gtk.Label(label=app.get_display_name(), xalign=0.5 if grid else 0)
        label.set_ellipsize(3)
        label.set_max_width_chars(12 if grid else 50)
        if grid:
            label.set_line_wrap(True)
            label.set_lines(2)
        label.get_style_context().add_class("app-name")
        box.pack_start(label, True, True, 0)
        button.add(box)
        button.set_tooltip_text(app.get_display_name())
        on_click(button, lambda: self.launch(app))
        button.connect("button-press-event", lambda widget, event: self.app_context(widget, event, app))
        return button

    def render(self):
        if not self.window or not hasattr(self, "apps"): return
        for widget in self.content.get_children(): widget.destroy()
        query = fold(self.search.get_text().strip())
        listing = self.all_apps or bool(query)
        self.heading.set_text(_t("Résultats") if query else _t("Toutes les applications") if listing else _t("Épinglé"))
        self.toggle.set_label(_t("‹  Retour") if listing else _t("Toutes les applications  ›"))
        self.results = sorted((app for app in self.apps.values() if matches(app, query)),
                              key=lambda app: (rank(app.get_display_name(), query), app.get_display_name().casefold()))
        if listing:
            # Ordre des résultats : applications (le nom qui commence par la
            # recherche d'abord), puis pages des Paramètres, puis fichiers. Avec
            # « par », un Parallel.dll téléchargé passait avant l'application
            # Paramètres et Entrée l'ouvrait.
            previous = None
            if not query: self.results.sort(key=lambda app: (category(app), app.get_display_name().casefold()))
            for app in self.results:
                group = category(app)
                if not query and group != previous:
                    heading = Gtk.Label(label=group, xalign=0, margin_top=10)
                    heading.get_style_context().add_class("heading")
                    self.content.pack_start(heading, False, False, 0)
                    previous = group
                self.content.pack_start(self.app_button(app), False, False, 0)
            if query:
                for label, page in settings_for(query):
                    button = Gtk.Button(label=_t("Paramètres · ") + label)
                    on_click(button, lambda target=page: self.open_setting(target))
                    self.content.pack_start(button, False, False, 0)
                if query != self.file_query:
                    self.file_query = query
                    self.file_generation += 1
                    self.file_results = []
                    if len(query) >= 2:
                        threading.Thread(target=self.find_files, args=(query, self.file_generation), daemon=True).start()
                for path in self.file_results:
                    button = Gtk.Button(label=_t("Fichier · ") + str(path.relative_to(Path.home())))
                    on_click(button, lambda target=path: self.open_file(target))
                    self.content.pack_start(button, False, False, 0)
                if len(self.file_results) == 30:
                    self.content.pack_start(Gtk.Label(label=_t("30 premiers fichiers affichés. Précisez la recherche."), xalign=0), False, False, 0)
            if not self.results and not (query and (self.file_results or any(query in fold(label) for label, _ in SETTINGS))):
                self.content.pack_start(Gtk.Label(label=_t("Aucune application trouvée")), False, False, 0)
        else:
            grid = Gtk.Grid(column_homogeneous=True, column_spacing=4, row_spacing=12)
            self.content.pack_start(grid, False, False, 0)
            pinned = [self.apps[value] for value in self.state.get("pins", DEFAULTS) if value in self.apps]
            for index, app in enumerate(pinned): grid.attach(self.app_button(app, True), index % 6, index // 6, 1, 1)
            # Clavier tactile ouvert : la place manque, les épinglés seuls (le
            # titre « Recommandations » était coupé par le bas de la liste).
            if self.keyboard_open():
                self.content.show_all()
                return
            heading = Gtk.Label(label=_t("Recommandations"), xalign=0, margin_top=15)
            heading.get_style_context().add_class("heading")
            self.content.pack_start(heading, False, False, 0)
            recent = [self.apps[value] for value in self.state.get("recent", []) if value in self.apps]
            for app in recent[:4]: self.content.pack_start(self.app_button(app), False, False, 0)
            if not recent:
                self.content.pack_start(Gtk.Label(label=_t("Vos applications récemment ouvertes apparaîtront ici."), xalign=0, wrap=True), False, False, 0)
        self.content.show_all()

    def open_setting(self, page):
        subprocess.Popen(["winlux-shell-app", "settings", str(page)])
        self.hide()

    def open_file(self, path):
        self.hide()
        try:Gio.AppInfo.launch_default_for_uri(path.as_uri(),self.window.get_display().get_app_launch_context())
        except GLib.Error as error:
            dialog=Gtk.MessageDialog(transient_for=self.window,modal=True,message_type=Gtk.MessageType.ERROR,
                buttons=Gtk.ButtonsType.CLOSE,text=_t("Impossible d’ouvrir ce fichier"),secondary_text=str(error))
            dialog.run();dialog.destroy()

    def find_files(self, query, generation):
        # Bounded name search in visible personal directories. Never follows
        # directory symlinks or scans the system disks from the UI thread.
        matches, visited = [], 0
        for directory, folders, files in os.walk(Path.home(), followlinks=False):
            if generation != self.file_generation: return
            folders[:] = [name for name in folders if not name.startswith(".") and not Path(directory, name).is_symlink()]
            for name in folders + files:
                visited += 1
                if not name.startswith(".") and query in fold(name): matches.append(Path(directory, name))
                if len(matches) >= 30 or visited >= 30000: break
            if len(matches) >= 30 or visited >= 30000: break
        def deliver():
            if generation == self.file_generation:
                self.file_results = matches
                self.render()
            return False
        GLib.idle_add(deliver)

    def menu(self, widget, event, actions):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        menu = Gtk.Menu()
        menu.get_style_context().add_class("winlux-menu")
        self.context = menu
        for entry in actions:
            title, callback = entry[0], entry[1]
            item = Gtk.MenuItem()
            row = Gtk.Box(spacing=12)
            if len(entry) > 2:
                row.pack_start(Gtk.Image.new_from_icon_name(entry[2], Gtk.IconSize.MENU), False, False, 0)
            row.pack_start(Gtk.Label(label=title, xalign=0), True, True, 0)
            item.add(row)
            item.connect("activate", lambda _, action=callback: action())
            menu.append(item)
        def deactivate(*_):
            self.context=None
            GLib.idle_add(self.grab_pointer)
        menu.connect("deactivate", deactivate)
        menu.show_all()
        if event: menu.popup_at_pointer(event)
        else: menu.popup_at_widget(widget, Gdk.Gravity.NORTH_EAST, Gdk.Gravity.SOUTH_EAST, None)

    def app_context(self, widget, event, app):
        if event.button != 3: return False
        identifier = app.get_id()
        def pin():
            pins = self.state.setdefault("pins", list(DEFAULTS))
            if identifier in pins: pins.remove(identifier)
            else: pins.append(identifier)
            self.save()
            self.render()
        def shortcut():
            source = app.get_filename()
            desktop = Path(GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_DESKTOP) or str(Path.home() / "Bureau"))
            desktop.mkdir(parents=True, exist_ok=True)
            target = desktop / Path(source).name
            if not target.exists():
                shutil.copyfile(source, target)
                target.chmod(0o755)
                subprocess.run(["gio", "set", "-t", "string", str(target), "metadata::trusted", "true"],
                               check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                # xfdesktop 4.20 ne fait confiance qu'à la somme SHA-256 du lanceur.
                checksum = hashlib.sha256(target.read_bytes()).hexdigest()
                subprocess.run(["gio", "set", "-t", "string", str(target), "metadata::xfce-exe-checksum", checksum],
                               check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def action(name):
            self.hide()
            app.launch_action(name, None)
        # Actions déclarées par l'application (« Mettre à jour Claude Code »,
        # « Nouvelle fenêtre »…), comme la liste de raccourcis de Windows.
        actions = [(app.get_action_name(name), lambda name=name: action(name))
                   for name in (app.list_actions() if hasattr(app, "list_actions") else [])]
        self.menu(widget, event, [(_t("Ouvrir"), lambda: self.launch(app)), *actions,
                                 (_t("Détacher du menu Démarrer") if identifier in self.state.get("pins", DEFAULTS) else _t("Épingler au menu Démarrer"), pin),
                                 (_t("Créer un raccourci sur le bureau"), shortcut)])
        return True

    def power(self, widget):
        # Confirmation, fermeture des applications et arrêt réel : winlux-power
        # (l'ancien chemin appelait le poweroff de systemd, absent, et rien ne
        # s'éteignait).
        def action(name):
            self.hide()
            subprocess.Popen(["/usr/bin/winlux-power", "ask", name], start_new_session=True)
        def lock(how="now"):
            self.hide()
            subprocess.Popen(["/usr/bin/winlux-lock", how], start_new_session=True)
        def sleep():
            # Veille : action « user » du relais root (winlux --power suspend) ;
            # l'écran de verrouillage revient au réveil (LOCK_ON_RESUME).
            self.hide()
            subprocess.Popen(["winlux-control", "power-suspend"], start_new_session=True,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def logoff():
            # Ferme la session de ce compte (applications comprises), comme
            # « Se déconnecter » de Windows ; le relais root vérifie le compte.
            self.hide()
            subprocess.Popen(["winlux-control", "session-logoff", current_account()], start_new_session=True,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # Ordre de Windows 11 : Verrouiller, Se déconnecter, Changer
        # d'utilisateur, Mettre en veille, Redémarrer, Arrêter.
        entries = [(_t("Verrouiller"), lock, "system-lock-screen-symbolic")]
        if power_menu_has_logoff():
            entries.append((_t("Se déconnecter"), logoff, "system-log-out-symbolic"))
        if power_menu_has_switch():
            entries.append((_t("Changer d’utilisateur"), lambda: lock("switch"), "system-switch-user-symbolic"))
        if power_menu_has_sleep():
            entries.append((_t("Mettre en veille"), sleep, "weather-clear-night-symbolic"))
        self.menu(widget, None, entries + [(_t("Redémarrer"), lambda: action("reboot"), "view-refresh-symbolic"),
                                           (_t("Arrêter"), lambda: action("shutdown"), "system-shutdown-symbolic")])


# Anciennes épingles par défaut remplacées : l'épingle Edge des premières
# versions devient le « Navigateur » générique (aucune marque imposée).
RENAMED_PINS = {"winlux-browser-edge.desktop": "winlux-browser.desktop"}


def rank(text, query):
    """0 : le nom commence par la recherche ; 1 : un de ses mots ; 2 : ailleurs."""
    text = fold(text)
    if text.startswith(query): return 0
    if any(word.startswith(query) for word in re.split(r"[\W_]+", text)): return 1
    return 2


def settings_for(query):
    """Pages des Paramètres trouvées, les mieux classées d'abord."""
    return sorted(((label, page) for label, page in SETTINGS if query in fold(label)), key=lambda item: rank(item[0], query))


def matches(app, query):
    """Nom affiché ou mot-clé du lanceur (« boutique » trouve le Store)."""
    if query in fold(app.get_display_name()): return True
    keywords = app.get_keywords() if hasattr(app, "get_keywords") else []
    return bool(query) and any(query in fold(word) for word in keywords or [])


def auto_pin(state, apps):
    """Lanceurs marqués X-Winlux-Pin=true (Store, boutiques choisies) épinglés
    une seule fois par compte : détachés ensuite, ils le restent. Une boutique
    retirée puis choisie de nouveau est épinglée de nouveau."""
    done = [ident for ident in state.get("auto_pinned", []) if ident in apps]
    pins = state.setdefault("pins", list(DEFAULTS))
    for ident, app in apps.items():
        if ident in done or not hasattr(app, "get_boolean") or not app.has_key("X-Winlux-Pin"): continue
        if not app.get_boolean("X-Winlux-Pin"): continue
        if ident not in pins: pins.append(ident)
        done.append(ident)
    if done == state.get("auto_pinned"): return False
    state["auto_pinned"] = done
    return True


def migrate_pins(pins):
    """Épingles enregistrées, anciennes épingles par défaut remplacées, sans doublon."""
    if not isinstance(pins, list):
        return list(DEFAULTS)
    result = []
    for pin in pins:
        pin = RENAMED_PINS.get(pin, pin)
        if pin not in result:
            result.append(pin)
    return result


def power_menu_has_sleep():
    """Veille proposée seulement si le noyau la sait faire (/sys/power/state)."""
    try:
        states = Path(os.environ.get("WINLUX_POWER_STATE", "/sys/power/state")).read_text().split()
    except OSError:
        return False
    return "mem" in states or "freeze" in states


def current_account():
    """Compte de la session (USER, sinon celui du processus)."""
    return os.environ.get("USER") or pwd.getpwuid(os.getuid()).pw_name


def power_menu_has_logoff():
    """« Se déconnecter » dans une session installée ; la session Live n'a
    pas d'écran de connexion vers lequel revenir."""
    return not Path(os.environ.get("WINLUX_LIVE_MARKER", "/etc/winlux/live-session")).exists()


def power_menu_has_switch():
    """« Changer d'utilisateur » dès qu'un autre compte existe (comme Windows)."""
    try:
        lines = Path(os.environ.get("WINLUX_USERS_CONF", "/etc/winlux/users.conf")).read_text(errors="replace").splitlines()
    except OSError:
        return False
    names = {line.split(":", 1)[0] for line in lines if line.split(":", 1)[0]} - {"root"}
    return len(names) >= 2


if __name__ == "__main__":
    raise SystemExit(Start().run())
