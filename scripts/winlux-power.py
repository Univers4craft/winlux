#!/usr/bin/python3
"""Arrêt et redémarrage de Winlux, au style de Windows 11.

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
    "Impossible d’arrêter le PC": ("Couldn’t shut down the PC", "No se pudo apagar el equipo", "PC konnte nicht heruntergefahren werden"),
    "Impossible de redémarrer le PC": ("Couldn’t restart the PC", "No se pudo reiniciar el equipo", "PC konnte nicht neu gestartet werden"),
    "Une mise à jour est en cours d’installation. Réessayez quand elle sera terminée.": (
        "An update is being installed. Try again when it has finished.",
        "Se está instalando una actualización. Inténtelo de nuevo cuando termine.",
        "Ein Update wird installiert. Versuchen Sie es nach Abschluss erneut."),
    "Le service d’alimentation ne répond pas.": ("The power service isn’t responding.", "El servicio de energía no responde.",
                                                  "Der Energiedienst antwortet nicht."),
    "OK": ("OK", "Aceptar", "OK"),
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
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(SOCKET)
    os.chmod(SOCKET, 0o666)
    server.listen(4)
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
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name


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
        if self.mode == "confirm":
            self.show_confirm()
        else:
            self.start_shutdown()
        self.window.show_all()
        self.GLib.idle_add(self.grab)

    # Saisie exclusive : la fenêtre POPUP n'a pas de gestionnaire de fenêtres.
    def grab(self):
        gdk_window = self.window.get_window()
        if gdk_window is None:
            return False
        seat = self.window.get_display().get_default_seat()
        for _ in range(20):
            if seat.grab(gdk_window, self.Gdk.SeatCapabilities.ALL, True, None, None, None, None) == self.Gdk.GrabStatus.SUCCESS:
                break
            time.sleep(0.05)
        return False

    def ungrab(self):
        self.window.get_display().get_default_seat().ungrab()

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
        listing = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        seen = set()
        for window_class, title, _pid in blockers.values():
            if title in seen:
                continue
            seen.add(title)
            row = Gtk.Box(spacing=10)
            icon = window_class.split(".")[0].casefold()
            row.pack_start(Gtk.Image.new_from_icon_name(icon if Gtk.IconTheme.get_default().has_icon(icon)
                                                        else "application-x-executable", Gtk.IconSize.LARGE_TOOLBAR),
                           False, False, 0)
            name = Gtk.Label(label=title, xalign=0, ellipsize=3, max_width_chars=40)
            name.get_style_context().add_class("app")
            row.pack_start(name, True, True, 0)
            listing.pack_start(row, False, False, 0)
            if len(seen) >= 6:
                break
        self.set_card(self.make_card(
            "dialog-warning-symbolic",
            _t("Ces applications empêchent l’arrêt") if shutdown else _t("Ces applications empêchent le redémarrage"),
            _t("Enregistrez votre travail dans ces applications, ou continuez : les modifications non enregistrées seront perdues."),
            [(_t("Arrêter quand même") if shutdown else _t("Redémarrer quand même"), self.finish, True),
             (_t("Annuler"), self.cancel, False)],
            extra=listing))

    def show_error(self, message):
        self.mode = "error"
        self.set_card(self.make_card(
            "dialog-error-symbolic",
            _t("Impossible d’arrêter le PC") if self.action == "poweroff" else _t("Impossible de redémarrer le PC"),
            _t(message), [(_t("OK"), self.cancel, True)]))

    # ── Déroulement ──
    def cancel(self):
        self.result = 1
        self.ungrab()
        self.window.destroy()

    def start_shutdown(self):
        self.mode = "progress"
        self.started = time.monotonic()
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
        error = execute(self.action)
        if error:
            self.show_error(error)
            return
        self.result = 0
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
    try:
        ui = PowerUI(action, confirm=confirm, screen_only=screen_only)
    except Exception as error:  # pas d'affichage, GTK absent…
        if screen_only:
            return 0
        print(f"Interface indisponible ({error}) : arrêt sans écran.", file=sys.stderr)
        message = execute(action)
        if message:
            print(message, file=sys.stderr)
            return 1
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
