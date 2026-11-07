#!/usr/bin/python3
"""Boîte « Exécuter » (Windows + R) et menu d'accès rapide (Windows + X).

    winlux-run              boîte Exécuter
    winlux-run menu         menu d'accès rapide (menu « pipe » d'openbox)
    winlux-run place-menu [left|center] [TAILLE]
                            place ce menu au-dessus du bouton Démarrer
    winlux-run resolve TEXTE   commande lancée pour TEXTE (tests)

La boîte comprend les noms courants de Windows (cmd, notepad, control,
taskmgr, regedit, msinfo32, appwiz.cpl…), les adresses Internet, les
dossiers et fichiers (chemins Linux, ~, C:\\Users\\<compte>\\…, Z:\\…) et
les commandes du système. Les 20 dernières saisies sont proposées.
"""
import html
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys


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
    'Exécuter': ('Run', 'Ejecutar', 'Ausführen'),
    'Entrez le nom d’un programme, dossier, document ou ressource Internet, et Winlux l’ouvrira pour vous.':
        ('Type the name of a program, folder, document or Internet resource, and Winlux will open it for you.',
         'Escriba el nombre de un programa, carpeta, documento o recurso de Internet y Winlux lo abrirá.',
         'Geben Sie den Namen eines Programms, Ordners, Dokuments oder einer Internetressource an. Winlux öffnet es für Sie.'),
    'Ouvrir :': ('Open:', 'Abrir:', 'Öffnen:'),
    'OK': ('OK', 'Aceptar', 'OK'),
    'Annuler': ('Cancel', 'Cancelar', 'Abbrechen'),
    'Parcourir…': ('Browse…', 'Examinar…', 'Durchsuchen…'),
    'Winlux ne trouve pas « {name} ». Vérifiez que le nom est correctement écrit et réessayez.':
        ('Winlux cannot find “{name}”. Make sure you typed the name correctly, and then try again.',
         'Winlux no encuentra «{name}». Asegúrese de que el nombre está bien escrito e inténtelo de nuevo.',
         '„{name}“ wurde nicht gefunden. Überprüfen Sie die Schreibweise und versuchen Sie es erneut.'),
    'Applications installées': ('Installed apps', 'Aplicaciones instaladas', 'Installierte Apps'),
    'Options d’alimentation': ('Power options', 'Opciones de energía', 'Energieoptionen'),
    'Observateur d’événements': ('Event viewer', 'Visor de eventos', 'Ereignisanzeige'),
    'Système': ('System', 'Sistema', 'System'),
    'Gestionnaire de périphériques': ('Device manager', 'Administrador de dispositivos', 'Geräte-Manager'),
    'Connexions réseau': ('Network connections', 'Conexiones de red', 'Netzwerkverbindungen'),
    'Terminal': ('Terminal', 'Terminal', 'Terminal'),
    'Gestionnaire des tâches': ('Task manager', 'Administrador de tareas', 'Task-Manager'),
    'Paramètres': ('Settings', 'Configuración', 'Einstellungen'),
    'Explorateur de fichiers': ('File explorer', 'Explorador de archivos', 'Explorer'),
    'Rechercher': ('Search', 'Buscar', 'Suchen'),
    'Arrêter ou se déconnecter': ('Shut down or sign out', 'Apagar o cerrar sesión', 'Herunterfahren oder abmelden'),
    'Verrouiller': ('Lock', 'Bloquear', 'Sperren'),
    'Redémarrer': ('Restart', 'Reiniciar', 'Neu starten'),
    'Arrêter': ('Shut down', 'Apagar', 'Herunterfahren'),
    'Bureau': ('Desktop', 'Escritorio', 'Desktop'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# Noms de Windows → commandes Winlux (sans .exe, en minuscules).
SETTINGS = ["winlux-shell-app", "settings"]
ALIASES = {
    "cmd": ["winlux-terminal"], "powershell": ["winlux-terminal"], "pwsh": ["winlux-terminal"], "wt": ["winlux-terminal"],
    "control": SETTINGS, "ms-settings:": SETTINGS, "explorer": ["winlux-shell-app", "explorer"],
    "taskmgr": ["winlux-shell-app", "taskmgr"], "regedit": ["winlux", "--regedit"], "regedt32": ["winlux", "--regedit"],
    "eventvwr": ["winlux-shell-app", "eventviewer"], "eventvwr.msc": ["winlux-shell-app", "eventviewer"],
    "msinfo32": ["winlux-shell-app", "msinfo32"], "taskschd.msc": ["winlux-shell-app", "taskschd"],
    "wf.msc": ["winlux-shell-app", "firewall"], "firewall.cpl": ["winlux-shell-app", "firewall"],
    "notepad": ["winlux-shell-app", "editor"], "write": ["winlux-shell-app", "editor"],
    "appwiz.cpl": SETTINGS + ["1"], "ncpa.cpl": SETTINGS + ["2"], "desk.cpl": SETTINGS + ["8"],
    "mmsys.cpl": SETTINGS + ["16"], "powercfg.cpl": SETTINGS + ["23"], "sysdm.cpl": SETTINGS + ["9"],
    "timedate.cpl": SETTINGS + ["14"], "main.cpl": SETTINGS + ["4"], "devmgmt.msc": SETTINGS + ["34"],
    "winver": SETTINGS + ["9"], "certmgr.msc": SETTINGS + ["36"], "intl.cpl": SETTINGS + ["14"],
}


def windows_path(text, home):
    """C:\\Users\\<compte>\\… → dossier personnel ; Z:\\… → racine ; sinon None."""
    match = re.fullmatch(r"([A-Za-z]):[\\/]*(.*)", text)
    if not match: return None
    drive, rest = match.group(1).lower(), [part for part in re.split(r"[\\/]+", match.group(2)) if part]
    if drive == "z": return Path("/", *rest)
    if drive == "c":
        if len(rest) >= 2 and rest[0].lower() == "users": return Path(home, *rest[2:])
        return Path(home, *rest)
    return None


def resolve(text, home=None, which=shutil.which):
    """Commande (liste d'arguments) pour la saisie, ou None si rien ne correspond."""
    text = text.strip()
    if not text: return None
    home = str(home or Path.home())
    lowered = text.casefold()
    if re.match(r"^(https?|ftp)://", lowered) or re.match(r"^www\.[^\s]+\.[a-z]{2,}", lowered):
        return ["xdg-open", text if "://" in text else "https://" + text]
    if lowered.startswith("ms-settings:"): return SETTINGS
    # Chemins Windows : les \ ne sont pas des échappements.
    try: words = shlex.split(text, posix="\\" not in text)
    except ValueError: words = text.split()
    words = [word[1:-1] if len(word) >= 2 and word[0] == word[-1] and word[0] in "\"'" else word for word in words]
    name = words[0].casefold() if words else ""
    base = name[:-4] if name.endswith(".exe") else name
    if base in ALIASES:
        command = list(ALIASES[base])
        rest = words[1:]
        if base == "explorer" and rest:
            target = windows_path(rest[0], home) or Path(os.path.expanduser(rest[0]))
            return command + [str(target)]
        return command + rest if base in ("notepad", "write") else command
    if text in ("~", ".") or text.startswith(("/", "~/", "~")) or windows_path(text, home):
        path = windows_path(text, home) or Path(os.path.expanduser(text))
        if path.is_dir(): return ["winlux-shell-app", "explorer", str(path)]
        if path.is_file():
            if path.suffix.lower() in (".exe", ".msi", ".bat", ".cmd"): return ["winlux-launch-exe", "desktop", str(path)]
            return ["xdg-open", str(path)]
        return None
    if words and which(words[0]): return words
    if words and which(base): return [base] + words[1:]
    return None


HISTORY = Path.home() / ".config/winlux/run-history"


def history():
    try: return [line for line in HISTORY.read_text(encoding="utf-8").splitlines() if line.strip()][:20]
    except OSError: return []


def remember(text):
    entries = [text] + [line for line in history() if line != text]
    try:
        HISTORY.parent.mkdir(parents=True, exist_ok=True)
        HISTORY.write_text("\n".join(entries[:20]) + "\n", encoding="utf-8")
    except OSError:
        pass


def launch(command):
    subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=True, cwd=str(Path.home()))


