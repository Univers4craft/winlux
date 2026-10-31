#!/usr/bin/python3
"""Applications par défaut : quel programme (Linux ou Windows) ouvre chaque type de fichier.

Les programmes Windows y figurent dès que leur installateur a déclaré une
association dans le registre (winlux-applications crée leurs lanceurs cachés
winlux-assoc-*.desktop). Le choix est écrit dans ~/.config/mimeapps.list par
GIO ; winlux-applications ne l'écrase plus ensuite.
"""
import os
from pathlib import Path
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gio, GLib, Gtk

ROOT = os.environ.get("WINLUX_ROOT", "")

def interface_lang():
    """WINLUX_LANG, sinon /etc/winlux/locale.conf (Paramètres > Langue), sinon LANG."""
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path(ROOT + "/etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="): code = line[5:].strip()
        except OSError: pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"

LANG = interface_lang()
# Texte français -> (anglais, espagnol, allemand).
_TR = {
    "Applications par défaut": ("Default apps", "Aplicaciones predeterminadas", "Standard-Apps"),
    "Choisissez le programme qui ouvre chaque type de fichier. Les programmes Windows installés apparaissent ici dès qu’ils déclarent les fichiers qu’ils savent ouvrir.":
        ("Choose the program that opens each file type. Installed Windows programs appear here as soon as they declare the files they can open.",
         "Elija el programa que abre cada tipo de archivo. Los programas de Windows instalados aparecen aquí en cuanto declaran los archivos que pueden abrir.",
         "Wählen Sie das Programm, das jeden Dateityp öffnet. Installierte Windows-Programme erscheinen hier, sobald sie die Dateien angeben, die sie öffnen können."),
    "Rechercher un type de fichier (.txt, image, PDF…)": ("Search a file type (.txt, image, PDF…)",
        "Buscar un tipo de archivo (.txt, imagen, PDF…)", "Dateityp suchen (.txt, Bild, PDF…)"),
    "Programmes Windows": ("Windows programs", "Programas de Windows", "Windows-Programme"),
    "Types de fichiers": ("File types", "Tipos de archivo", "Dateitypen"),
    "Aucun programme Windows n’a encore déclaré de type de fichier.": ("No Windows program has declared a file type yet.",
        "Ningún programa de Windows ha declarado todavía un tipo de archivo.", "Noch kein Windows-Programm hat einen Dateityp angegeben."),
    "Utiliser pour tous ses types": ("Use for all its types", "Usar para todos sus tipos", "Für alle seine Typen verwenden"),
    "Par défaut pour tous ses types": ("Default for all its types", "Predeterminado para todos sus tipos", "Standard für alle seine Typen"),
    "{count} types de fichiers": ("{count} file types", "{count} tipos de archivo", "{count} Dateitypen"),
    "Impossible d’enregistrer ce choix": ("Could not save this choice", "No se pudo guardar esta elección", "Diese Auswahl konnte nicht gespeichert werden"),
    "Aucun type ne correspond à la recherche.": ("No type matches the search.", "Ningún tipo coincide con la búsqueda.", "Kein Typ entspricht der Suche."),
}

def _t(text, **values):
    columns = {"en": 0, "es": 1, "de": 2}
    if LANG in columns and text in _TR: text = _TR[text][columns[LANG]]
    return text.format(**values) if values else text

# Types que tout le monde ouvre, présentés même sans programme Windows.
COMMON = ["text/plain", "application/pdf", "image/jpeg", "image/png", "image/gif", "image/webp", "image/svg+xml",
          "audio/mpeg", "audio/flac", "audio/ogg", "video/mp4", "video/x-matroska", "video/webm", "text/html",
          "application/zip", "application/x-7z-compressed", "application/x-rar",
          "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
          "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
          "application/vnd.oasis.opendocument.text", "text/csv", "application/json", "text/x-python"]

def windows_apps():
    """Hidden launchers of Windows programs that declared associations."""
    apps = []
    for app in Gio.AppInfo.get_all():
        if (app.get_id() or "").startswith("winlux-assoc-") and isinstance(app, Gio.DesktopAppInfo):
            apps.append((app, [kind for kind in app.get_supported_types() or [] if kind]))
    return sorted(apps, key=lambda item: item[0].get_display_name().casefold())

def describe(kind):
    description = Gio.content_type_get_description(kind)
    extensions = sorted(set(extensions_of(kind)))
    return description, extensions

_GLOBS = None
def extensions_of(kind):
    global _GLOBS
    if _GLOBS is None:
        _GLOBS = {}
        data = [Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))]
        data += [Path(item) for item in os.environ.get("XDG_DATA_DIRS", "/usr/local/share:/usr/share").split(":") if item]
        for directory in data:
            try: lines = (directory / "mime/globs2").read_text(errors="replace").splitlines()
            except OSError: continue
            for line in lines:
                parts = line.split(":")
                if len(parts) >= 3 and parts[2].startswith("*.") and not any(c in parts[2][2:] for c in "*?["):
                    _GLOBS.setdefault(parts[1], []).append(parts[2][1:].lower())
    return _GLOBS.get(kind, [])

