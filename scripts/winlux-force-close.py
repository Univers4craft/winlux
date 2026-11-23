#!/usr/bin/python3
"""Forcer la fermeture d'une fenêtre qui ne répond pas, comme Windows.

    winlux-force-close [--hung] [WID]   demande confirmation puis ferme le programme
    winlux-force-close menu             menu de fenêtre d'Openbox (clic droit sur
                                        la barre de titre), traduit

Un programme bloqué reçoit bien WM_DELETE_WINDOW quand on clique sur la croix,
mais ne le traite jamais ; s'il répond encore à _NET_WM_PING, Openbox ne
propose pas non plus de le tuer. Ici : le PID de la fenêtre (_NET_WM_PID)
reçoit SIGTERM, puis SIGKILL au bout de 3 s, seulement s'il appartient au
compte courant, tourne sur cette machine (WM_CLIENT_MACHINE) et n'est pas init.

Pendant la question, le programme reste surveillé (#3259) : s'il meurt, si sa
fenêtre disparaît ou passe à un autre processus (identifiant X réattribué), ou
s'il ouvre une fenêtre rattachée à celle-ci (il répond), la question se retire
seule, sans rien fermer ni rien annoncer.

WINLUX_FORCE_CLOSE_ANSWER=close|wait répond à la question sans fenêtre (tests).
"""
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import time


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
    "{name} ne répond pas": ("{name} is not responding", "{name} no responde", "{name} reagiert nicht"),
    "Forcer la fermeture de {name} ?": ("Force {name} to close?", "¿Forzar el cierre de {name}?", "{name} zwangsweise schließen?"),
    "Si vous fermez le programme, les données non enregistrées seront perdues.":
        ("If you close the program, any unsaved data will be lost.",
         "Si cierra el programa, se perderán los datos no guardados.",
         "Wenn Sie das Programm schließen, gehen nicht gespeicherte Daten verloren."),
    "Fermer le programme": ("Close the program", "Cerrar el programa", "Programm schließen"),
    "Attendre": ("Wait", "Esperar", "Warten"),
    "Ce programme": ("This program", "Este programa", "Dieses Programm"),
    "Réduire": ("Minimize", "Minimizar", "Minimieren"),
    "Agrandir ou restaurer": ("Maximize or restore", "Maximizar o restaurar", "Maximieren oder wiederherstellen"),
    "Déplacer": ("Move", "Mover", "Verschieben"),
    "Redimensionner": ("Size", "Cambiar tamaño", "Größe ändern"),
    "Toujours au premier plan": ("Always on top", "Siempre visible", "Immer im Vordergrund"),
    "Fermer": ("Close", "Cerrar", "Schließen"),
    "Forcer la fermeture": ("Force close", "Forzar cierre", "Schließen erzwingen"),
    "Fenêtre": ("Window", "Ventana", "Fenster"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def xprop(window, *names):
    try:
        return subprocess.run(["xprop", "-id", str(window), *names], capture_output=True, text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def active_window():
    found = re.search(r"window id # (0x[0-9a-fA-F]+)", subprocess.run(
        ["xprop", "-root", "_NET_ACTIVE_WINDOW"], capture_output=True, text=True, timeout=3).stdout)
    return int(found[1], 16) if found and int(found[1], 16) else 0


def parse_window(text):
    """Identifiant de fenêtre décimal ($wid d'Openbox) ou hexadécimal ; 0 si absent."""
    try:
        return int(text, 0)
    except (TypeError, ValueError):
        return 0


def window_info(window):
    output = xprop(window, "_NET_WM_PID", "WM_CLIENT_MACHINE", "_NET_WM_NAME", "WM_NAME", "WM_CLASS")
    pid = re.search(r"_NET_WM_PID\(CARDINAL\) = (\d+)", output)
    machine = re.search(r'WM_CLIENT_MACHINE\(\w+\) = "(.*)"', output)
    name = re.search(r'_NET_WM_NAME\(UTF8_STRING\) = "(.*)"', output) or re.search(r'WM_NAME\(\w+\) = "(.*)"', output)
    klass = re.search(r'WM_CLASS\(\w+\) = "(.*)", "(.*)"', output)
    title = (name[1] if name else "") or (klass[2] if klass else "")
    title = title.replace('\\"', '"')
    # « Document — Programme » : Windows nomme le programme, pas le document.
    if " — " in title or " - " in title:
        title = re.split(r" [—-] ", title)[-1]
    return {"pid": int(pid[1]) if pid else 0, "machine": machine[1] if machine else None,
            "name": title.strip() or _t("Ce programme")}


def refusal(info):
    """Raison de ne pas toucher à ce processus, ou None."""
    pid = info["pid"]
    if pid <= 1 or pid == os.getpid():
        return f"PID {pid} : processus protégé"
    if info["machine"] is not None and info["machine"] not in (socket.gethostname(), socket.getfqdn(), "localhost"):
        return f"fenêtre d'une autre machine ({info['machine']})"
    try:
        owner = os.stat(f"/proc/{pid}").st_uid
    except OSError:
        return f"PID {pid} : processus terminé"
    if owner != os.getuid():
        return f"PID {pid} : appartient à l'utilisateur {owner}, pas à {os.getuid()}"
    return None


def clients():
    """Fenêtres gérées par le gestionnaire de fenêtres (_NET_CLIENT_LIST)."""
    try:
        listing = subprocess.run(["xprop", "-root", "_NET_CLIENT_LIST"], capture_output=True, text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    # « _NET_CLIENT_LIST(WINDOW): window id # 0x…, 0x… » ; rien de lisible : on ne sait pas.
    if not listing.startswith("_NET_CLIENT_LIST(WINDOW)"):
        return None
    return {int(item, 16) for item in re.findall(r"0x[0-9a-fA-F]+", listing)}


def start_time(pid):
    """Date de départ du processus (champ 22 de /proc/PID/stat), ou None s'il n'existe plus."""
    try:
        return Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]
    except (OSError, IndexError):
        return None


def same_process(pid, start):
    return start is not None and start_time(pid) == start and alive(pid)


class Target:
    """Le programme visé par la question : un processus (PID et date de départ) et sa fenêtre."""

    def __init__(self, window, pid, start):
        self.window, self.pid, self.start = window, pid, start
        self.known = clients() or set()

    def reason_to_withdraw(self):
        """Pourquoi la question n'a plus lieu d'être, ou None."""
        if not same_process(self.pid, self.start):
            return "programme terminé"
        now = clients()
        if now is None:
            return None  # serveur X momentanément muet : on ne conclut rien
        if self.window not in now:
            return "fenêtre fermée"
        found = re.search(r"_NET_WM_PID\(CARDINAL\) = (\d+)", xprop(self.window, "_NET_WM_PID"))
        if not found or int(found[1]) != self.pid:
            return "fenêtre passée à un autre processus"
        for other in now - self.known:
            output = xprop(other, "_NET_WM_PID", "WM_TRANSIENT_FOR")
            pid = re.search(r"_NET_WM_PID\(CARDINAL\) = (\d+)", output)
            parent = re.search(r"window id # (0x[0-9a-fA-F]+)", output)
            if pid and int(pid[1]) == self.pid and parent and int(parent[1], 16) == self.window:
                return "le programme répond"
        self.known |= now
        return None


WITHDRAWN = "withdrawn"


def ask(info, hung, target=None):
    """« close », « wait » ou WITHDRAWN (question retirée : le programme n'est plus à fermer)."""
    answer = os.environ.get("WINLUX_FORCE_CLOSE_ANSWER")
    if answer:
        if target is not None and target.reason_to_withdraw():
            return WITHDRAWN
        return "close" if answer == "close" else "wait"
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk
    text = (_t("{name} ne répond pas") if hung else _t("Forcer la fermeture de {name} ?")).format(name=info["name"])
    dialog = Gtk.MessageDialog(message_type=Gtk.MessageType.WARNING, text=text,
                               secondary_text=_t("Si vous fermez le programme, les données non enregistrées seront perdues."))
    dialog.set_title(text)
    dialog.set_keep_above(True)
    dialog.set_position(Gtk.WindowPosition.CENTER)
    dialog.add_button(_t("Attendre"), Gtk.ResponseType.CANCEL)
    dialog.add_button(_t("Fermer le programme"), Gtk.ResponseType.ACCEPT)
    dialog.set_default_response(Gtk.ResponseType.CANCEL)
    withdrawn = []

    def watch():
        reason = target.reason_to_withdraw()
        if reason:
            withdrawn.append(reason)
            dialog.response(Gtk.ResponseType.NONE)
            return False
        return True
    if target is not None:
        GLib.timeout_add(int(float(os.environ.get("WINLUX_FORCE_CLOSE_POLL", "0.25")) * 1000), watch)
    response = dialog.run()
    dialog.destroy()
    while Gtk.events_pending():
        Gtk.main_iteration()
    if withdrawn:
        print(f"winlux-force-close : question retirée ({withdrawn[0]})", file=sys.stderr)
        return WITHDRAWN
    # Un clic sur « Fermer le programme » ne vaut que pour le processus visé : vérifié de nouveau ici.
    if response == Gtk.ResponseType.ACCEPT and target is not None and target.reason_to_withdraw():
        return WITHDRAWN
    return "close" if response == Gtk.ResponseType.ACCEPT else "wait"


def alive(pid):
    try:
        state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
    except (OSError, IndexError):
        return False
    return state not in ("Z", "X")


def terminate(pid, start):
    """SIGTERM, puis SIGKILL après le délai de grâce ; même processus (date de départ) seulement.

    Vrai si le processus a reçu un signal ; faux s'il était déjà terminé (rien n'a été fermé).
    """
    def same():
        return start_time(pid) == start
    if not same() or not alive(pid):
        return False
    os.kill(pid, signal.SIGTERM)
    deadline = time.monotonic() + float(os.environ.get("WINLUX_FORCE_CLOSE_GRACE", "3"))
    while time.monotonic() < deadline:
        if not alive(pid) or not same():
            return True
        time.sleep(0.1)
    if same() and alive(pid):
        os.kill(pid, signal.SIGKILL)
    return True


def window_menu():
    """Menu de fenêtre d'Openbox (menu « pipe ») : le menu intégré ne peut pas être complété."""
    def item(label, action):
        return f'<item label="{_t(label)}">{action}</item>'
    print('<openbox_pipe_menu>')
    print(item("Réduire", '<action name="Iconify"/>'))
    print(item("Agrandir ou restaurer", '<action name="ToggleMaximize"/>'))
    print(item("Déplacer", '<action name="Move"/>'))
    print(item("Redimensionner", '<action name="Resize"/>'))
    print(item("Toujours au premier plan", '<action name="ToggleAlwaysOnTop"/>'))
    print('<separator/>')
    print(item("Fermer", '<action name="Close"/><action name="Execute"><command>winlux-close-watch $wid</command></action>'))
    print(item("Forcer la fermeture", '<action name="Execute"><command>winlux-force-close $wid</command></action>'))
    print('</openbox_pipe_menu>')
    return 0


def main(argv):
    if argv[1:2] == ["menu"]:
        return window_menu()
    hung = "--hung" in argv
    rest = [item for item in argv[1:] if item != "--hung"]
    window = parse_window(rest[0]) if rest else 0
    if not window:
        window = active_window()
    if not window:
        print("winlux-force-close : aucune fenêtre", file=sys.stderr)
        return 1
    info = window_info(window)
    reason = refusal(info)
    if reason:
        print(f"winlux-force-close : refusé, {reason}", file=sys.stderr)
        return 1
    start = start_time(info["pid"])
    if start is None:
        return 1
    answer = ask(info, hung, Target(window, info["pid"], start))
    if answer != "close":
        return 0
    if terminate(info["pid"], start):
        print(f"winlux-force-close : {info['name']} (PID {info['pid']}) fermé")
    else:
        print(f"winlux-force-close : {info['name']} (PID {info['pid']}) déjà terminé, rien à fermer", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