def run_dialog():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk
    dialog = Gtk.Dialog(title=_t("Exécuter"))
    dialog.set_role("winlux-run")
    dialog.set_resizable(False)
    dialog.set_keep_above(True)
    dialog.set_position(Gtk.WindowPosition.NONE)
    area = dialog.get_content_area()
    area.set_spacing(12)
    area.set_border_width(16)
    row = Gtk.Box(spacing=12)
    icon = Gtk.Image.new_from_icon_name("system-run", Gtk.IconSize.DIALOG)
    row.pack_start(icon, False, False, 0)
    intro = Gtk.Label(label=_t("Entrez le nom d’un programme, dossier, document ou ressource Internet, et Winlux l’ouvrira pour vous."),
                      xalign=0, wrap=True, max_width_chars=48)
    row.pack_start(intro, True, True, 0)
    area.pack_start(row, False, False, 0)
    line = Gtk.Box(spacing=8)
    line.pack_start(Gtk.Label(label=_t("Ouvrir :")), False, False, 0)
    combo = Gtk.ComboBoxText.new_with_entry()
    for entry in history(): combo.append_text(entry)
    entry = combo.get_child()
    entry.set_activates_default(True)
    entry.set_width_chars(40)
    if history():
        entry.set_text(history()[0])
        entry.select_region(0, -1)
    line.pack_start(combo, True, True, 0)
    area.pack_start(line, False, False, 0)
    error = Gtk.Label(xalign=0, wrap=True, max_width_chars=56)
    error.get_style_context().add_class("error")
    area.pack_start(error, False, False, 0)
    browse = dialog.add_button(_t("Parcourir…"), 2)
    dialog.add_button(_t("Annuler"), Gtk.ResponseType.CANCEL)
    ok = dialog.add_button(_t("OK"), Gtk.ResponseType.OK)
    ok.get_style_context().add_class("suggested-action")
    dialog.set_default_response(Gtk.ResponseType.OK)
    entry.connect("changed", lambda *_: ok.set_sensitive(bool(entry.get_text().strip())))
    ok.set_sensitive(bool(entry.get_text().strip()))
    browse.set_can_default(False)
    dialog.show_all()
    error.hide()
    # En bas à gauche, au-dessus de la barre des tâches, comme Windows.
    screen = dialog.get_screen()
    monitor = screen.get_display().get_monitor_at_window(dialog.get_window()) if dialog.get_window() else None
    if monitor is not None:
        work = monitor.get_workarea()
        _, height = dialog.get_size()
        dialog.move(work.x + 12, work.y + work.height - height - 12)
    while True:
        response = dialog.run()
        if response == 2:
            chooser = Gtk.FileChooserNative.new(_t("Parcourir…"), dialog, Gtk.FileChooserAction.OPEN, None, None)
            chooser.set_current_folder(str(Path.home()))
            if chooser.run() == Gtk.ResponseType.ACCEPT: entry.set_text(chooser.get_filename() or "")
            chooser.destroy()
            continue
        if response != Gtk.ResponseType.OK: return 0
        text = entry.get_text().strip()
        command = resolve(text)
        if command is None:
            error.set_text(_t("Winlux ne trouve pas « {name} ». Vérifiez que le nom est correctement écrit et réessayez.").format(name=text))
            error.show()
            entry.grab_focus()
            continue
        remember(text)
        launch(command)
        return 0


