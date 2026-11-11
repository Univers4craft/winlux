#!/usr/bin/python3
"""Assistant d'installation de Winlux (#833) : ce qui s'ouvre au double-clic
sur un installateur Windows (.msi, ou .exe NSIS / Inno Setup reconnu).

Comme l'assistant d'un programme d'installation sous Windows : une page
d'accueil (nom, version, éditeur, dossier d'installation modifiable pour
NSIS et Inno, raccourci sur le Bureau), la progression, puis la fin
(réussite ou erreur avec le journal). Le travail lui-même est fait par le
runtime (actions personnalisées et programmes lancés par l'installateur compris) :
  winlux --setup FICHIER.exe [/D=dossier] [/nodesktop]
  winlux --msiexec /i FICHIER.msi /qn

Usage : winlux-setup-wizard FICHIER [--runtime CHEMIN]
Code de sortie : celui de l'installation, 1602 si l'utilisateur annule.
WINLUX_SETUP_WIZARD_AUTO=1 enchaîne les pages sans clic (tests).
"""
import os
import subprocess
import sys
import threading
from pathlib import Path

ROOT = os.environ.get("WINLUX_ROOT", "")
CANCELLED = 1602


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
    "Installation de {name}": ("{name} Setup", "Instalación de {name}", "{name} – Setup"),
    "Bienvenue dans l'assistant d'installation de {name}":
        ("Welcome to the {name} Setup Wizard", "Bienvenido al asistente de instalación de {name}",
         "Willkommen beim Setup-Assistenten von {name}"),
    "Version {version}": ("Version {version}", "Versión {version}", "Version {version}"),
    "Éditeur : {publisher}": ("Publisher: {publisher}", "Editor: {publisher}", "Herausgeber: {publisher}"),
    "Winlux installe ce programme lui-même.":
        ("Winlux installs this program itself.", "Winlux instala este programa por sí mismo.",
         "Winlux installiert dieses Programm selbst."),
    "Dossier d'installation": ("Install location", "Carpeta de instalación", "Installationsordner"),
    "Parcourir…": ("Browse…", "Examinar…", "Durchsuchen…"),
    "Créer un raccourci sur le Bureau": ("Create a desktop shortcut", "Crear un acceso directo en el escritorio",
                                         "Verknüpfung auf dem Desktop erstellen"),
    "Installer": ("Install", "Instalar", "Installieren"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Terminer": ("Finish", "Finalizar", "Fertigstellen"),
    "Installation en cours…": ("Installing…", "Instalando…", "Installation läuft…"),
    "Veuillez patienter pendant l'installation de {name}.":
        ("Please wait while {name} is being installed.", "Espere mientras se instala {name}.",
         "Bitte warten Sie, während {name} installiert wird."),
    "{name} est installé.": ("{name} has been installed.", "{name} se ha instalado.", "{name} wurde installiert."),
    "Vous le trouverez dans le menu Démarrer.": ("You can find it in the Start menu.",
                                                 "Lo encontrará en el menú Inicio.", "Sie finden es im Startmenü."),
    "L'installation a échoué (code {code}).": ("Setup failed (code {code}).", "La instalación ha fallado (código {code}).",
                                                "Die Installation ist fehlgeschlagen (Code {code})."),
    "Choisir le dossier d'installation": ("Choose the install location", "Elegir la carpeta de instalación",
                                          "Installationsordner auswählen"),
    "Le dossier est choisi par le paquet.": ("The package chooses the location.", "El paquete elige la carpeta.",
                                             "Das Paket legt den Ordner fest."),
    "Programme d'installation inconnu": ("Unknown installer", "Instalador desconocido", "Unbekanntes Installationsprogramm"),
}
_COLUMN = {"en": 0, "es": 1, "de": 2}


def tr(text, **values):
    if LANG in _COLUMN and text in _TR:
        text = _TR[text][_COLUMN[LANG]]
    return text.format(**values) if values else text


def read_info(runtime, path):
    """`winlux --setup --info` : type, nom, version, éditeur, dossier."""
    try:
        out = subprocess.run([runtime, "--setup", "--info", path], capture_output=True, text=True,
                             timeout=120).stdout
    except (OSError, subprocess.TimeoutExpired):
        return None
    # La dernière colonne (dossier) est vide pour un MSI : pas de strip().
    lines = [l for l in out.split("\n") if l.strip()]
    parts = lines[-1].rstrip("\r").split("\t") if lines else []
    if len(parts) < 5:
        return None
    kind, name, version, publisher, folder = parts[:5]
    return {"kind": kind, "name": name or Path(path).stem, "version": version,
            "publisher": publisher, "folder": folder}


def win_to_linux(folder):
    """C:\\Program Files\\App -> chemin Linux (même disposition que le runtime)."""
    if len(folder) >= 2 and folder[1] == ":":
        drive = os.environ.get("WINLUX_DRIVE_C") or os.environ.get("HOME", "")
        return str(Path(drive, *folder[2:].replace("\\", "/").strip("/").split("/")))
    return folder