class DefaultApps(Gtk.Window):
    def __init__(self):
        super().__init__(title=_t("Applications par défaut"))
        self.set_icon_name("preferences-desktop-default-applications")
        self.set_default_size(860, 680)
        self.connect("destroy", Gtk.main_quit)
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=24)
        self.add(outer)
        heading = Gtk.Label(xalign=0)
        heading.set_markup(f"<span size='x-large' weight='bold'>{GLib.markup_escape_text(_t('Applications par défaut'))}</span>")
        outer.pack_start(heading, False, False, 0)
        outer.pack_start(Gtk.Label(label=_t("Choisissez le programme qui ouvre chaque type de fichier. Les programmes Windows installés apparaissent ici dès qu’ils déclarent les fichiers qu’ils savent ouvrir."),
                                   xalign=0, wrap=True), False, False, 0)
        self.search = Gtk.SearchEntry(placeholder_text=_t("Rechercher un type de fichier (.txt, image, PDF…)"))
        self.search.connect("search-changed", lambda *_: self.types.invalidate_filter())
        outer.pack_start(self.search, False, False, 0)
        scroll = Gtk.ScrolledWindow()
        outer.pack_start(scroll, True, True, 0)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        scroll.add(body)

        body.pack_start(self.section(_t("Programmes Windows")), False, False, 0)
        apps = windows_apps()
        if not apps:
            body.pack_start(Gtk.Label(label=_t("Aucun programme Windows n’a encore déclaré de type de fichier."), xalign=0), False, False, 0)
        self.app_buttons = []
        for app, kinds in apps:
            row = Gtk.Box(spacing=12)
            icon = Gtk.Image.new_from_gicon(app.get_icon(), Gtk.IconSize.DND) if app.get_icon() else Gtk.Image()
            row.pack_start(icon, False, False, 0)
            text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            text.pack_start(Gtk.Label(label=app.get_display_name(), xalign=0), False, False, 0)
            summary = ", ".join(sorted({ext for kind in kinds for ext in extensions_of(kind)})[:12]) or _t("{count} types de fichiers", count=len(kinds))
            detail = Gtk.Label(label=summary, xalign=0, wrap=True)
            detail.get_style_context().add_class("dim-label")
            text.pack_start(detail, False, False, 0)
            row.pack_start(text, True, True, 0)
            button = Gtk.Button()
            button.connect("clicked", self.use_everywhere, app, kinds)
            row.pack_end(button, False, False, 0)
            self.app_buttons.append((button, app, kinds))
            body.pack_start(row, False, False, 0)

        body.pack_start(self.section(_t("Types de fichiers")), False, False, 0)
        kinds = list(COMMON)
        for _, declared in apps:
            kinds += [kind for kind in declared if kind not in kinds]
        self.types = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
        self.types.set_filter_func(self.matches)
        self.types.set_placeholder(Gtk.Label(label=_t("Aucun type ne correspond à la recherche.")))
        self.choosers = []
        for kind in kinds:
            description, extensions = describe(kind)
            row = Gtk.Box(spacing=12, margin=6)
            label = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            label.pack_start(Gtk.Label(label=description, xalign=0), False, False, 0)
            ext = Gtk.Label(label=" ".join(extensions[:8]) or kind, xalign=0)
            ext.get_style_context().add_class("dim-label")
            label.pack_start(ext, False, False, 0)
            row.pack_start(label, True, True, 0)
            chooser = Gtk.AppChooserButton(content_type=kind)
            chooser.set_show_default_item(True)
            chooser.set_size_request(300, -1)
            chooser.connect("changed", self.chosen, kind)
            row.pack_end(chooser, False, False, 0)
            line = Gtk.ListBoxRow()
            line.add(row)
            line.search_text = " ".join([description, kind] + extensions).casefold()
            self.types.add(line)
            self.choosers.append(chooser)
        body.pack_start(self.types, False, False, 0)
        self.refresh_buttons()
        self.show_all()

    def section(self, title):
        label = Gtk.Label(xalign=0, margin_top=12)
        label.set_markup(f"<b>{GLib.markup_escape_text(title)}</b>")
        return label

    def matches(self, row):
        query = self.search.get_text().strip().casefold()
        return not query or query.lstrip(".") in row.search_text

    def chosen(self, chooser, kind):
        app = chooser.get_app_info()
        if app is None: return
        current = Gio.AppInfo.get_default_for_type(kind, False)
        if current is not None and current.get_id() == app.get_id(): return
        self.save_default(app, [kind])

    def save_default(self, app, kinds):
        try:
            for kind in kinds: app.set_as_default_for_type(kind)
        except GLib.Error as error:
            dialog = Gtk.MessageDialog(transient_for=self, modal=True, message_type=Gtk.MessageType.ERROR,
                                       buttons=Gtk.ButtonsType.CLOSE, text=_t("Impossible d’enregistrer ce choix"))
            dialog.format_secondary_text(error.message)
            dialog.run()
            dialog.destroy()
        self.refresh_buttons()

    def use_everywhere(self, _, app, kinds):
        self.save_default(app, kinds)
        for chooser in self.choosers: chooser.refresh()

    def refresh_buttons(self):
        for button, app, kinds in self.app_buttons:
            everywhere = bool(kinds) and all((Gio.AppInfo.get_default_for_type(kind, False) or app).get_id() == app.get_id()
                                             and Gio.AppInfo.get_default_for_type(kind, False) is not None for kind in kinds)
            button.set_label(_t("Par défaut pour tous ses types") if everywhere else _t("Utiliser pour tous ses types"))
            button.set_sensitive(not everywhere)

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--list":
        for app, kinds in windows_apps(): print(app.get_id(), app.get_display_name(), ";".join(kinds), sep="\t")
        raise SystemExit(0)
    DefaultApps()
    Gtk.main()
