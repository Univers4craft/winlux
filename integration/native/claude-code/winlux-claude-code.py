#!/usr/bin/python3
"""Claude Code dans Winlux : l'application du menu Démarrer.

Au premier lancement, une fenêtre télécharge la version Windows officielle
de Claude Code (claude.exe), avec une barre de progression. Le fichier est
vérifié par la somme SHA-256 de son manifeste, puis Claude Code s'ouvre dans
le terminal Winlux. Aux lancements suivants, il s'ouvre directement.

La commande « winlux-claude-code install » fait la même installation en
ligne de commande : c'est le chemin de secours. Les deux utilisent le même
emplacement (~/.local/share/winlux/claude-code) et la même commande
~/.local/bin/claude.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import urllib.request

BASE = ("https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819"
        "/claude-code-releases")
DATA = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share") / "winlux/claude-code"
BIN = Path.home() / ".local/bin"
CANAL = os.environ.get("WINLUX_CLAUDE_CANAL", "stable")


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
_TR = {
    "Installer Claude Code": ("Install Claude Code", "Instalar Claude Code", "Claude Code installieren"),
    "Claude Code, l'assistant de programmation d'Anthropic, va être téléchargé (environ 300 Mo) "
    "puis ouvert dans le terminal Winlux.":
        ("Claude Code, Anthropic's coding assistant, will be downloaded (about 300 MB) "
         "and then opened in the Winlux terminal.",
         "Claude Code, el asistente de programación de Anthropic, se descargará (unos 300 MB) "
         "y se abrirá en el terminal de Winlux.",
         "Claude Code, der Programmierassistent von Anthropic, wird heruntergeladen (etwa 300 MB) "
         "und dann im Winlux-Terminal geöffnet."),
    "Recherche de la dernière version…": ("Looking for the latest version…", "Buscando la última versión…",
                                          "Neueste Version wird gesucht…"),
    "Téléchargement de Claude Code {v}…": ("Downloading Claude Code {v}…", "Descargando Claude Code {v}…",
                                          "Claude Code {v} wird heruntergeladen…"),
    "Vérification du fichier…": ("Checking the file…", "Comprobando el archivo…", "Datei wird geprüft…"),
    "Installation…": ("Installing…", "Instalando…", "Installation…"),
    "Claude Code est installé. Ouverture…": ("Claude Code is installed. Opening…",
                                             "Claude Code está instalado. Abriendo…",
                                             "Claude Code ist installiert. Wird geöffnet…"),
    "Le téléchargement a échoué : {e}": ("The download failed: {e}", "La descarga falló: {e}",
                                         "Der Download ist fehlgeschlagen: {e}"),
    "Le fichier téléchargé est abîmé (somme SHA-256 incorrecte). Réessayez.":
        ("The downloaded file is damaged (wrong SHA-256 checksum). Try again.",
         "El archivo descargado está dañado (suma SHA-256 incorrecta). Inténtelo de nuevo.",
         "Die heruntergeladene Datei ist beschädigt (falsche SHA-256-Prüfsumme). Erneut versuchen."),
    "Réessayer": ("Try again", "Reintentar", "Erneut versuchen"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Fermer": ("Close", "Cerrar", "Schließen"),
    "Le terminal Winlux est introuvable.": ("The Winlux terminal is missing.",
                                           "No se encuentra el terminal de Winlux.",
                                           "Das Winlux-Terminal fehlt."),
    "Appuyez sur Entrée pour fermer cette fenêtre.": ("Press Enter to close this window.",
                                                      "Pulse Intro para cerrar esta ventana.",
                                                      "Drücken Sie die Eingabetaste, um dieses Fenster zu schließen."),
}


def _t(text, **kw):
    row = _TR.get(text)
    out = row[_COL[_LANG]] if row and _LANG in _COL else text
    return out.format(**kw) if kw else out


class Cancelled(Exception):
    pass


def installed_launcher():
    """Commande qui lance Claude Code, s'il est installé."""
    launcher = BIN / "claude"
    if launcher.is_file() and os.access(launcher, os.X_OK) and \
            ((DATA / "claude.exe").is_file() or (DATA / "claude").is_file()):
        return launcher
    return None


def write_launcher():
    BIN.mkdir(parents=True, exist_ok=True)
    launcher = BIN / "claude"
    launcher.write_text("#!/bin/sh\n# Claude Code (version Windows) via la couche PE de Winlux.\n"
                        f'exec winlux "{DATA / "claude.exe"}" "$@"\n', encoding="utf-8")
    launcher.chmod(0o755)
    return launcher


