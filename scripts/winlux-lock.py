#!/usr/bin/python3
"""Écran de verrouillage Winlux, au style de Windows 11.

Usage :
  winlux-lock now       verrouille tout de suite (Windows+L, menu Démarrer)
  winlux-lock switch    « Changer d'utilisateur » : verrouille et propose les
                        autres comptes (en bas à gauche, comme Windows 11)
  winlux-lock daemon    verrouille après l'inactivité réglée et au retour de veille
  winlux-lock status    état lisible par winlux-control (clé=valeur)

Réglages : /etc/winlux/lock.conf
  LOCK_AFTER=300      secondes d'inactivité avant verrouillage (0 : jamais)
  LOCK_ON_RESUME=1    demander le mot de passe à la sortie de veille

Le mot de passe est celui du compte local : haché dans /etc/shadow (vérifié
avec la fonction crypt() de la libc), ou, en session Live, celui du fichier
/etc/winlux/users.conf. Un compte sans mot de passe déverrouille d'un clic,
et ne se verrouille alors jamais tout seul.
"""
import ctypes
import ctypes.util
import hmac
import os
import signal
import subprocess
import sys
import time
from pathlib import Path


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
    'Autre compte : {name}': ('Other account: {name}', 'Otra cuenta: {name}', 'Anderes Konto: {name}'),
    'Impossible d’ouvrir la session de {name}.': ('Couldn’t open {name}’s session.', 'No se pudo abrir la sesión de {name}.', 'Die Sitzung von {name} konnte nicht geöffnet werden.'),
    "Mot de passe": ("Password", "Contraseña", "Kennwort"),
    "Se connecter": ("Sign in", "Iniciar sesión", "Anmelden"),
    "Le mot de passe est incorrect. Réessayez.": (
        "The password is incorrect. Try again.",
        "La contraseña no es correcta. Inténtelo de nuevo.",
        "Das Kennwort ist falsch. Versuchen Sie es erneut."),
    "Trop de tentatives. Réessayez dans {s} secondes.": (
        "Too many attempts. Try again in {s} seconds.",
        "Demasiados intentos. Vuelva a intentarlo en {s} segundos.",
        "Zu viele Versuche. Versuchen Sie es in {s} Sekunden erneut."),
    "Verr. Maj activé": ("Caps Lock is on", "Bloq Mayús activado", "Feststelltaste ist aktiviert"),
    "Afficher le mot de passe": ("Show password", "Mostrar contraseña", "Kennwort anzeigen"),
    "Appuyez sur une touche ou cliquez pour vous connecter": (
        "Press any key or click to sign in",
        "Pulse una tecla o haga clic para iniciar sesión",
        "Drücken Sie eine Taste oder klicken Sie, um sich anzumelden"),
}