def quick_menu():
    """Windows + X : menu d'accès rapide d'openbox, dans la langue du système."""
    def item(label, command):
        return (f'<item label="{html.escape(_t(label))}"><action name="Execute"><command>'
                f'{html.escape(command)}</command></action></item>')
    lines = ['<openbox_pipe_menu>',
             item("Applications installées", "winlux-shell-app settings 1"),
             item("Options d’alimentation", "winlux-shell-app settings 23"),
             item("Observateur d’événements", "winlux-shell-app eventviewer"),
             item("Système", "winlux-shell-app settings 9"),
             item("Gestionnaire de périphériques", "winlux-shell-app settings 34"),
             item("Connexions réseau", "winlux-shell-app settings 2"),
             '<separator/>',
             item("Terminal", "winlux-terminal"),
             item("Gestionnaire des tâches", "winlux-shell-app taskmgr"),
             item("Paramètres", "winlux-shell-app settings"),
             item("Explorateur de fichiers", "winlux-shell-app explorer"),
             item("Rechercher", "winlux-start-menu"),
             item("Exécuter", "winlux-run"),
             '<separator/>',
             f'<menu id="winlux-quick-link-power" label="{html.escape(_t("Arrêter ou se déconnecter"))}">',
             item("Verrouiller", "winlux-lock now"),
             item("Redémarrer", "winlux-power ask reboot"),
             item("Arrêter", "winlux-power ask shutdown"),
             '</menu>',
             f'<item label="{html.escape(_t("Bureau"))}"><action name="ToggleShowDesktop"/></item>',
             '</openbox_pipe_menu>']
    print("\n".join(lines))
    return 0