def install(progress, cancelled):
    """Télécharge, vérifie et installe claude.exe. progress(fraction, texte)."""
    progress(None, _t("Recherche de la dernière version…"))
    with urllib.request.urlopen(f"{BASE}/{CANAL}", timeout=30) as r:
        version = r.read().decode().strip()
    if not version or not version[0].isdigit() or version.count(".") != 2:
        raise RuntimeError(f"version ? {version[:40]!r}")
    with urllib.request.urlopen(f"{BASE}/{version}/manifest.json", timeout=30) as r:
        manifest = json.load(r)
    expected = manifest["platforms"]["win32-x64"]["checksum"].lower()
    size = int(manifest["platforms"]["win32-x64"].get("size") or 0)
    DATA.mkdir(parents=True, exist_ok=True)
    for stale in DATA.glob(".claude-*.part"):  # téléchargement interrompu
        stale.unlink(missing_ok=True)
    digest = hashlib.sha256()
    done = 0
    text = _t("Téléchargement de Claude Code {v}…", v=version)
    progress(0.0, text)
    with tempfile.NamedTemporaryFile(dir=DATA, prefix=".claude-", suffix=".part", delete=False) as out:
        part = Path(out.name)
        try:
            with urllib.request.urlopen(f"{BASE}/{version}/win32-x64/claude.exe", timeout=60) as r:
                total = int(r.headers.get("Content-Length") or size or 0)
                while True:
                    if cancelled.is_set():
                        raise Cancelled()
                    chunk = r.read(1 << 20)
                    if not chunk:
                        break
                    out.write(chunk)
                    digest.update(chunk)
                    done += len(chunk)
                    if total:
                        mo = f" {done >> 20} / {total >> 20} Mo"
                        progress(min(done / total, 1.0), text + mo)
        except BaseException:
            out.close()
            part.unlink(missing_ok=True)
            raise
    progress(1.0, _t("Vérification du fichier…"))
    if digest.hexdigest() != expected:
        part.unlink(missing_ok=True)
        raise RuntimeError(_t("Le fichier téléchargé est abîmé (somme SHA-256 incorrecte). Réessayez."))
    progress(1.0, _t("Installation…"))
    part.chmod(0o755)
    part.replace(DATA / "claude.exe")
    (DATA / "version").write_text(f"{version} windows\n", encoding="utf-8")
    return write_launcher()


def open_in_terminal(launcher):
    """Ouvre Claude Code dans le terminal Winlux, dans le dossier personnel.
    La fenêtre reste ouverte si Claude Code s'arrête sur une erreur."""
    terminal = shutil.which("winlux-terminal") or shutil.which("x-terminal-emulator")
    if not terminal:
        return False
    script = (f'"{launcher}"; status=$?; '
              f'if [ "$status" -ne 0 ]; then echo; printf "%s" "{_t("Appuyez sur Entrée pour fermer cette fenêtre.")}"; '
              'read _; fi')
    args = [terminal]
    if terminal.endswith("winlux-terminal"):
        args += ["--cwd", str(Path.home())]
    args += ["-e", "/bin/sh", "-c", script]
    subprocess.Popen(args, start_new_session=True)
    return True


def run_gui():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk

    win = Gtk.Window(title=_t("Installer Claude Code"))
    win.set_default_size(520, -1)
    win.set_resizable(False)
    win.set_position(Gtk.WindowPosition.CENTER)
    win.set_icon_name("winlux-claude-code")
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14, margin=24)
    head = Gtk.Box(spacing=14)
    head.pack_start(Gtk.Image.new_from_icon_name("winlux-claude-code", Gtk.IconSize.DIALOG), False, False, 0)
    title = Gtk.Label(xalign=0)
    title.set_markup(f"<span size='x-large' weight='bold'>{GLib.markup_escape_text(_t('Installer Claude Code'))}</span>")
    head.pack_start(title, True, True, 0)
    box.pack_start(head, False, False, 0)
    intro = Gtk.Label(label=_t("Claude Code, l'assistant de programmation d'Anthropic, va être téléchargé "
                               "(environ 300 Mo) puis ouvert dans le terminal Winlux."),
                      xalign=0, wrap=True, max_width_chars=60)
    box.pack_start(intro, False, False, 0)
    bar = Gtk.ProgressBar(show_text=False)
    box.pack_start(bar, False, False, 0)
    status = Gtk.Label(xalign=0, wrap=True, max_width_chars=60)
    box.pack_start(status, False, False, 0)
    buttons = Gtk.Box(spacing=8, halign=Gtk.Align.END)
    retry = Gtk.Button(label=_t("Réessayer"))
    cancel = Gtk.Button(label=_t("Annuler"))
    buttons.pack_start(retry, False, False, 0)
    buttons.pack_start(cancel, False, False, 0)
    box.pack_start(buttons, False, False, 0)
    win.add(box)

    state = {"cancel": threading.Event(), "pulse": 0}

    def show(fraction, text):
        if fraction is None:
            bar.pulse()
        else:
            bar.set_fraction(fraction)
        status.set_text(text)
        return False

    def progress(fraction, text):
        GLib.idle_add(show, fraction, text)

    def finished(launcher, error):
        if launcher:
            status.set_text(_t("Claude Code est installé. Ouverture…"))
            bar.set_fraction(1.0)
            if not open_in_terminal(launcher):
                status.set_text(_t("Le terminal Winlux est introuvable."))
                cancel.set_label(_t("Fermer"))
                return False
            GLib.timeout_add(800, Gtk.main_quit)
        elif error is not None:
            status.set_text(error)
            retry.show()
            cancel.set_label(_t("Fermer"))
        return False

    def work():
        try:
            launcher = install(progress, state["cancel"])
            GLib.idle_add(finished, launcher, None)
        except Cancelled:
            GLib.idle_add(Gtk.main_quit)
        except Exception as e:  # réseau, disque, manifeste
            message = str(e) if "SHA-256" in str(e) else _t("Le téléchargement a échoué : {e}", e=e)
            GLib.idle_add(finished, None, message)

    def start(*_):
        retry.hide()
        cancel.set_label(_t("Annuler"))
        state["cancel"] = threading.Event()
        threading.Thread(target=work, daemon=True).start()

    def on_cancel(*_):
        state["cancel"].set()
        Gtk.main_quit()

    retry.connect("clicked", start)
    cancel.connect("clicked", on_cancel)
    win.connect("destroy", on_cancel)
    win.show_all()
    retry.hide()
    start()
    Gtk.main()


def main():
    launcher = installed_launcher()
    if launcher and "--reinstaller" not in sys.argv:
        if open_in_terminal(launcher):
            return 0
    run_gui()
    return 0


if __name__ == "__main__":
    sys.exit(main())