def linux_to_win(path):
    drive = os.environ.get("WINLUX_DRIVE_C") or os.environ.get("HOME", "")
    try:
        rel = Path(path).resolve().relative_to(Path(drive).resolve())
        return "C:\\" + str(rel).replace("/", "\\") if str(rel) != "." else "C:\\"
    except ValueError:
        return "Z:" + path.replace("/", "\\")


def command(runtime, path, info, folder=None, desktop=True):
    """Ligne de commande de l'installation silencieuse faite par le runtime."""
    if info["kind"] == "msi":
        return [runtime, "--msiexec", "/i", path, "/qn"]
    if info["kind"] == "burn":
        # Bundle WiX Burn : lancé par le runtime, qui installe sa chaîne.
        return [runtime, path, "/quiet", "/norestart"]
    argv = [runtime, "--setup", path]
    if folder and folder != info["folder"]:
        argv.append("/D=" + folder)
    if not desktop:
        argv.append("/nodesktop")
    return argv


def menu_entries():
    """Lanceurs du menu Démarrer du compte (nom -> date de modification)."""
    base = os.environ.get("XDG_DATA_HOME") or os.path.join(os.path.expanduser("~"), ".local", "share")
    entries = {}
    for root, _dirs, files in os.walk(os.path.join(base, "applications")):
        for name in files:
            if name.endswith(".desktop"):
                full = os.path.join(root, name)
                try:
                    entries[full] = os.stat(full).st_mtime_ns
                except OSError:
                    pass
    return entries


