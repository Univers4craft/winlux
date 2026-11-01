#!/usr/bin/python3
"""« Ouvrir avec… » : choisir le programme (Linux ou Windows) qui ouvre un fichier.

Appelé par l'Explorateur Winlux avec le chemin du fichier (chemin Windows
C:\\…/Z:\\… ou chemin Linux). Les programmes Windows y figurent dès que leur
installateur a déclaré le type (lanceurs winlux-assoc-*.desktop). « Toujours
utiliser ce programme » en fait le programme par défaut de ce type.
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
    "Ouvrir avec": ("Open with", "Abrir con", "Öffnen mit"),
    "Comment voulez-vous ouvrir « {name} » ?": ("How do you want to open “{name}”?", "¿Cómo quiere abrir «{name}»?", "Wie möchten Sie „{name}“ öffnen?"),
    "Toujours utiliser ce programme pour les fichiers {kind}": ("Always use this app to open {kind} files",
        "Usar siempre esta aplicación para los archivos {kind}", "Immer diese App für {kind}-Dateien verwenden"),
    "Ouvrir": ("Open", "Abrir", "Öffnen"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Fichier introuvable : {path}": ("File not found: {path}", "Archivo no encontrado: {path}", "Datei nicht gefunden: {path}"),
    "Impossible d’ouvrir le fichier": ("Could not open the file", "No se pudo abrir el archivo", "Die Datei konnte nicht geöffnet werden"),
}

def _t(text, **values):
    columns = {"en": 0, "es": 1, "de": 2}
    if LANG in columns and text in _TR: text = _TR[text][columns[LANG]]
    return text.format(**values) if values else text

def linux_path(value):
    """C:\\ est le dossier personnel, Z:\\ la racine (comme le runtime) ; file:// accepté."""
    if value.startswith("file://"): return Gio.File.new_for_uri(value).get_path() or value
    if len(value) >= 2 and value[1] == ":" and value[0].isalpha():
        rest = value[2:].replace("\\", "/").lstrip("/")
        letter = value[0].upper()
        if letter == "Z": return "/" + rest
        if letter == "C": return str(Path.home() / rest)
    return value

def choose(path):
    target = Gio.File.new_for_path(path)
    info = target.query_info("standard::content-type", Gio.FileQueryInfoFlags.NONE, None)
    kind = info.get_content_type() or "application/octet-stream"
    dialog = Gtk.AppChooserDialog.new(None, 0, target)
    dialog.set_title(_t("Ouvrir avec"))
    dialog.set_heading(_t("Comment voulez-vous ouvrir « {name} » ?", name=target.get_basename()))
    dialog.get_widget().set_show_all(True)
    extension = Path(path).suffix.lower() or Gio.content_type_get_description(kind)
    always = Gtk.CheckButton(label=_t("Toujours utiliser ce programme pour les fichiers {kind}", kind=extension))
    always.set_margin_start(12)
    always.set_margin_bottom(6)
    dialog.get_content_area().pack_end(always, False, False, 0)
    always.show()
    for button, label in ((Gtk.ResponseType.CANCEL, "Annuler"), (Gtk.ResponseType.OK, "Ouvrir")):
        widget = dialog.get_widget_for_response(button)
        if widget is not None: widget.set_label(_t(label))
    status = 1
    if dialog.run() == Gtk.ResponseType.OK:
        app = dialog.get_app_info()
        if app is not None:
            try:
                if always.get_active(): app.set_as_default_for_type(kind)
                app.launch([target], None)
                status = 0
            except GLib.Error as error:
                message = Gtk.MessageDialog(message_type=Gtk.MessageType.ERROR, buttons=Gtk.ButtonsType.CLOSE,
                                            text=_t("Impossible d’ouvrir le fichier"))
                message.format_secondary_text(error.message)
                message.run()
                message.destroy()
    dialog.destroy()
    return status

def main():
    if len(sys.argv) < 2:
        print("usage : winlux-open-with FICHIER", file=sys.stderr)
        return 2
    path = linux_path(sys.argv[1])
    if not Path(path).exists():
        dialog = Gtk.MessageDialog(message_type=Gtk.MessageType.ERROR, buttons=Gtk.ButtonsType.CLOSE,
                                   text=_t("Fichier introuvable : {path}", path=path))
        dialog.run()
        return 1
    return choose(path)

if __name__ == "__main__": raise SystemExit(main())
