#!/usr/bin/python3
"""Windows 11 inspired native Start: real XDG apps, pins and context actions."""
import json
import os
import pwd
from pathlib import Path
import shutil
from string import Template
import subprocess
import threading
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
    session live (root)."""
    try:
        entry = pwd.getpwuid(os.getuid())
    except KeyError:
        return "Winlux"
    if entry.pw_uid == 0:
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
    'Son': ('Sound', 'Sonido', 'Sound'),
    'Alimentation batterie veille': ('Power battery sleep', 'Energía batería suspensión', 'Energie Akku Standby'),
    'Clavier': ('Keyboard', 'Teclado', 'Tastatur'),
    'Souris': ('Mouse', 'Ratón', 'Maus'),
    'Personnalisation fond écran thème': ('Personalization wallpaper theme', 'Personalización fondo pantalla tema', 'Personalisierung Hintergrund Design'),
    'Mises à jour': ('Updates', 'Actualizaciones', 'Updates'),
    'Écran résolution': ('Display resolution', 'Pantalla resolución', 'Bildschirm Auflösung'),
    'À propos version': ('About version', 'Acerca de versión', 'Info Version'),
    'Navigateurs': ('Browsers', 'Navegadores', 'Browser'),
    'Partage de dossiers et transfert des réglages': ('Folder sharing and settings transfer', 'Compartir carpetas y transferir ajustes', 'Ordnerfreigabe und Übertragung der Einstellungen'),
    'Paramètres': ('Settings', 'Configuración', 'Einstellungen'),
    'Bureautique': ('Office', 'Oficina', 'Büro'),
    'Graphisme': ('Graphics', 'Gráficos', 'Grafik'),
    'Audio et vidéo': ('Audio and video', 'Audio y vídeo', 'Audio und Video'),
    'Jeux': ('Games', 'Juegos', 'Spiele'),
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
    'Arrêter': ('Shut down', 'Apagar', 'Herunterfahren'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


STATE = Path.home() / ".config/winlux/start.json"
DEFAULTS = ["winlux-explorer.desktop", "winlux-browser-edge.desktop", "org.xfce.mousepad.desktop",
            "winlux-settings.desktop", "winlux-terminal.desktop", "winlux-updates.desktop"]
SETTINGS = [(_t("Système et alimentation"), 0), (_t("Applications et démarrage"), 1), (_t("Réseau Wi-Fi"), 2),
            (_t("Son"), 16), (_t("Alimentation batterie veille"), 23), (_t("Clavier"), 4), (_t("Souris"), 4), (_t("Personnalisation fond écran thème"), 21),
            (_t("Mises à jour"), 7), (_t("Écran résolution"), 8), (_t("À propos version"), 9), (_t("Navigateurs"), 11),
            (_t("Partage de dossiers et transfert des réglages"), 28)]
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

SESSION_SERVICES = {"picom.desktop"}

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
        # Services de la session livrés avec une entrée de menu par leur paquet.
        self.apps = {app.get_id(): app for app in Gio.AppInfo.get_all()
                     if listed(app) and app.get_id() not in SESSION_SERVICES}
        self.all_apps = False
        self.search.set_text("")
        self.render()
        monitor = self.window.get_display().get_primary_monitor()
        if monitor is None: monitor = self.window.get_display().get_monitor(0)
        area = monitor.get_workarea()
        width, height = min(660, area.width - 24), min(650, area.height - 24)
        self.window.set_default_size(width, height)
        self.window.set_size_request(width, height)
        # Comme Windows 11 : barre alignée à gauche, le menu s'ouvre au-dessus
        # du bouton Démarrer ; barre centrée ou ouverture au clavier, il est
        # centré.
        x = area.x + (area.width - width) // 2
        _, pointer_x, pointer_y = self.window.get_display().get_default_seat().get_pointer().get_position()
        if pointer_y >= area.y + area.height and area.x <= pointer_x < area.x + area.width // 3:
            x = area.x + 12
        self.window.move(x, area.y + area.height - height - 10)
        self.window.show_all()
        self.window.present()
        self.search.grab_focus()

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
        outer.pack_start(body, True, True, 0)
        self.search = Gtk.SearchEntry(placeholder_text=_t("Applications, fichiers et paramètres"))
        self.search.connect("search-changed", lambda *_: self.render())
        self.search.connect("activate", self.first_result)
        body.pack_start(self.search, False, False, 0)
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

    def outside_click(self, _, event):
        x,y=self.window.get_position();width,height=self.window.get_size()
        if not (x<=event.x_root<x+width and y<=event.y_root<y+height):return self.hide()
        return False

    def blur(self, *_):
        def later():
            if not self.window.is_active() and not self.context: self.hide()
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
        query=fold(self.search.get_text().strip())
        setting=next((page for label,page in SETTINGS if query and query in fold(label)),None)
        if setting is not None:self.open_setting(setting)
        elif self.results:self.launch(self.results[0])
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
        self.results = sorted((app for app in self.apps.values() if query in fold(app.get_display_name())), key=lambda app: app.get_display_name().casefold())
        if listing:
            if query:
                for label, page in SETTINGS:
                    if query in fold(label):
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
            if not self.results and not (query and (self.file_results or any(query in fold(label) for label, _ in SETTINGS))):
                self.content.pack_start(Gtk.Label(label=_t("Aucune application trouvée")), False, False, 0)
        else:
            grid = Gtk.Grid(column_homogeneous=True, column_spacing=4, row_spacing=12)
            self.content.pack_start(grid, False, False, 0)
            pinned = [self.apps[value] for value in self.state.get("pins", DEFAULTS) if value in self.apps]
            for index, app in enumerate(pinned): grid.attach(self.app_button(app, True), index % 6, index // 6, 1, 1)
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
        self.menu(widget, event, [(_t("Ouvrir"), lambda: self.launch(app)),
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
        def lock():
            self.hide()
            subprocess.Popen(["/usr/bin/winlux-lock", "now"], start_new_session=True)
        self.menu(widget, None, [(_t("Verrouiller"), lock, "system-lock-screen-symbolic"),
                                 (_t("Redémarrer"), lambda: action("reboot"), "view-refresh-symbolic"),
                                 (_t("Arrêter"), lambda: action("shutdown"), "system-shutdown-symbolic")])

if __name__ == "__main__":
    raise SystemExit(Start().run())