def main(argv):
    args = [a for a in argv[1:]]
    runtime = os.environ.get("WINLUX_BIN", "/usr/bin/winlux")
    if "--runtime" in args:
        i = args.index("--runtime")
        runtime = args[i + 1]
        del args[i:i + 2]
    if not args:
        print(__doc__, file=sys.stderr)
        return 2
    path = os.path.abspath(args[0])
    info = read_info(runtime, path)
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk, Pango

    auto = os.environ.get("WINLUX_SETUP_WIZARD_AUTO") == "1"
    state = {"code": CANCELLED}
    if info is None:
        dialog = Gtk.MessageDialog(message_type=Gtk.MessageType.ERROR, buttons=Gtk.ButtonsType.CLOSE,
                                   text=tr("Programme d'installation inconnu"))
        dialog.format_secondary_text(os.path.basename(path))
        if not auto:
            dialog.run()
        return 1

    win = Gtk.Window(title=tr("Installation de {name}", name=info["name"]))
    win.set_default_size(560, 380)
    win.set_resizable(False)
    win.set_position(Gtk.WindowPosition.CENTER)
    win.set_icon_name("system-software-install")
    stack = Gtk.Stack(transition_type=Gtk.StackTransitionType.SLIDE_LEFT)
    outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    outer.pack_start(stack, True, True, 0)
    buttons = Gtk.Box(spacing=8, margin=16, halign=Gtk.Align.END)
    outer.pack_end(buttons, False, False, 0)
    outer.pack_end(Gtk.Separator(), False, False, 0)
    win.add(outer)

    def page(title):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=24)
        head = Gtk.Box(spacing=14)
        icon = Gtk.Image.new_from_icon_name("system-software-install", Gtk.IconSize.DIALOG)
        head.pack_start(icon, False, False, 0)
        label = Gtk.Label(xalign=0, wrap=True)
        label.set_markup("<span size='x-large' weight='bold'>%s</span>" % GLib.markup_escape_text(title))
        head.pack_start(label, True, True, 0)
        box.pack_start(head, False, False, 0)
        return box

    def text(box, value, dim=False):
        label = Gtk.Label(label=value, xalign=0, wrap=True)
        label.set_line_wrap_mode(Pango.WrapMode.WORD_CHAR)
        if dim:
            label.get_style_context().add_class("dim-label")
        box.pack_start(label, False, False, 0)
        return label

    # Accueil.
    welcome = page(tr("Bienvenue dans l'assistant d'installation de {name}", name=info["name"]))
    if info["version"]:
        text(welcome, tr("Version {version}", version=info["version"]))
    if info["publisher"]:
        text(welcome, tr("Éditeur : {publisher}", publisher=info["publisher"]))
    # Rien sur le code de l'installateur : les actions
    # personnalisées des .msi et les programmes lancés par NSIS tournent.
    text(welcome, tr("Winlux installe ce programme lui-même."), dim=True)
    folder_entry = Gtk.Entry(hexpand=True)
    desktop = Gtk.CheckButton(label=tr("Créer un raccourci sur le Bureau"), active=True)
    frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin_top=8)
    frame.pack_start(Gtk.Label(label=tr("Dossier d'installation"), xalign=0), False, False, 0)
    if info["kind"] in ("msi", "burn") or not info["folder"]:
        frame.pack_start(Gtk.Label(label=tr("Le dossier est choisi par le paquet."), xalign=0), False, False, 0)
    else:
        row = Gtk.Box(spacing=6)
        folder_entry.set_text(info["folder"])
        row.pack_start(folder_entry, True, True, 0)
        browse = Gtk.Button(label=tr("Parcourir…"))

        def choose(_button):
            chooser = Gtk.FileChooserDialog(title=tr("Choisir le dossier d'installation"), transient_for=win,
                                            action=Gtk.FileChooserAction.SELECT_FOLDER)
            chooser.add_buttons(tr("Annuler"), Gtk.ResponseType.CANCEL, "OK", Gtk.ResponseType.OK)
            parent = Path(win_to_linux(folder_entry.get_text())).parent
            if parent.is_dir():
                chooser.set_current_folder(str(parent))
            if chooser.run() == Gtk.ResponseType.OK and chooser.get_filename():
                name = Path(win_to_linux(info["folder"])).name
                folder_entry.set_text(linux_to_win(os.path.join(chooser.get_filename(), name)))
            chooser.destroy()
        browse.connect("clicked", choose)
        row.pack_start(browse, False, False, 0)
        frame.pack_start(row, False, False, 0)
        frame.pack_start(desktop, False, False, 0)
    welcome.pack_start(frame, False, False, 0)
    stack.add_named(welcome, "welcome")

    # Progression.
    progress_page = page(tr("Installation en cours…"))
    text(progress_page, tr("Veuillez patienter pendant l'installation de {name}.", name=info["name"]))
    bar = Gtk.ProgressBar(margin_top=12)
    progress_page.pack_start(bar, False, False, 0)
    status = text(progress_page, "", dim=True)
    status.set_ellipsize(Pango.EllipsizeMode.END)
    stack.add_named(progress_page, "progress")

    # Fin.
    done_page = page("")
    done_title = done_page.get_children()[0].get_children()[1]
    done_text = text(done_page, "")
    log_view = Gtk.TextView(editable=False, monospace=True, wrap_mode=Gtk.WrapMode.WORD_CHAR)
    scroll = Gtk.ScrolledWindow(vexpand=True, no_show_all=True)
    scroll.add(log_view)
    done_page.pack_start(scroll, True, True, 0)
    stack.add_named(done_page, "done")

    cancel = Gtk.Button(label=tr("Annuler"))
    install = Gtk.Button(label=tr("Installer"))
    install.get_style_context().add_class("suggested-action")
    finish = Gtk.Button(label=tr("Terminer"), no_show_all=True)
    finish.get_style_context().add_class("suggested-action")
    for b in (cancel, install, finish):
        b.set_size_request(110, -1)
        buttons.pack_start(b, False, False, 0)

    def finished(code, output):
        state["code"] = code
        bar.set_fraction(1.0)
        if code in (0, 3010):
            done_title.set_markup("<span size='x-large' weight='bold'>%s</span>"
                                  % GLib.markup_escape_text(tr("{name} est installé.", name=info["name"])))
            # Seulement si l'installation a posé ou refait un lanceur.
            if menu_entries() != state["menu"]:
                done_text.set_text(tr("Vous le trouverez dans le menu Démarrer."))
        else:
            done_title.set_markup("<span size='x-large' weight='bold'>%s</span>"
                                  % GLib.markup_escape_text(tr("L'installation a échoué (code {code}).", code=code)))
            done_text.set_text(os.path.basename(path))
            log_view.get_buffer().set_text("\n".join(output.strip().splitlines()[-15:]))
            scroll.show_all()
        stack.set_visible_child_name("done")
        finish.show()
        if auto:
            GLib.timeout_add(300, lambda: finish.clicked() or False)
        return False

    def work(argv):
        lines = []
        try:
            proc = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                    cwd=os.path.dirname(path) or None)
            for line in proc.stdout:
                lines.append(line)
                GLib.idle_add(status.set_text, line.strip())
            code = proc.wait()
        except OSError as e:
            lines.append(str(e))
            code = 1
        print("".join(lines), end="", flush=True)
        GLib.idle_add(finished, code, "".join(lines))

    def pulse():
        if stack.get_visible_child_name() != "progress":
            return False
        bar.pulse()
        return True

    def start(_button):
        argv = command(runtime, path, info, folder_entry.get_text().strip() or None, desktop.get_active())
        if os.environ.get("WINLUX_SETUP_WIZARD_TRACE"):
            Path(os.environ["WINLUX_SETUP_WIZARD_TRACE"]).write_text("\n".join(argv) + "\n")
        state["menu"] = menu_entries()
        install.hide()
        cancel.set_sensitive(False)
        stack.set_visible_child_name("progress")
        GLib.timeout_add(120, pulse)
        threading.Thread(target=work, args=(argv,), daemon=True).start()

    install.connect("clicked", start)
    cancel.connect("clicked", lambda _b: win.destroy())
    finish.connect("clicked", lambda _b: win.destroy())
    win.connect("destroy", Gtk.main_quit)
    win.show_all()
    if auto:
        GLib.timeout_add(300, lambda: install.clicked() or False)
    Gtk.main()
    return state["code"]


if __name__ == "__main__":
    sys.exit(main(sys.argv))