_DAYS = {
    "fr": ("lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"),
    "en": ("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"),
    "es": ("lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"),
    "de": ("Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"),
}
_MONTHS = {
    "fr": ("janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août",
           "septembre", "octobre", "novembre", "décembre"),
    "en": ("January", "February", "March", "April", "May", "June", "July", "August",
           "September", "October", "November", "December"),
    "es": ("enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto",
           "septiembre", "octubre", "noviembre", "diciembre"),
    "de": ("Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August",
           "September", "Oktober", "November", "Dezember"),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def long_date(when, lang=None):
    """« vendredi 31 octobre », « Friday, October 31 », « Freitag, 31. Oktober »."""
    lang = lang or _LANG
    day, month = _DAYS[lang][when.tm_wday], _MONTHS[lang][when.tm_mon - 1]
    if lang == "en":
        return f"{day}, {month} {when.tm_mday}"
    if lang == "de":
        return f"{day}, {when.tm_mday}. {month}"
    if lang == "es":
        return f"{day}, {when.tm_mday} de {month}"
    return f"{day} {when.tm_mday} {month}"


# ── Réglages et état ────────────────────────────────────────────────────────

ROOT = os.environ.get("WINLUX_LOCK_ROOT", "")
CHOICES = (0, 60, 180, 300, 600, 900, 1800, 3600)
MAX_TRIES = 5
WAIT_AFTER_TRIES = 30


def path(name):
    return Path(ROOT + name)


def read_conf():
    conf = {"LOCK_AFTER": "300", "LOCK_ON_RESUME": "1"}
    try:
        for line in path("/etc/winlux/lock.conf").read_text(errors="replace").splitlines():
            key, sep, value = line.partition("=")
            if sep and key in conf:
                conf[key] = value.strip()
    except OSError:
        pass
    return conf


def lock_after(conf=None):
    try:
        value = int((conf or read_conf())["LOCK_AFTER"])
    except ValueError:
        return 300
    return max(0, value)


def lock_on_resume(conf=None):
    return (conf or read_conf())["LOCK_ON_RESUME"] != "0"


def active_file():
    return path("/run/winlux/lock/active")


def shown_file():
    return path("/run/winlux/lock/shown")


def is_locked():
    try:
        pid = int(active_file().read_text().strip())
    except (OSError, ValueError):
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        pass
    return True


# ── Compte et mot de passe ──────────────────────────────────────────────────

def _passwd():
    entries = []
    try:
        for line in path("/etc/passwd").read_text(errors="replace").splitlines():
            parts = line.split(":")
            if len(parts) >= 7 and parts[2].isdigit():
                entries.append((parts[0], int(parts[2]), parts[5], parts[6]))
    except OSError:
        pass
    return entries


def session_account():
    """Compte dont la session est verrouillée.

    La session Winlux tourne encore en administrateur : on retient alors le
    compte personnel créé à l'installation (premier UID ≥ 1000 avec un shell),
    sinon root (session Live)."""
    chosen = os.environ.get("WINLUX_SESSION_USER", "").strip()
    if chosen:
        return chosen
    if os.geteuid() != 0:
        for name, uid, _home, _shell in _passwd():
            if uid == os.geteuid():
                return name
    for name, uid, _home, shell in _passwd():
        if 1000 <= uid < 60000 and not shell.endswith(("nologin", "false")):
            return name
    return "root"


def display_name(user):
    """Nom affiché d'un compte : le nom complet (champ GECOS de /etc/passwd,
    rempli par Paramètres > Comptes et l'installateur), sinon l'identifiant."""
    try:
        for line in path("/etc/passwd").read_text(errors="replace").splitlines():
            parts = line.split(":")
            if len(parts) >= 7 and parts[0] == user:
                full = parts[4].split(",")[0].strip()
                return full or user
    except OSError:
        pass
    return user


def account_home(user):
    for name, _uid, home, _shell in _passwd():
        if name == user:
            return home
    return "/root" if user == "root" else ""


def shadow_hash(user):
    """Empreinte de /etc/shadow, None si le fichier ou le compte manque."""
    try:
        text = path("/etc/shadow").read_text(errors="replace")
    except OSError:
        return None
    for line in text.splitlines():
        parts = line.split(":")
        if len(parts) >= 2 and parts[0] == user:
            return parts[1]
    return None


def users_conf_password(user):
    """Mot de passe Winlux Live (« nom:rôle:mot_de_passe|nopass »)."""
    try:
        text = path("/etc/winlux/users.conf").read_text(errors="replace")
    except OSError:
        return None
    for line in text.splitlines():
        parts = line.split(":", 2)
        if len(parts) == 3 and parts[0] == user:
            return parts[2]
    return None


def _crypt(password, setting):
    name = ctypes.util.find_library("crypt") or "libcrypt.so.1"
    lib = ctypes.CDLL(name)
    lib.crypt.restype = ctypes.c_char_p
    lib.crypt.argtypes = (ctypes.c_char_p, ctypes.c_char_p)
    out = lib.crypt(password.encode(), setting.encode())
    return out.decode(errors="replace") if out else ""


RELAY = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")


def relay_check(user, password, op="check"):
    """Mot de passe vérifié par le relais root (le bureau ne lit pas /etc/shadow)."""
    import json
    import socket
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(30)
            client.connect(str(path(RELAY)))
            client.sendall((json.dumps({"op": op, "user": user, "password": password}) + "\n").encode())
            answer = client.makefile("r", encoding="utf-8").readline()
        return bool(json.loads(answer or "{}").get("ok"))
    except (OSError, ValueError):
        return None


def relay_switch(user, password):
    """Ouvre ou rejoint la session d'un autre compte après vérification de son
    mot de passe par le relais root (winlux-sessions switch). Vrai si fait."""
    import json
    import socket
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(60)
            client.connect(str(path(RELAY)))
            client.sendall((json.dumps({"op": "switch", "user": user, "password": password}) + "\n").encode())
            answer = client.makefile("r", encoding="utf-8").readline()
        return bool(json.loads(answer or "{}").get("ok"))
    except (OSError, ValueError):
        return False


def other_accounts(user):
    """Comptes de l'écran de connexion (users.conf) autres que celui-ci."""
    names = []
    try:
        for line in path("/etc/winlux/users.conf").read_text(errors="replace").splitlines():
            name = line.split(":", 1)[0]
            if name and name != user and name not in names:
                names.append(name)
    except OSError:
        pass
    # root n'apparaît que s'il est le seul autre compte (session Live).
    return [n for n in names if n != "root"] or names


def requires_password(user):
    stored = shadow_hash(user)
    if stored and stored.startswith("$"):
        return True
    live = users_conf_password(user)
    if live == "shadow":
        # Compte du système installé : son empreinte est dans /etc/shadow,
        # que seul le relais root lit. Vide seulement si root le dit.
        return stored is None or stored.startswith(("$", "!", "*"))
    if live is not None:
        return live != "nopass"
    # Sans empreinte vérifiable, ne jamais enfermer l'utilisateur dehors.
    return False


def check_password(user, password):
    stored = shadow_hash(user)
    if stored and stored.startswith("$"):
        try:
            return hmac.compare_digest(_crypt(password, stored), stored)
        except OSError:
            return False
    live = users_conf_password(user)
    if live == "shadow" or (stored is None and os.geteuid() != 0):
        if stored is not None and os.geteuid() == 0:
            return stored == "" and password == ""
        answer = relay_check(user, password)
        if answer is not None:
            return answer
        if live == "shadow":
            return False
    if live is not None:
        return live == "nopass" or hmac.compare_digest(live.encode(), password.encode())
    return not requires_password(user)


# ── Inactivité X11 et retour de veille ──────────────────────────────────────

class _XssInfo(ctypes.Structure):
    _fields_ = [("window", ctypes.c_ulong), ("state", ctypes.c_int), ("kind", ctypes.c_int),
                ("til_or_since", ctypes.c_ulong), ("idle", ctypes.c_ulong),
                ("eventMask", ctypes.c_ulong)]


class XIdle:
    """Secondes depuis la dernière entrée clavier/souris (extension MIT-SCREEN-SAVER)."""

    def __init__(self):
        self.display = None
        try:
            self.x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
            self.xss = ctypes.CDLL(ctypes.util.find_library("Xss") or "libXss.so.1")
        except OSError:
            self.x11 = self.xss = None
            return
        self.x11.XOpenDisplay.restype = ctypes.c_void_p
        self.x11.XOpenDisplay.argtypes = (ctypes.c_char_p,)
        self.x11.XDefaultRootWindow.restype = ctypes.c_ulong
        self.x11.XDefaultRootWindow.argtypes = (ctypes.c_void_p,)
        self.xss.XScreenSaverAllocInfo.restype = ctypes.POINTER(_XssInfo)
        self.xss.XScreenSaverQueryInfo.argtypes = (ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(_XssInfo))

    def seconds(self):
        if self.xss is None:
            return None
        if self.display is None:
            self.display = self.x11.XOpenDisplay(None)
            if not self.display:
                self.display = None
                return None
            self.info = self.xss.XScreenSaverAllocInfo()
        root = self.x11.XDefaultRootWindow(self.display)
        if not self.xss.XScreenSaverQueryInfo(self.display, root, self.info):
            return None
        return self.info.contents.idle // 1000


def suspended_seconds():
    """Temps total passé en veille depuis le démarrage (BOOTTIME - MONOTONIC)."""
    return time.clock_gettime(time.CLOCK_BOOTTIME) - time.clock_gettime(time.CLOCK_MONOTONIC)


def should_lock(idle, after, slept, on_resume, password):
    """Décision du service : (verrouiller, raison)."""
    if not password:
        return False, ""
    if on_resume and slept >= 3:
        return True, "resume"
    if after and idle is not None and idle >= after:
        return True, "idle"
    return False, ""


def spawn_lock():
    subprocess.Popen([sys.executable, os.path.abspath(__file__), "now"], start_new_session=True,
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def daemon():
    idle_source = XIdle()
    slept_before = suspended_seconds()
    while True:
        time.sleep(2)
        slept_now = suspended_seconds()
        slept, slept_before = slept_now - slept_before, slept_now
        if is_locked():
            continue
        conf = read_conf()
        lock, _reason = should_lock(idle_source.seconds(), lock_after(conf), slept,
                                    lock_on_resume(conf), requires_password(session_account()))
        if lock:
            spawn_lock()
            # Laisser l'écran apparaître avant de mesurer à nouveau.
            time.sleep(3)


def status():
    conf = read_conf()
    user = session_account()
    print(f"lock_after={lock_after(conf)}")
    print(f"lock_on_resume={1 if lock_on_resume(conf) else 0}")
    print(f"lock_account={user}")
    print(f"lock_password={1 if requires_password(user) else 0}")
    print(f"lock_active={1 if is_locked() else 0}")


# ── Interface ───────────────────────────────────────────────────────────────

def wallpaper_file():
    try:
        out = subprocess.run(["xfconf-query", "-c", "xfce4-desktop", "-l", "-v"], capture_output=True,
                             text=True, timeout=2).stdout
        for line in out.splitlines():
            key, _, value = line.partition(" ")
            if key.endswith("/last-image") and Path(value.strip()).is_file():
                return value.strip()
    except (OSError, subprocess.SubprocessError):
        pass
    for candidate in ("/usr/share/winlux/wallpaper.bmp", "/usr/share/winlux/wallpaper.png"):
        if path(candidate).is_file():
            return str(path(candidate))
    return None


def _channel(value):
    value /= 255
    return value / 12.92 if value <= 0.03928 else ((value + 0.055) / 1.055) ** 2.4


def text_veil(pixbuf, base, minimum=4.5):
    """Opacité du voile noir posé sur le fond d'écran pour que le texte blanc
    atteigne minimum:1 sur 95 % de l'image (« Winlux Aube » ou une photo claire
    laissaient l'heure et la date blanches à moins de 2:1 sous un voile de 18 %)."""
    if pixbuf is None:
        return base
    from gi.repository import GdkPixbuf
    small = pixbuf.scale_simple(64, 36, GdkPixbuf.InterpType.BILINEAR)
    data, stride, channels = small.get_pixels(), small.get_rowstride(), small.get_n_channels()
    pixels = [tuple(data[y * stride + x * channels + c] for c in range(3)) for y in range(36) for x in range(64)]
    limit = 1.05 / minimum - 0.05  # luminance maximale du fond sous du blanc

    def bright_share(alpha):
        darker = [0.2126 * _channel(r * (1 - alpha)) + 0.7152 * _channel(g * (1 - alpha))
                  + 0.0722 * _channel(b * (1 - alpha)) for r, g, b in pixels]
        return sum(value > limit for value in darker) / len(darker)
    alpha = base
    while alpha < 0.8 and bright_share(alpha) > 0.05:
        alpha += 0.02
    return round(alpha, 2)


def run_lock(start="clock"):
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GdkPixbuf, GLib, Gtk

    css = b"""
      .lock-time { color: white; font-size: 112px; font-weight: 600; text-shadow: 0 2px 6px rgba(0,0,0,0.55); }
      .lock-date { color: white; font-size: 26px; text-shadow: 0 1px 4px rgba(0,0,0,0.6); }
      .lock-hint { color: white; font-size: 13px; text-shadow: 0 1px 3px rgba(0,0,0,0.7); }
      .lock-name { color: white; font-size: 26px; font-weight: 600; text-shadow: 0 1px 4px rgba(0,0,0,0.6); }
      .lock-error { color: white; font-size: 14px; text-shadow: 0 1px 3px rgba(0,0,0,0.7); }
      .lock-caps { color: white; font-size: 12px; text-shadow: 0 1px 3px rgba(0,0,0,0.7); }
      entry.lock-password { background: rgba(255,255,255,0.88); color: #1b1b1b; border: none;
                            border-bottom: 2px solid #3b6ee8; border-radius: 4px; padding: 8px 10px;
                            min-width: 260px; font-size: 15px; caret-color: #1b1b1b; }
      button.lock-go, button.lock-signin { background: rgba(255,255,255,0.20); color: white; border: none;
                            border-radius: 4px; padding: 8px 14px; box-shadow: none; text-shadow: none; }
      button.lock-go:hover, button.lock-signin:hover { background: rgba(255,255,255,0.32); }
    """

    class Lock:
        def __init__(self, start="clock"):
            self.user = session_account()
            self.owner = self.user  # compte de la session verrouillée
            self.password = requires_password(self.user)
            self.failures = 0
            self.blocked_until = 0
            self.view = "clock"
            self.last_typing = time.monotonic()
            provider = Gtk.CssProvider()
            provider.load_from_data(css)
            Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider, 900)
            self.sharp = self.blurred = None
            source = wallpaper_file()
            if source:
                try:
                    self.sharp = GdkPixbuf.Pixbuf.new_from_file(source)
                    # Flou « acrylique » : réduction puis agrandissements successifs.
                    width, height = self.sharp.get_width(), self.sharp.get_height()
                    image = self.sharp
                    for factor in (4, 12, 32):
                        image = image.scale_simple(max(1, width // factor), max(1, height // factor),
                                                   GdkPixbuf.InterpType.BILINEAR)
                    for factor in (16, 6, 2, 1):
                        image = image.scale_simple(max(1, width // factor), max(1, height // factor),
                                                   GdkPixbuf.InterpType.BILINEAR)
                    self.blurred = image
                except GLib.Error:
                    self.sharp = self.blurred = None
            # Voile assez sombre pour que le texte blanc reste lisible sur un fond clair.
            self.veil = {"clock": text_veil(self.sharp, 0.18), "signin": text_veil(self.blurred, 0.40)}
            display = Gdk.Display.get_default()
            primary = display.get_primary_monitor() or display.get_monitor(0)
            self.windows = []
            for index in range(display.get_n_monitors()):
                monitor = display.get_monitor(index)
                self.windows.append(self.make_window(monitor.get_geometry(), monitor == primary))
            self.grab_tries = 0
            GLib.timeout_add(100, self.grab)
            GLib.timeout_add(1000, self.tick)
            self.tick()
            if start == "signin":
                self.show_signin()

        def make_window(self, geo, main):
            window = Gtk.Window(type=Gtk.WindowType.POPUP)
            window.set_app_paintable(True)
            window.move(geo.x, geo.y)
            window.set_default_size(geo.width, geo.height)
            window.set_size_request(geo.width, geo.height)
            overlay = Gtk.Overlay()
            window.add(overlay)
            area = Gtk.DrawingArea()
            area.connect("draw", self.draw_background)
            overlay.add(area)
            if main:
                self.main = window
                self.area = area
                overlay.add_overlay(self.clock_view())
                overlay.add_overlay(self.signin_view())
                overlay.add_overlay(self.accounts_view())
                window.add_events(Gdk.EventMask.KEY_PRESS_MASK | Gdk.EventMask.BUTTON_PRESS_MASK)
                window.connect("key-press-event", self.on_key)
                window.connect("button-press-event", self.on_click)
            window.show_all()
            if main:
                self.signin.hide()
                self.accounts.hide()
                self.select_account(self.user)
            return window

        def accounts_view(self):
            """Autres comptes en bas à gauche : un clic change de compte."""
            self.accounts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4, halign=Gtk.Align.START,
                                    valign=Gtk.Align.END, margin_start=28, margin_bottom=28)
            self.account_names = [self.owner] + other_accounts(self.owner)
            if len(self.account_names) > 1:
                for name in self.account_names:
                    button = Gtk.Button(label=display_name(name))
                    button.get_style_context().add_class("lock-signin")
                    button.get_accessible().set_name(_t("Autre compte : {name}").format(name=display_name(name)))
                    button.connect("clicked", lambda _b, n=name: self.select_account(n))
                    self.accounts.pack_start(button, False, False, 0)
            return self.accounts

        def select_account(self, name):
            """Compte affiché au centre : celui de la session ou un autre."""
            self.user = name
            self.password = requires_password(name)
            self.name_label.set_text(display_name(name))
            self.face = None
            home = account_home(name)
            if home and Path(home, ".face").is_file():
                try:
                    self.face = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(Path(home, ".face")), 176, 176, False)
                except GLib.Error:
                    self.face = None
            self.avatar.queue_draw()
            self.password_entry.set_text("")
            self.error.set_text("")
            self.entry = self.password_entry if self.password else None
            if self.view == "signin":
                self.password_row.set_visible(self.password)
                self.signin_button.set_visible(not self.password)
                if self.entry is not None:
                    self.entry.grab_focus()

        def clock_view(self):
            self.clock = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, halign=Gtk.Align.CENTER,
                                 valign=Gtk.Align.START, margin_top=110)
            self.time_label = Gtk.Label()
            self.time_label.get_style_context().add_class("lock-time")
            self.date_label = Gtk.Label()
            self.date_label.get_style_context().add_class("lock-date")
            self.clock.pack_start(self.time_label, False, False, 0)
            self.clock.pack_start(self.date_label, False, False, 0)
            hint = Gtk.Label(label=_t("Appuyez sur une touche ou cliquez pour vous connecter"), margin_top=24)
            hint.get_style_context().add_class("lock-hint")
            self.clock.pack_start(hint, False, False, 0)
            return self.clock

        def signin_view(self):
            self.signin = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14,
                                  halign=Gtk.Align.CENTER, valign=Gtk.Align.CENTER)
            avatar = Gtk.DrawingArea()
            avatar.set_size_request(176, 176)
            avatar.set_halign(Gtk.Align.CENTER)
            self.avatar = avatar
            self.face = None
            home = account_home(self.user)
            if home and Path(home, ".face").is_file():
                try:
                    self.face = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(Path(home, ".face")), 176, 176, False)
                except GLib.Error:
                    self.face = None
            avatar.connect("draw", self.draw_avatar)
            self.signin.pack_start(avatar, False, False, 0)
            name = Gtk.Label(label=display_name(self.user))
            name.get_style_context().add_class("lock-name")
            self.name_label = name
            self.signin.pack_start(name, False, False, 6)
            # Champ et bouton construits tous deux : le compte choisi en bas à
            # gauche peut avoir un mot de passe ou non.
            row = Gtk.Box(spacing=0, halign=Gtk.Align.CENTER)
            self.password_entry = Gtk.Entry(visibility=False, placeholder_text=_t("Mot de passe"))
            self.password_entry.get_style_context().add_class("lock-password")
            self.password_entry.set_icon_from_icon_name(Gtk.EntryIconPosition.SECONDARY, "view-reveal-symbolic")
            self.password_entry.set_icon_tooltip_text(Gtk.EntryIconPosition.SECONDARY, _t("Afficher le mot de passe"))
            self.password_entry.connect("icon-press", lambda e, *_: e.set_visibility(not e.get_visibility()))
            self.password_entry.connect("activate", lambda *_: self.submit())
            self.password_entry.connect("changed", self.on_typing)
            go = Gtk.Button.new_from_icon_name("go-next-symbolic", Gtk.IconSize.BUTTON)
            go.get_style_context().add_class("lock-go")
            go.connect("clicked", lambda *_: self.submit())
            row.pack_start(self.password_entry, False, False, 0)
            row.pack_start(go, False, False, 4)
            row.set_no_show_all(True)
            self.password_row = row
            self.signin.pack_start(row, False, False, 0)
            button = Gtk.Button(label=_t("Se connecter"), halign=Gtk.Align.CENTER)
            button.get_style_context().add_class("lock-signin")
            button.connect("clicked", lambda *_: self.sign_in())
            button.set_no_show_all(True)
            self.signin_button = button
            self.signin.pack_start(button, False, False, 0)
            for widget in (row, go, self.password_entry):
                widget.show()
            self.entry = self.password_entry if self.password else None
            self.error = Gtk.Label(wrap=True, max_width_chars=44, justify=Gtk.Justification.CENTER)
            self.error.get_style_context().add_class("lock-error")
            self.signin.pack_start(self.error, False, False, 0)
            self.caps = Gtk.Label()
            self.caps.get_style_context().add_class("lock-caps")
            self.signin.pack_start(self.caps, False, False, 0)
            return self.signin

        def draw_background(self, widget, cr):
            width, height = widget.get_allocated_width(), widget.get_allocated_height()
            image = self.blurred if self.view == "signin" else self.sharp
            if image is None:
                cr.set_source_rgb(0.06, 0.12, 0.27)
                cr.paint()
            else:
                scale = max(width / image.get_width(), height / image.get_height())
                cr.save()
                cr.translate((width - image.get_width() * scale) / 2, (height - image.get_height() * scale) / 2)
                cr.scale(scale, scale)
                Gdk.cairo_set_source_pixbuf(cr, image, 0, 0)
                cr.paint()
                cr.restore()
            cr.set_source_rgba(0, 0, 0, self.veil["signin" if self.view == "signin" else "clock"])
            cr.paint()
            return False

        def draw_avatar(self, widget, cr):
            size = min(widget.get_allocated_width(), widget.get_allocated_height())
            cr.arc(size / 2, size / 2, size / 2, 0, 6.2832)
            cr.clip()
            if self.face is not None:
                Gdk.cairo_set_source_pixbuf(cr, self.face, 0, 0)
                cr.paint()
                return False
            cr.set_source_rgb(0.30, 0.33, 0.40)
            cr.paint()
            cr.set_source_rgb(1, 1, 1)
            cr.select_font_face("Sans")
            cr.set_font_size(size * 0.42)
            letter = (display_name(self.user)[:1] or "?").upper()
            ext = cr.text_extents(letter)
            cr.move_to(size / 2 - ext.width / 2 - ext.x_bearing, size / 2 - ext.height / 2 - ext.y_bearing)
            cr.show_text(letter)
            return False

        def tick(self):
            now = time.localtime()
            self.time_label.set_text(time.strftime("%H:%M", now) if _LANG != "en" else time.strftime("%I:%M", now).lstrip("0"))
            self.date_label.set_text(long_date(now))
            if self.view == "signin":
                waiting = int(self.blocked_until - time.monotonic())
                if waiting > 0:
                    self.error.set_text(_t("Trop de tentatives. Réessayez dans {s} secondes.").format(s=waiting))
                elif self.failures >= MAX_TRIES:
                    self.failures = 0
                    self.error.set_text("")
                    if self.entry is not None:
                        self.entry.set_sensitive(True)
                        self.entry.grab_focus()
                keymap = Gdk.Keymap.get_for_display(Gdk.Display.get_default())
                self.caps.set_text(_t("Verr. Maj activé") if keymap.get_caps_lock_state() else "")
                empty = self.entry is None or not self.entry.get_text()
                if empty and time.monotonic() - self.last_typing > 30:
                    self.show_clock()
            return True

        def grab(self):
            gdk_window = self.main.get_window()
            if gdk_window is not None:
                seat = Gdk.Display.get_default().get_default_seat()
                status = seat.grab(gdk_window, Gdk.SeatCapabilities.ALL, True, None, None, None)
                if status == Gdk.GrabStatus.SUCCESS:
                    # Signal pour la mise en veille : l'écran est en place.
                    try:
                        shown_file().write_text(f"{os.getpid()}\n")
                    except OSError:
                        pass
                    return False
            self.grab_tries += 1
            # Un menu ouvert peut détenir le clavier : on réessaie sans fin.
            return True

        def show_signin(self):
            if self.view == "signin":
                return
            self.view = "signin"
            self.last_typing = time.monotonic()
            self.clock.hide()
            self.signin.show()
            self.password_row.set_visible(self.password)
            self.signin_button.set_visible(not self.password)
            if len(self.account_names) > 1:
                self.accounts.show_all()
            self.area.queue_draw()
            if self.entry is not None:
                self.entry.grab_focus()
            self.tick()

        def show_clock(self):
            self.view = "clock"
            self.signin.hide()
            self.accounts.hide()
            if self.user != self.owner:
                self.select_account(self.owner)
            self.clock.show()
            self.error.set_text("")
            self.area.queue_draw()

        def on_typing(self, *_):
            self.last_typing = time.monotonic()
            if self.error.get_text() and self.failures < MAX_TRIES:
                self.error.set_text("")

        def on_click(self, *_):
            if self.view == "clock":
                self.show_signin()
            return False

        def on_key(self, _widget, event):
            self.last_typing = time.monotonic()
            if self.view == "clock":
                self.show_signin()
                if self.entry is not None and event.string and event.string.isprintable():
                    self.entry.set_text(event.string)
                    self.entry.set_position(-1)
                return True
            if event.keyval == Gdk.KEY_Escape:
                if self.entry is not None and self.entry.get_text():
                    self.entry.set_text("")
                else:
                    self.show_clock()
                return True
            if self.entry is None and event.keyval in (Gdk.KEY_Return, Gdk.KEY_KP_Enter):
                self.sign_in()
                return True
            return False

        def submit(self):
            if self.entry is None or time.monotonic() < self.blocked_until:
                return
            if self.user != self.owner:
                # Autre compte : le relais root vérifie et bascule de session ;
                # celle-ci reste verrouillée derrière.
                if relay_switch(self.user, self.entry.get_text()):
                    self.failures = 0
                    self.show_clock()
                    return
            elif check_password(self.user, self.entry.get_text()):
                self.unlock()
                return
            self.failures += 1
            self.entry.set_text("")
            if self.failures >= MAX_TRIES:
                self.blocked_until = time.monotonic() + WAIT_AFTER_TRIES
                self.entry.set_sensitive(False)
                self.tick()
            else:
                self.error.set_text(_t("Le mot de passe est incorrect. Réessayez."))

        def sign_in(self):
            """Compte sans mot de passe : sa session, ou celle d'un autre."""
            if self.user == self.owner:
                self.unlock()
            elif relay_switch(self.user, ""):
                self.show_clock()
            else:
                self.error.set_text(_t("Impossible d’ouvrir la session de {name}.").format(name=self.user))

        def unlock(self):
            Gdk.Display.get_default().get_default_seat().ungrab()
            Gtk.main_quit()

    Lock(start)
    Gtk.main()


