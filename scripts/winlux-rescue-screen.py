#!/usr/bin/python3
"""Écran de secours : le bureau n'a pas pu s'ouvrir.

Lancé par l'init (root, DISPLAY=:0) quand les services de la session
échouent, au lieu de laisser un écran noir. Il dit ce qui s'est passé, quoi
faire, montre la fin du journal et propose Redémarrer, Éteindre et, s'il y
en a un, un terminal. Textes en français, anglais, allemand et espagnol.
"""
import os
import shutil
import signal
import subprocess
import sys

LOGS = ("/run/winlux/accounts-repair.log", "/run/winlux/native-early.log",
        "/run/winlux/native-session.log", "/run/winlux/system-services.log")

TEXT = {
    "fr": {
        "title": "Winlux n'a pas pu ouvrir le bureau",
        "body": ("Les services de la session n'ont pas démarré. Cela arrive surtout après une "
                 "coupure de courant ou un arrêt forcé pendant que le disque était écrit.\n\n"
                 "Que faire :\n"
                 "1. Cliquez sur « Redémarrer » : Winlux répare les fichiers de comptes à chaque démarrage.\n"
                 "2. Si cet écran revient, au menu de démarrage choisissez « Winlux secours » "
                 "(touche S), puis « Vérifier le système de fichiers » ou « Restaurer le point de "
                 "restauration précédent ».\n"
                 "3. En dernier recours, « Réinitialiser ce PC » dans ce même menu garde vos fichiers.\n\n"
                 "Le journal ci-dessous indique la cause ; il est aussi dans /run/winlux."),
        "reboot": "Redémarrer", "poweroff": "Éteindre", "terminal": "Terminal",
    },
    "en": {
        "title": "Winlux could not open the desktop",
        "body": ("The session services did not start. This mostly happens after a power cut or a "
                 "forced shutdown while the disk was being written.\n\n"
                 "What to do:\n"
                 "1. Click “Restart”: Winlux repairs the account files at every start.\n"
                 "2. If this screen comes back, choose “Winlux secours” (key S) in the boot menu, "
                 "then “Check the file system” or “Restore the previous version”.\n"
                 "3. As a last resort, “Reset this PC” in the same menu keeps your files.\n\n"
                 "The log below shows the cause; it is also in /run/winlux."),
        "reboot": "Restart", "poweroff": "Shut down", "terminal": "Terminal",
    },
    "de": {
        "title": "Winlux konnte den Desktop nicht öffnen",
        "body": ("Die Sitzungsdienste sind nicht gestartet. Das passiert meist nach einem "
                 "Stromausfall oder einem erzwungenen Ausschalten, während auf die Festplatte "
                 "geschrieben wurde.\n\n"
                 "Was tun:\n"
                 "1. Klicken Sie auf „Neu starten“: Winlux repariert die Kontodateien bei jedem Start.\n"
                 "2. Erscheint dieser Bildschirm wieder, wählen Sie im Startmenü „Winlux secours“ "
                 "(Taste S) und dann „Dateisystem prüfen“ oder „Vorherige Version wiederherstellen“.\n"
                 "3. Als letzter Ausweg behält „Diesen PC zurücksetzen“ im selben Menü Ihre Dateien.\n\n"
                 "Das Protokoll unten nennt die Ursache; es liegt auch in /run/winlux."),
        "reboot": "Neu starten", "poweroff": "Herunterfahren", "terminal": "Terminal",
    },
    "es": {
        "title": "Winlux no pudo abrir el escritorio",
        "body": ("Los servicios de la sesión no se iniciaron. Suele ocurrir tras un corte de "
                 "corriente o un apagado forzado mientras se escribía en el disco.\n\n"
                 "Qué hacer:\n"
                 "1. Pulse «Reiniciar»: Winlux repara los archivos de cuentas en cada inicio.\n"
                 "2. Si esta pantalla vuelve, elija «Winlux secours» (tecla S) en el menú de inicio "
                 "y luego «Comprobar el sistema de archivos» o «Restaurar la versión anterior».\n"
                 "3. Como último recurso, «Restablecer este PC» en el mismo menú conserva sus archivos.\n\n"
                 "El registro de abajo indica la causa; también está en /run/winlux."),
        "reboot": "Reiniciar", "poweroff": "Apagar", "terminal": "Terminal",
    },
}


def language():
    try:
        with open("/etc/winlux/locale.conf", encoding="utf-8") as stream:
            for line in stream:
                if line.startswith("LANG="):
                    code = line[5:].strip()[:2].lower()
                    if code in TEXT:
                        return code
    except OSError:
        pass
    code = os.environ.get("LANG", "fr")[:2].lower()
    return code if code in TEXT else "fr"


def log_tail(limit=60):
    lines = []
    for path in LOGS:
        try:
            with open(path, encoding="utf-8", errors="replace") as stream:
                content = stream.read().strip().splitlines()
        except OSError:
            continue
        if content:
            lines.append(f"── {path}")
            lines += content[-limit:]
    return "\n".join(lines) or "—"


def power(action):
    """Redémarrage ou arrêt par l'init (TERM : redémarrer, USR2 : éteindre)."""
    helper = shutil.which("winlux-power")
    if helper and subprocess.run([helper, "execute", action], timeout=60).returncode == 0:
        return
    if os.geteuid() == 0:
        os.kill(1, signal.SIGTERM if action == "reboot" else signal.SIGUSR2)


def terminal_command():
    for name in ("xfce4-terminal", "xterm"):
        path = shutil.which(name)
        if path:
            return [path]
    return None


def main():
    text = TEXT[language()]
    if "--print" in sys.argv:
        print(text["title"])
        print(text["body"])
        return 0
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk
    window = Gtk.Window(title=text["title"])
    window.set_icon_name("preferences-system")
    window.set_decorated(False)
    window.fullscreen()
    window.connect("destroy", Gtk.main_quit)
    outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
    outer.set_border_width(32)
    outer.set_halign(Gtk.Align.CENTER)
    outer.set_size_request(min(900, max(600, window.get_screen().get_width() - 64)), -1)
    title = Gtk.Label()
    title.set_markup(f"<span size='xx-large' weight='bold'>{text['title']}</span>")
    title.set_xalign(0)
    title.set_line_wrap(True)
    outer.pack_start(title, False, False, 0)
    body = Gtk.Label(label=text["body"])
    body.set_xalign(0)
    body.set_line_wrap(True)
    outer.pack_start(body, False, False, 0)
    view = Gtk.TextView()
    view.set_editable(False)
    view.set_monospace(True)
    view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
    view.get_buffer().set_text(log_tail())
    scroller = Gtk.ScrolledWindow()
    scroller.set_min_content_height(200)
    scroller.add(view)
    outer.pack_start(scroller, True, True, 0)
    buttons = Gtk.Box(spacing=12)
    buttons.set_halign(Gtk.Align.END)
    command = terminal_command()
    if command:
        terminal = Gtk.Button(label=text["terminal"])
        terminal.connect("clicked", lambda _b: subprocess.Popen(command, start_new_session=True))
        buttons.pack_start(terminal, False, False, 0)
    for action in ("poweroff", "reboot"):
        button = Gtk.Button(label=text[action])
        button.connect("clicked", lambda _b, a=action: power(a))
        buttons.pack_start(button, False, False, 0)
        if action == "reboot":
            button.get_style_context().add_class("suggested-action")
            button.set_can_default(True)
            window.set_default(button)
    outer.pack_start(buttons, False, False, 0)
    window.add(outer)
    window.show_all()
    Gtk.main()
    return 0


if __name__ == "__main__":
    sys.exit(main())
