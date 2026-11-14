#!/usr/bin/python3
"""Arrêt et redémarrage de Winlux.

  winlux-power ask shutdown|reboot   confirmation, fermeture des applications, écran d'arrêt
  winlux-power now shutdown|reboot   sans confirmation (bouton d'alimentation, anciens appels)
  winlux-power screen ACTION         seulement l'écran « Arrêt en cours » (détaché)
  winlux-power execute poweroff|reboot   étape finale, sans interface
  winlux-power serve                 (root) relaie les demandes des comptes non root

L'init de Winlux est un script shell (pas de systemd ni de logind) : il arrête
la machine sur SIGUSR2 et la redémarre sur SIGTERM, après avoir terminé les
services et synchronisé les disques. Les commandes « poweroff »/« reboot » de
l'image pointent vers le systemctl d'Ubuntu, qui échoue sans systemd : c'est
pour cela que le bouton Arrêter ne faisait rien. Aucun pkexec ni polkit : le
bureau principal tourne en root et signale l'init directement ; un second
compte (changement rapide d'utilisateur) passe par le socket de « serve ».
"""
import math
import os
import shutil
import signal
import socket
import struct
import subprocess
import sys
import time


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
    "Arrêter le PC ?": ("Shut down the PC?", "¿Apagar el equipo?", "PC herunterfahren?"),
    "Redémarrer le PC ?": ("Restart the PC?", "¿Reiniciar el equipo?", "PC neu starten?"),
    "Les applications ouvertes vont se fermer. Enregistrez votre travail avant de continuer.": (
        "Open apps will close. Save your work before you continue.",
        "Las aplicaciones abiertas se cerrarán. Guarde su trabajo antes de continuar.",
        "Geöffnete Apps werden geschlossen. Speichern Sie Ihre Arbeit, bevor Sie fortfahren."),
    "Arrêter": ("Shut down", "Apagar", "Herunterfahren"),
    "Redémarrer": ("Restart", "Reiniciar", "Neu starten"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Arrêt en cours": ("Shutting down", "Apagando", "Wird heruntergefahren"),
    "Redémarrage en cours": ("Restarting", "Reiniciando", "Wird neu gestartet"),
    "Redémarrage de la session": ("Restarting the session", "Reiniciando la sesión", "Sitzung wird neu gestartet"),
    "Fermeture des applications…": ("Closing apps…", "Cerrando aplicaciones…", "Apps werden geschlossen…"),
    "Ces applications empêchent l’arrêt": ("These apps are preventing shutdown", "Estas aplicaciones impiden el apagado",
                                           "Diese Apps verhindern das Herunterfahren"),
    "Ces applications empêchent le redémarrage": ("These apps are preventing restart", "Estas aplicaciones impiden el reinicio",
                                                  "Diese Apps verhindern den Neustart"),
    "Enregistrez votre travail dans ces applications, ou continuez : les modifications non enregistrées seront perdues.": (
        "Save your work in these apps, or continue: unsaved changes will be lost.",
        "Guarde su trabajo en estas aplicaciones o continúe: los cambios no guardados se perderán.",
        "Speichern Sie Ihre Arbeit in diesen Apps oder fahren Sie fort: Nicht gespeicherte Änderungen gehen verloren."),
    "Arrêter quand même": ("Shut down anyway", "Apagar de todos modos", "Trotzdem herunterfahren"),
    "Redémarrer quand même": ("Restart anyway", "Reiniciar de todos modos", "Trotzdem neu starten"),
    "Déverrouiller": ("Unlock", "Desbloquear", "Entsperren"),
    "Déverrouillez le PC pour enregistrer votre travail dans ces applications ou pour arrêter quand même.": (
        "Unlock the PC to save your work in these apps or to shut down anyway.",
        "Desbloquee el equipo para guardar su trabajo en estas aplicaciones o para apagarlo de todos modos.",
        "Entsperren Sie den PC, um Ihre Arbeit in diesen Apps zu speichern oder trotzdem herunterzufahren."),
    "Déverrouillez le PC pour enregistrer votre travail dans ces applications ou pour redémarrer quand même.": (
        "Unlock the PC to save your work in these apps or to restart anyway.",
        "Desbloquee el equipo para guardar su trabajo en estas aplicaciones o para reiniciarlo de todos modos.",
        "Entsperren Sie den PC, um Ihre Arbeit in diesen Apps zu speichern oder trotzdem neu zu starten."),
    "Impossible d’arrêter le PC": ("Couldn’t shut down the PC", "No se pudo apagar el equipo", "PC konnte nicht heruntergefahren werden"),
    "Impossible de redémarrer le PC": ("Couldn’t restart the PC", "No se pudo reiniciar el equipo", "PC konnte nicht neu gestartet werden"),
    "Une mise à jour est en cours d’installation. Réessayez quand elle sera terminée.": (
        "An update is being installed. Try again when it has finished.",
        "Se está instalando una actualización. Inténtelo de nuevo cuando termine.",
        "Ein Update wird installiert. Versuchen Sie es nach Abschluss erneut."),
    "Le service d’alimentation ne répond pas.": ("The power service isn’t responding.", "El servicio de energía no responde.",
                                                  "Der Energiedienst antwortet nicht."),
    "OK": ("OK", "Aceptar", "OK"),
    "D’autres personnes sont connectées à ce PC : {names}. Si vous l’arrêtez maintenant, leurs travaux non enregistrés seront perdus.": (
        "Other people are signed in to this PC: {names}. If you shut it down now, they could lose unsaved work.",
        "Hay otras personas con sesión iniciada en este equipo: {names}. Si lo apaga ahora, podrían perder el trabajo no guardado.",
        "Andere Personen sind an diesem PC angemeldet: {names}. Wenn Sie ihn jetzt herunterfahren, kann nicht gespeicherte Arbeit verloren gehen."),
    "D’autres personnes sont connectées à ce PC : {names}. Si vous le redémarrez maintenant, leurs travaux non enregistrés seront perdus.": (
        "Other people are signed in to this PC: {names}. If you restart it now, they could lose unsaved work.",
        "Hay otras personas con sesión iniciada en este equipo: {names}. Si lo reinicia ahora, podrían perder el trabajo no guardado.",
        "Andere Personen sind an diesem PC angemeldet: {names}. Wenn Sie ihn jetzt neu starten, kann nicht gespeicherte Arbeit verloren gehen."),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


ROOT = os.environ.get("WINLUX_CONTROL_ROOT", "")
SOCKET = os.environ.get("WINLUX_POWER_SOCKET", ROOT + "/run/winlux-power.sock")
INIT_PID = int(os.environ.get("WINLUX_POWER_INIT_PID", "1"))
SESSIONS = ROOT + "/run/winlux/sessions"
UPDATE_BUSY = "Une mise à jour est en cours d’installation. Réessayez quand elle sera terminée."
NO_SERVICE = "Le service d’alimentation ne répond pas."
CLOSE_TIMEOUT = float(os.environ.get("WINLUX_POWER_CLOSE_TIMEOUT", "20"))
# Canal vers le processus de garde (voir gui()) : l'écran y note son étape.
STATE_FD = None


def report(state):
    """Note l'étape de l'écran pour le gardien : go, done, cancel, blocked, error."""
    if STATE_FD is None:
        return
    try:
        os.write(STATE_FD, state.encode() + b"\n")
    except OSError:
        pass


def normalize(action):
    action = (action or "").strip().lower()
    if action in ("shutdown", "poweroff", "halt"):
        return "poweroff"
    if action in ("reboot", "restart"):
        return "reboot"
    if action == "restart-session":
        return action
    raise ValueError(action)


# ── Étape finale (sans interface) ───────────────────────────────────────────

def updates_busy():
    """Une installation de mise à jour en cours bloque l'arrêt (comme l'init)."""
    for tool in ("winlux-update", "winlux-sysupdate"):
        path = shutil.which(tool)
        if not path:
            continue
        try:
            if subprocess.run([path, "--busy"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                              timeout=10).returncode == 0:
                return True
        except (OSError, subprocess.SubprocessError):
            pass
    return False


def signal_init(action):
    """Demande à l'init d'arrêter ou de redémarrer la machine.

    systemd (si un jour il est PID 1) reçoit « systemctl poweroff|reboot » ;
    l'init shell de Winlux (et l'init BusyBox) suivent la même convention de
    signaux : SIGUSR2 = arrêt, SIGTERM = redémarrage."""
    if INIT_PID == 1 and os.path.isdir("/run/systemd/system") and shutil.which("systemctl"):
        return subprocess.run(["systemctl", action], check=False).returncode == 0
    os.kill(INIT_PID, signal.SIGUSR2 if action == "poweroff" else signal.SIGTERM)
    return True


def execute(action):
    """Renvoie None si l'arrêt est lancé, sinon le message d'erreur (français)."""
    action = normalize(action)
    if action not in ("poweroff", "reboot"):
        raise ValueError(action)
    if os.geteuid() != 0 and not ROOT:
        return request_from_service(action)
    if updates_busy():
        return UPDATE_BUSY
    os.sync()
    try:
        if signal_init(action):
            return None
    except OSError as error:
        return f"{NO_SERVICE} ({error})"
    return NO_SERVICE


def request_from_service(action):
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(15)
            client.connect(SOCKET)
            client.sendall(action.encode() + b"\n")
            answer = client.makefile("r", encoding="utf-8").readline().strip()
    except OSError:
        return NO_SERVICE
    if answer == "ok":
        return None
    return answer or NO_SERVICE


def session_users():
    try:
        return set(os.listdir(SESSIONS))
    except OSError:
        return set()


def other_sessions(me=None):
    """Noms affichés des autres comptes dont la session est ouverte (registre
    de winlux-sessions, écran X toujours là), comme l'avertissement de Windows
    « D'autres personnes utilisent ce PC »."""
    if me is None:
        try:
            import pwd
            me = pwd.getpwuid(os.getuid()).pw_name
        except (KeyError, ImportError):
            me = ""
    sockets = os.environ.get("WINLUX_X11_SOCKETS", "/tmp/.X11-unix")
    full = {}
    try:
        for line in open(ROOT + "/etc/passwd", encoding="utf-8", errors="replace"):
            parts = line.split(":")
            if len(parts) >= 5:
                full[parts[0]] = parts[4].split(",")[0].strip()
    except OSError:
        pass
    names = []
    for user in sorted(session_users()):
        if user == me or user.startswith("."):
            continue
        display = ""
        try:
            for line in open(os.path.join(SESSIONS, user), encoding="utf-8", errors="replace"):
                if line.startswith("display="):
                    display = line.strip()[8:]
        except OSError:
            continue
        number = display.rpartition(":")[2].split(".")[0]
        if not number.isdigit() or not os.path.exists(os.path.join(sockets, "X" + number)):
            continue
        names.append(full.get(user) or user)
    return names


def peer_allowed(uid):
    """root, ou un compte qui a un bureau ouvert (pas le compte isolé du navigateur)."""
    if uid == 0:
        return True
    try:
        import pwd
        name = pwd.getpwuid(uid).pw_name
    except (KeyError, ImportError):
        return False
    return name in session_users()


def serve():
    """Relais root : un compte connecté peut arrêter le PC, comme sous Windows."""
    if os.geteuid() != 0 and not ROOT:
        print("winlux-power serve doit tourner en root", file=sys.stderr)
        return 1
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as probe:
            probe.connect(SOCKET)
        return 0  # déjà servi
    except OSError:
        pass
    try:
        os.unlink(SOCKET)
    except FileNotFoundError:
        pass
    # Le socket n'apparaît sous son vrai nom qu'une fois à l'écoute : un
    # client qui le voit ne tombe jamais entre bind() et listen() (la CI
    # obtenait alors « service absent » au lieu de « Accès refusé »).
    pending = SOCKET + ".new"
    try:
        os.unlink(pending)
    except FileNotFoundError:
        pass
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(pending)
    os.chmod(pending, 0o666)
    server.listen(4)
    os.replace(pending, SOCKET)
    while True:
        connection, _ = server.accept()
        with connection:
            try:
                credentials = connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i"))
                _, uid, _ = struct.unpack("3i", credentials)
                connection.settimeout(5)
                request = connection.makefile("r", encoding="utf-8").readline().strip()
                if not peer_allowed(uid):
                    connection.sendall("Accès refusé\n".encode())
                    continue
                error = execute(normalize(request)) if request in ("poweroff", "reboot") else "Action inconnue"
                connection.sendall(((error or "ok") + "\n").encode())
            except (OSError, ValueError):
                continue


# ── Interface GTK ───────────────────────────────────────────────────────────

PALETTES = {
    # Mêmes couleurs que le menu Démarrer (contraste ≥ 4,5:1, test-shell-contrast.py).
    "dark": {"card": "#2b2840", "fg": "#f7f5ff", "dim": "#c9c2e6", "border": "#4d4670", "footer": "#211e33",
             "button": "#3a3555", "button_hover": "#463f66", "accent": "#7c4dff", "accent_hover": "#8d63ff",
             "on_accent": "#ffffff", "icon_bg": "#3d335f", "icon": "#d3c4ff"},
    "light": {"card": "#fbfaff", "fg": "#1f1b2e", "dim": "#4d4766", "border": "#d6cfee", "footer": "#efebf9",
              "button": "#ffffff", "button_hover": "#ece7fa", "accent": "#6a3df0", "accent_hover": "#5a2fe0",
              "on_accent": "#ffffff", "icon_bg": "#e8e0ff", "icon": "#5b2fe6"},
}
CSS = """
#winlux-power-card {{ background: {card}; border: 1px solid {border}; border-radius: 8px;
    box-shadow: 0 16px 48px rgba(0,0,0,0.45); }}
#winlux-power-card box {{ background: transparent; }}
#winlux-power-card label {{ color: {fg}; text-shadow: none; }}
#winlux-power-card .title {{ font-size: 15pt; font-weight: 600; }}
#winlux-power-card .body {{ color: {dim}; font-size: 10.5pt; }}
#winlux-power-card .app {{ font-size: 10.5pt; }}
#winlux-power-card .footer {{ background: {footer}; border-top: 1px solid {border};
    border-radius: 0 0 8px 8px; padding: 20px 24px; }}
#winlux-power-card .icon-circle {{ background: {icon_bg}; border-radius: 999px; padding: 12px; }}
#winlux-power-card .icon-circle image {{ color: {icon}; -gtk-icon-shadow: none; }}
#winlux-power-card image {{ color: {fg}; -gtk-icon-shadow: none; }}
#winlux-power-card button {{ background: {button}; background-image: none; color: {fg}; border: 1px solid {border};
    border-radius: 4px; box-shadow: none; text-shadow: none; padding: 6px 18px; min-height: 22px; min-width: 96px; }}
#winlux-power-card button label {{ color: {fg}; }}
#winlux-power-card button:hover {{ background: {button_hover}; }}
#winlux-power-card button:focus {{ outline-color: {accent}; }}
#winlux-power-card button.accent {{ background: {accent}; border-color: {accent}; }}
#winlux-power-card button.accent label {{ color: {on_accent}; font-weight: 600; }}
#winlux-power-card button.accent:hover {{ background: {accent_hover}; }}
"""

SCREEN_BG = (0.067, 0.055, 0.137)   # nuit Winlux (#110e23)


def gtk_modules():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gtk, Gdk, GLib
    return Gtk, Gdk, GLib


def dark_theme(Gtk):
    forced = os.environ.get("GTK_THEME")
    if forced:
        return "dark" in forced.casefold()
    settings = Gtk.Settings.get_default()
    name = (settings.get_property("gtk-theme-name") or "").casefold()
    if name.startswith("winlux-"): return "-dark" in name  # thème de winlux-personalize
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name


def _screen_suffix():
    """Même nommage que winlux-lock : :0 sans suffixe, :1 → « -1 »."""
    display = os.environ.get("DISPLAY", ":0").rpartition(":")[2].split(".")[0]
    return "" if display in ("", "0") else "-" + "".join(c for c in display if c.isdigit())


def screen_locked():
    """Vrai si winlux-lock tient l'écran (pid vivant dans son marqueur)."""
    try:
        with open(ROOT + "/run/winlux/lock/active" + _screen_suffix()) as marker:
            pid = int(marker.read().strip())
        os.kill(pid, 0)
    except PermissionError:
        return True
    except (OSError, ValueError):
        return False
    return True


def power_request_path():
    return ROOT + "/run/winlux/lock/power" + _screen_suffix()


def request_lock_yield():
    """Demande à l'écran de verrouillage de céder la saisie à cet écran d'arrêt
    (#2254). winlux-lock vérifie le pid, son propriétaire et son programme."""
    target = power_request_path()
    tmp = f"{target}.{os.getpid()}"
    try:
        with open(tmp, "w") as marker:
            # Lisible par le compte verrouillé quel que soit l'umask : le
            # démon root tourne en umask 077, et une demande en 0600 restait
            # illisible pour winlux-lock (banc 2, 12 nov.). Jamais inscriptible.
            os.fchmod(marker.fileno(), 0o644)
            marker.write(f"{os.getpid()}\n")
        os.replace(tmp, target)
        return True
    except OSError as error:
        log(f"demande de saisie au verrou impossible : {error}")
        try:
            os.unlink(tmp)
        except OSError:
            pass
        return False


def release_lock_yield():
    try:
        with open(power_request_path()) as marker:
            if int(marker.read().strip()) != os.getpid():
                return
        os.unlink(power_request_path())
    except (OSError, ValueError):
        pass


def log(message):
    print(f"winlux-power {time.strftime('%H:%M:%S')} : {message}", file=sys.stderr, flush=True)


def open_windows():
    """Fenêtres d'applications ouvertes : {id: (classe, titre, pid)}.

    Le bureau, la barre des tâches et les fenêtres collantes (« -1 ») ne sont
    pas des applications : on ne les ferme pas."""
    try:
        output = subprocess.check_output(["wmctrl", "-lxp"], text=True, stderr=subprocess.DEVNULL, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return {}
    found = {}
    for line in output.splitlines():
        fields = line.split(None, 5)
        if len(fields) < 5 or fields[1] == "-1":
            continue
        window_class = fields[3].casefold()
        if any(name in window_class for name in ("xfce4-panel", "xfdesktop", "winlux-start", "winlux-power")):
            continue
        title = fields[5] if len(fields) > 5 else fields[3].split(".")[-1]
        found[fields[0]] = (fields[3], title, fields[2])
    return found


class PowerUI:
    """Fenêtre plein écran (au-dessus de tout) : boîte de confirmation sur le
    bureau assombri, puis écran « Arrêt en cours » avec l'anneau de points."""

    def __init__(self, action, confirm=True, screen_only=False):
        self.Gtk, self.Gdk, self.GLib = gtk_modules()
        Gtk, Gdk = self.Gtk, self.Gdk
        self.action = normalize(action)
        self.mode = "confirm" if confirm else "progress"
        self.screen_only = screen_only
        self.started = time.monotonic()
        self.status = ""
        self.result = 1
        display = Gdk.Display.get_default()
        if display is None:
            raise RuntimeError("pas d'affichage")
        screen = Gdk.Screen.get_default()
        root = screen.get_root_window()
        self.width, self.height = root.get_width(), root.get_height()
        # Capture du bureau : fond assombri sans dépendre d'un compositeur.
        self.backdrop = None
        if confirm:
            try:
                self.backdrop = Gdk.pixbuf_get_from_window(root, 0, 0, self.width, self.height)
            except Exception:
                self.backdrop = None
        self.css = Gtk.CssProvider()
        Gtk.StyleContext.add_provider_for_screen(screen, self.css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)
        palette = PALETTES["dark" if dark_theme(Gtk) else "light"]
        self.css.load_from_data(CSS.format(**palette).encode())

        self.window = Gtk.Window(type=Gtk.WindowType.POPUP)
        self.window.set_name("winlux-power")
        self.window.set_title("Winlux")
        self.window.set_app_paintable(True)
        self.window.move(0, 0)
        self.window.set_default_size(self.width, self.height)
        self.window.set_size_request(self.width, self.height)
        self.window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK | Gdk.EventMask.KEY_PRESS_MASK)
        self.window.connect("key-press-event", self.on_key)
        self.window.connect("button-press-event", self.on_click)
        self.window.connect("destroy", lambda *_: Gtk.main_level() and Gtk.main_quit())
        self.overlay = Gtk.Overlay()
        self.canvas = Gtk.DrawingArea()
        self.canvas.connect("draw", self.draw)
        self.overlay.add(self.canvas)
        self.card_holder = Gtk.Box(halign=Gtk.Align.CENTER, valign=Gtk.Align.CENTER)
        self.overlay.add_overlay(self.card_holder)
        self.window.add(self.overlay)
        self.card = None
        self.ticker = None
        self.locked = screen_locked()
        if self.mode == "confirm":
            self.show_confirm()
        else:
            self.start_shutdown()
        self.window.show_all()
        # Session verrouillée (#2254) : l'écran de verrouillage tient la saisie
        # et cachait la carte « Ces applications empêchent l'arrêt ». Il la
        # cède à cet écran, plein écran et sans rien montrer de la session.
        self.yield_requested = self.locked and request_lock_yield()
        self.window.connect("destroy", lambda *_: release_lock_yield())
        self.grabbed = False
        self.grab_failures = 0
        self.GLib.idle_add(self.grab)
        self.GLib.timeout_add(1000, self.watch_lock)

    def watch_lock(self):
        """Verrouillage survenu pendant l'écran d'arrêt (inactivité pendant
        l'attente d'une application) : même passation que ci-dessus, et cet
        écran repasse au premier plan."""
        if self.mode == "unlock":
            return False  # wait_unlock relance cette veille après le déverrouillage
        if self.yield_requested or not screen_locked():
            return not self.yield_requested
        self.locked = True
        self.yield_requested = request_lock_yield()
        gdk_window = self.window.get_window()
        if gdk_window is not None:
            gdk_window.raise_()
        if not self.grabbed:
            self.GLib.idle_add(self.grab)
        if self.mode == "blocked":
            # Plus de titres de fenêtres sur un écran verrouillé.
            self.show_blocked(self.blockers)
        log("écran verrouillé pendant l'arrêt : saisie demandée au verrou")
        return False

    # Saisie exclusive : la fenêtre POPUP n'a pas de gestionnaire de fenêtres.
    def grab(self):
        gdk_window = self.window.get_window()
        if gdk_window is None:
            return False
        seat = self.window.get_display().get_default_seat()
        status = seat.grab(gdk_window, self.Gdk.SeatCapabilities.ALL, True, None, None, None, None)
        if status == self.Gdk.GrabStatus.SUCCESS:
            if self.grab_failures:
                log(f"saisie obtenue après {self.grab_failures} refus")
            self.grabbed = True
            return False
        self.grab_failures += 1
        if self.grab_failures == 1:
            # Échec auparavant silencieux : la carte restait affichée mais
            # inutilisable, sans aucune trace (#2254).
            log(f"saisie refusée ({status.value_nick}), "
                + ("écran de verrouillage actif, nouvel essai" if self.locked else "nouvel essai"))
        # On réessaie tant que l'écran est là : la carte doit rester atteignable.
        self.GLib.timeout_add(20 if self.yield_requested else 50, self.grab)
        return False

    def ungrab(self):
        self.window.get_display().get_default_seat().ungrab()
        release_lock_yield()

    # ── Cartes ──
    def set_card(self, widget):
        if self.card is not None:
            self.card_holder.remove(self.card)
        self.card = widget
        if widget is not None:
            self.card_holder.pack_start(widget, False, False, 0)
            widget.show_all()
        self.canvas.queue_draw()

    def make_card(self, icon, title, body, buttons, extra=None):
        Gtk = self.Gtk
        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        card.set_name("winlux-power-card")
        card.set_size_request(min(460, self.width - 32), -1)
        content = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=18, margin=26)
        circle = Gtk.Box(valign=Gtk.Align.START)
        circle.get_style_context().add_class("icon-circle")
        circle.pack_start(Gtk.Image.new_from_icon_name(icon, Gtk.IconSize.DND), False, False, 0)
        content.pack_start(circle, False, False, 0)
        text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        heading = Gtk.Label(label=title, xalign=0, wrap=True)
        heading.get_style_context().add_class("title")
        text.pack_start(heading, False, False, 0)
        if body:
            label = Gtk.Label(label=body, xalign=0, wrap=True, max_width_chars=44)
            label.get_style_context().add_class("body")
            text.pack_start(label, False, False, 0)
        if extra is not None:
            text.pack_start(extra, False, False, 4)
        content.pack_start(text, True, True, 0)
        card.pack_start(content, True, True, 0)
        footer = Gtk.Box(spacing=8, homogeneous=True)
        footer.get_style_context().add_class("footer")
        first = None
        for label, callback, accent in buttons:
            button = Gtk.Button(label=label)
            if accent:
                button.get_style_context().add_class("accent")
                first = first or button
            button.connect("clicked", lambda *_, cb=callback: cb())
            footer.pack_start(button, True, True, 0)
        card.pack_end(footer, False, False, 0)
        if first is not None:
            first.set_can_default(True)
            self.GLib.idle_add(first.grab_focus)
        return card

    def show_confirm(self):
        shutdown = self.action == "poweroff"
        self.mode = "confirm"
        others = other_sessions()
        if others:
            # Une autre session (un administrateur, par exemple) serait coupée
            # sans prévenir : avertissement, et « Annuler » par défaut.
            body = _t("D’autres personnes sont connectées à ce PC : {names}. Si vous l’arrêtez maintenant, leurs travaux non enregistrés seront perdus."
                      if shutdown else
                      "D’autres personnes sont connectées à ce PC : {names}. Si vous le redémarrez maintenant, leurs travaux non enregistrés seront perdus.")
            self.set_card(self.make_card(
                "dialog-warning-symbolic",
                _t("Arrêter le PC ?") if shutdown else _t("Redémarrer le PC ?"),
                body.format(names=", ".join(others)),
                [(_t("Annuler"), self.cancel, True),
                 (_t("Arrêter quand même") if shutdown else _t("Redémarrer quand même"), self.start_shutdown, False)]))
            return
        self.set_card(self.make_card(
            "system-shutdown-symbolic" if shutdown else "view-refresh-symbolic",
            _t("Arrêter le PC ?") if shutdown else _t("Redémarrer le PC ?"),
            _t("Les applications ouvertes vont se fermer. Enregistrez votre travail avant de continuer."),
            [(_t("Arrêter") if shutdown else _t("Redémarrer"), self.start_shutdown, True),
             (_t("Annuler"), self.cancel, False)]))

    def show_blocked(self, blockers):
        Gtk = self.Gtk
        shutdown = self.action == "poweroff"
        self.mode = "blocked"
        self.blockers = blockers
        report("blocked")
        listing = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        seen = set()
        for window_class, title, _pid in blockers.values():
            key = window_class if self.locked else title
            if key in seen:
                continue
            seen.add(key)
            row = Gtk.Box(spacing=10)
            icon = window_class.split(".")[0].casefold()
            row.pack_start(Gtk.Image.new_from_icon_name(icon if Gtk.IconTheme.get_default().has_icon(icon)
                                                        else "application-x-executable", Gtk.IconSize.LARGE_TOOLBAR),
                           False, False, 0)
            # Session verrouillée : le nom de l'application, jamais le titre de
            # sa fenêtre (nom de document, objet d'un message…).
            shown = window_class.split(".")[-1] if self.locked else title
            name = Gtk.Label(label=shown, xalign=0, ellipsize=3, max_width_chars=40)
            name.get_style_context().add_class("app")
            row.pack_start(name, True, True, 0)
            listing.pack_start(row, False, False, 0)
            if len(seen) >= 6:
                break
        title = _t("Ces applications empêchent l’arrêt") if shutdown else _t("Ces applications empêchent le redémarrage")
        if self.locked:
            # #2300, décision de damien du 14 nov. : sur un écran verrouillé,
            # personne ne force l'arrêt sans le mot de passe. La carte n'offre
            # que « Annuler » et « Déverrouiller » ; l'arrêt reste en attente,
            # sans délai, jusqu'à la réponse de l'utilisateur déverrouillé.
            # Windows propose « Arrêter quand même » sur l'écran verrouillé :
            # Winlux s'en écarte volontairement.
            self.set_card(self.make_card(
                "dialog-warning-symbolic", title,
                _t("Déverrouillez le PC pour enregistrer votre travail dans ces applications ou pour arrêter quand même."
                   if shutdown else
                   "Déverrouillez le PC pour enregistrer votre travail dans ces applications ou pour redémarrer quand même."),
                [(_t("Annuler"), self.cancel, True),
                 (_t("Déverrouiller"), self.unlock_first, False)],
                extra=listing))
            return
        self.set_card(self.make_card(
            "dialog-warning-symbolic", title,
            _t("Enregistrez votre travail dans ces applications, ou continuez : les modifications non enregistrées seront perdues."),
            # #2299 : « Annuler » par défaut. Sur un écran verrouillé, la touche
            # qui réveille l'écran déclenchait « Arrêter quand même » et
            # perdait le travail non enregistré.
            [(_t("Annuler"), self.cancel, True),
             (_t("Arrêter quand même") if shutdown else _t("Redémarrer quand même"), self.finish, False)],
            extra=listing))

    def unlock_first(self):
        """« Déverrouiller » (#2300) : l'écran d'arrêt s'efface et rend la
        saisie au verrou, qui demande le mot de passe. L'arrêt reste en
        attente ; une fois la session déverrouillée, la carte complète revient
        avec « Arrêter quand même »."""
        self.mode = "unlock"
        report("blocked")
        self.ungrab()
        self.grabbed = False
        self.yield_requested = False
        self.window.hide()
        log("déverrouillage demandé : l'arrêt attend la session déverrouillée")
        self.GLib.timeout_add(500, self.wait_unlock)

    def wait_unlock(self):
        if self.mode != "unlock":
            return False
        if screen_locked():
            return True
        self.locked = False
        self.window.show_all()
        gdk_window = self.window.get_window()
        if gdk_window is not None:
            gdk_window.raise_()
        self.GLib.idle_add(self.grab)
        self.GLib.timeout_add(1000, self.watch_lock)
        remaining = {key: value for key, value in open_windows().items() if key in getattr(self, "windows", {})}
        log("session déverrouillée : carte de l'arrêt en attente affichée")
        # Même si tout a été fermé entre-temps, on demande : l'arrêt ne part
        # jamais sans réponse.
        self.show_blocked(remaining or self.blockers)
        return False

    def show_error(self, message):
        self.mode = "error"
        report("error")
        self.set_card(self.make_card(
            "dialog-error-symbolic",
            _t("Impossible d’arrêter le PC") if self.action == "poweroff" else _t("Impossible de redémarrer le PC"),
            _t(message), [(_t("OK"), self.cancel, True)]))

    # ── Déroulement ──
    def cancel(self):
        self.result = 1
        report("cancel")
        self.ungrab()
        self.window.destroy()

    def start_shutdown(self):
        self.mode = "progress"
        self.started = time.monotonic()
        if not self.screen_only:
            report("go")
        self.set_card(None)
        self.animate()
        if self.screen_only:
            # Relance du bureau : l'écran s'efface une fois le bureau revenu.
            self.GLib.timeout_add_seconds(12 if self.action == "restart-session" else 90, self.window.destroy)
            return
        self.status = _t("Fermeture des applications…")
        self.windows = open_windows()
        for window in self.windows:
            subprocess.run(["wmctrl", "-ic", window], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.deadline = time.monotonic() + CLOSE_TIMEOUT
        self.GLib.timeout_add(500, self.wait_windows)

    def wait_windows(self):
        if self.mode != "progress":
            return False
        remaining = {key: value for key, value in open_windows().items() if key in self.windows}
        if not remaining:
            self.finish()
            return False
        if time.monotonic() >= self.deadline:
            self.show_blocked(remaining)
            return False
        return True

    def finish(self):
        self.mode = "progress"
        self.set_card(None)
        self.animate()
        self.status = ""
        report("go")
        error = execute(self.action)
        if error:
            self.show_error(error)
            return
        self.result = 0
        report("done")
        # L'init termine maintenant tous les processus : l'écran reste affiché.
        # S'il n'a toujours rien fait au bout d'une minute, on le dit.
        self.GLib.timeout_add_seconds(60, self.init_silent)

    def init_silent(self):
        if self.mode == "progress":
            self.show_error(NO_SERVICE)
        return False

    # ── Dessin ──
    def tick(self):
        """Animation de l'anneau, seulement tant que l'écran d'arrêt est affiché."""
        self.canvas.queue_draw()
        if self.mode == "progress":
            return True
        self.ticker = None
        return False

    def animate(self):
        if self.ticker is None:
            self.ticker = self.GLib.timeout_add(16, self.tick)

    def draw(self, _widget, cr):
        width, height = self.width, self.height
        if self.mode == "confirm" and self.backdrop is not None:
            # Bureau figé et assombri derrière la boîte, comme Windows 11.
            self.Gdk.cairo_set_source_pixbuf(cr, self.backdrop, 0, 0)
            cr.paint()
            cr.set_source_rgba(0, 0, 0, 0.55)
            cr.paint()
            return False
        cr.set_source_rgb(*SCREEN_BG)
        cr.paint()
        if self.mode != "progress":
            return False
        # Anneau de points de Windows 11 : cinq points qui accélèrent en bas.
        scale = max(1.0, height / 900)
        cx, cy = width / 2, height / 2 - 20 * scale
        radius = 22 * scale
        elapsed = time.monotonic() - self.started
        period = 1.8
        for index in range(5):
            phase = ((elapsed - index * 0.12) % period) / period
            eased = phase - math.sin(2 * math.pi * phase) / (2 * math.pi)
            angle = math.pi / 2 + 2 * math.pi * eased
            cr.set_source_rgb(1, 1, 1)
            cr.arc(cx + radius * math.cos(angle), cy + radius * math.sin(angle), 3 * scale, 0, 2 * math.pi)
            cr.fill()
        title = {"poweroff": _t("Arrêt en cours"), "reboot": _t("Redémarrage en cours"),
                 "restart-session": _t("Redémarrage de la session")}[self.action]
        self.text(cr, title, cx, cy + radius + 46 * scale, 17 * scale, 1.0)
        if self.status:
            self.text(cr, self.status, cx, cy + radius + 78 * scale, 11 * scale, 0.75)
        return False

    def text(self, cr, value, cx, y, size, alpha):
        import gi
        gi.require_version("PangoCairo", "1.0")
        from gi.repository import Pango, PangoCairo
        layout = PangoCairo.create_layout(cr)
        layout.set_font_description(Pango.FontDescription(f"Segoe UI, Noto Sans, sans {size:.1f}px"))
        layout.set_text(value, -1)
        text_width, text_height = layout.get_pixel_size()
        cr.set_source_rgba(1, 1, 1, alpha)
        cr.move_to(cx - text_width / 2, y - text_height / 2)
        PangoCairo.show_layout(cr, layout)

    def on_key(self, _widget, event):
        if event.keyval == self.Gdk.KEY_Escape and self.mode in ("confirm", "blocked", "error"):
            self.cancel()
            return True
        return False

    def on_click(self, _widget, event):
        # Clic sur le bureau assombri : comme Échap.
        if self.mode == "confirm" and self.card is not None:
            allocation = self.card.get_allocation()
            if not (allocation.x <= event.x <= allocation.x + allocation.width
                    and allocation.y <= event.y <= allocation.y + allocation.height):
                self.cancel()
                return True
        return False

    def run(self):
        self.Gtk.main()
        return self.result


def gui(action, confirm, screen_only=False):
    if screen_only:
        return run_gui(action, confirm, screen_only)
    # L'écran GTK tourne dans un processus enfant. S'il meurt une fois l'arrêt
    # décidé (« Bus error » vu en session live, perte de l'affichage quand les
    # applications se ferment…), le parent, qui n'a chargé ni GTK ni cairo,
    # termine le travail : sinon la machine ne redémarrait jamais.
    global STATE_FD
    read_end, write_end = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(read_end)
        STATE_FD = write_end
        code = 1
        try:
            if os.environ.get("WINLUX_POWER_TEST_GUI_CRASH") == "1":
                os.kill(os.getpid(), signal.SIGBUS)  # test : écran qui plante
            code = run_gui(action, confirm, screen_only)
        finally:
            os._exit(code if isinstance(code, int) else 1)
    os.close(write_end)
    # « now » : l'arrêt est décidé dès le départ ; « ask » : après confirmation.
    state = "pending" if confirm else "go"
    with os.fdopen(read_end, "rb") as states:
        for line in states:
            state = line.decode(errors="replace").strip() or state
    _pid, status = os.waitpid(child, 0)
    # GDK quitte avec le code 1 quand l'affichage disparaît : seul l'état
    # compte. « go » sans « done » = arrêt décidé mais jamais demandé à l'init.
    if state != "go":
        return os.WEXITSTATUS(status) if os.WIFEXITED(status) else 1
    print(f"Écran d'arrêt interrompu (état {status}) : {action} sans écran.", file=sys.stderr)
    message = execute(action)
    if message:
        print(message, file=sys.stderr)
        return 1
    return 0


def run_gui(action, confirm, screen_only=False):
    try:
        ui = PowerUI(action, confirm=confirm, screen_only=screen_only)
    except Exception as error:  # pas d'affichage, GTK absent…
        if screen_only:
            return 0
        print(f"Interface indisponible ({error}) : arrêt sans écran.", file=sys.stderr)
        message = execute(action)
        if message:
            report("error")
            print(message, file=sys.stderr)
            return 1
        report("done")
        return 0
    return ui.run()


def main(argv):
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    command, rest = argv[1], argv[2:]
    try:
        if command == "serve":
            return serve()
        if command == "execute" and len(rest) == 1:
            message = execute(rest[0])
            if message:
                print(message, file=sys.stderr)
                return 1
            return 0
        if command in ("ask", "now") and len(rest) == 1:
            if normalize(rest[0]) == "restart-session":
                raise ValueError(rest[0])
            return gui(rest[0], confirm=command == "ask")
        if command == "screen" and len(rest) == 1:
            normalize(rest[0])
            # Détaché : l'appelant (winlux-control) continue pendant l'affichage.
            if os.fork() > 0:
                return 0
            os.setsid()
            signal.signal(signal.SIGHUP, signal.SIG_IGN)
            return gui(rest[0], confirm=False, screen_only=True)
    except ValueError as error:
        print(f"Action inconnue : {error}", file=sys.stderr)
        return 2
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