def claim_marker(marker):
    """Crée le marqueur de verrouillage de façon atomique (O_EXCL).

    Deux « winlux-lock now » simultanés (raccourci et démon) ne doivent ouvrir
    qu'un seul écran : seul celui qui crée le fichier continue. Un marqueur
    laissé par un processus disparu est remplacé.
    """
    for _attempt in range(2):
        try:
            marker.parent.mkdir(parents=True, exist_ok=True)
            fd = os.open(marker, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        except FileExistsError:
            if is_locked():
                return False
            try:
                marker.unlink()
            except OSError:
                pass
            continue
        except OSError:
            # /run indisponible (tests) : on verrouille sans marqueur.
            return True
        with os.fdopen(fd, "w") as handle:
            handle.write(f"{os.getpid()}\n")
        return True
    return False


def lock_now(start="clock"):
    marker = active_file()
    if not claim_marker(marker):
        return 0
    # Un signal ordinaire ne doit pas lever le verrouillage.
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP, signal.SIGQUIT):
        signal.signal(sig, signal.SIG_IGN)
    try:
        run_lock(start)
    finally:
        for done in (shown_file(), marker):
            try:
                done.unlink()
            except OSError:
                pass
    return 0


def main(argv):
    command = argv[1] if len(argv) > 1 else "now"
    if command == "now":
        return lock_now()
    if command == "switch":
        return lock_now("signin")
    if command == "daemon":
        daemon()
        return 0
    if command == "status":
        status()
        return 0
    print(__doc__.strip(), file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