def menu_position(alignment, size):
    """Position openbox du menu Windows + X, au-dessus du bouton Démarrer.

    Sans <position>, openbox ouvre le menu sous le pointeur. Un y négatif
    compte depuis le bas de l'écran jusqu'au bas du menu : -TAILLE le pose
    juste au-dessus de la barre. Barre centrée : le bouton Démarrer est au
    début du groupe centré, le menu est centré au-dessus de la barre."""
    x = "center" if alignment == "center" else "0"
    return f"<position><x>{x}</x><y>-{size}</y></position>"


def _panel_size():
    try:
        out = subprocess.run(["xfconf-query", "-c", "xfce4-panel", "-p", "/panels/panel-1/size"],
                             capture_output=True, text=True, timeout=5).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        out = ""
    return out if out.isdigit() and 16 <= int(out) <= 128 else "40"


def _alignment():
    try:
        for line in Path("/etc/winlux/preferences.conf").read_text(encoding="utf-8").splitlines():
            if line.startswith("TASKBAR_ALIGN="): return line.split("=", 1)[1].strip()
    except OSError:
        pass
    return "left"


def place_menu(alignment=None, size=None, config=None):
    """Réécrit la position du menu Windows + X dans le rc.xml du compte
    (copie figée par compte) et recharge openbox si elle a changé."""
    alignment = alignment or _alignment()
    size = size or _panel_size()
    if alignment not in ("left", "center") or not str(size).isdigit(): return 2
    config = Path(config or os.path.expanduser("~/.config/openbox/rc.xml"))
    try:
        text = config.read_text(encoding="utf-8")
    except OSError:
        return 1
    pattern = re.compile(r'(<keybind key="W-x">\s*<action name="ShowMenu">\s*<menu>winlux-quick-link</menu>)'
                         r'\s*(?:<position>.*?</position>)?(\s*</action>)', re.S)
    updated = pattern.sub(lambda m: m.group(1) + menu_position(alignment, size) + m.group(2), text, count=1)
    if updated == text: return 0
    temp = config.with_name(config.name + ".tmp")
    temp.write_text(updated, encoding="utf-8")
    temp.replace(config)
    if os.environ.get("DISPLAY") and shutil.which("openbox"):
        try:
            subprocess.run(["openbox", "--reconfigure"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
        except (OSError, subprocess.SubprocessError):
            pass
    return 0


def main(argv):
    if len(argv) == 1: return run_dialog()
    if len(argv) == 2 and argv[1] == "menu": return quick_menu()
    if 2 <= len(argv) <= 4 and argv[1] == "place-menu": return place_menu(*argv[2:])
    if len(argv) == 3 and argv[1] == "resolve":
        command = resolve(argv[2])
        if command is None: return 1
        print(" ".join(shlex.quote(part) for part in command))
        return 0
    print(__doc__.strip().split("\n\n")[1], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
