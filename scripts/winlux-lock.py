#!/usr/bin/python3
"""Écran de verrouillage Winlux.

Usage :
  winlux-lock now       verrouille tout de suite (Windows+L, menu Démarrer)
  winlux-lock switch    « Changer d'utilisateur » : verrouille et propose les
                        autres comptes (en bas à gauche)
  winlux-lock daemon    verrouille après l'inactivité réglée et au retour de veille
  winlux-lock status    état lisible par winlux-control (clé=valeur)

Réglages : /etc/winlux/lock.conf
  LOCK_AFTER=300      secondes d'inactivité avant verrouillage (0 : jamais)
  LOCK_ON_RESUME=1    demander le mot de passe à la sortie de veille

Options de connexion (#659) : un compte qui a choisi un code PIN le tape à
la place du mot de passe (« Options de connexion » sous le champ), et un
doigt enregistré déverrouille dès qu'il est posé sur le lecteur, même sur
l'horloge. Le relais root vérifie les deux (winlux-signin).

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
    "Contrôle parental : ce compte ne peut pas être utilisé maintenant. Un adulte peut accorder du temps.": (
        "Parental controls: this account cannot be used right now. An adult can grant more time.",
        "Control parental: esta cuenta no se puede usar ahora. Un adulto puede conceder más tiempo.",
        "Jugendschutz: Dieses Konto kann gerade nicht verwendet werden. Ein Erwachsener kann mehr Zeit geben."),
    "Verr. Maj activé": ("Caps Lock is on", "Bloq Mayús activado", "Feststelltaste ist aktiviert"),
    "Clavier tactile": ("Touch keyboard", "Teclado táctil", "Bildschirmtastatur"),
    "Afficher le mot de passe": ("Show password", "Mostrar contraseña", "Kennwort anzeigen"),
    "Code PIN": ("PIN", "PIN", "PIN"),
    "Options de connexion": ("Sign-in options", "Opciones de inicio de sesión", "Anmeldeoptionen"),
    "Posez votre doigt sur le lecteur d’empreintes.": (
        "Place your finger on the fingerprint reader.",
        "Ponga el dedo en el lector de huellas.",
        "Legen Sie Ihren Finger auf den Fingerabdruckleser."),
    "Empreinte non reconnue. Réessayez.": (
        "Fingerprint not recognised. Try again.",
        "Huella no reconocida. Inténtelo de nuevo.",
        "Fingerabdruck nicht erkannt. Versuchen Sie es erneut."),
    "Utilisez votre code PIN ou votre mot de passe.": (
        "Use your PIN or password.",
        "Use su PIN o su contraseña.",
        "Verwenden Sie Ihre PIN oder Ihr Kennwort."),
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


def type_into(entry, key, submit):
    """Une touche du clavier tactile dans un champ GTK (écran de connexion)."""
    position = entry.get_position()
    if key == "BackSpace":
        if position > 0:
            entry.delete_text(position - 1, position)
    elif key == "Return":
        submit()
    elif key == "Escape":
        entry.set_text("")
    elif key in ("Left", "Right"):
        entry.set_position(max(0, position + (1 if key == "Right" else -1)))
    elif len(key) == 1:
        entry.insert_text(key, position)
        entry.set_position(position + 1)


def signin_module():
    """Options de connexion (/usr/bin/winlux-signin, ou le script voisin)."""
    import importlib.machinery
    import importlib.util
    for path_ in (Path("/usr/bin/winlux-signin"), Path(__file__).resolve().with_name("winlux-signin.py")):
        if path_.is_file():
            try:
                loader = importlib.machinery.SourceFileLoader("winlux_signin", str(path_))
                spec = importlib.util.spec_from_loader("winlux_signin", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


def osk_module():
    """Module du clavier tactile (/usr/bin/winlux-osk, ou le script voisin)."""
    import importlib.machinery
    import importlib.util
    for path in (Path("/usr/bin/winlux-osk"), Path(__file__).resolve().with_name("winlux-osk.py")):
        if path.is_file():
            try:
                loader = importlib.machinery.SourceFileLoader("winlux_osk", str(path))
                spec = importlib.util.spec_from_loader("winlux_osk", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


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


def _screen_suffix():
    """Marqueurs par écran : « Changer d'utilisateur » laisse le premier compte
    verrouillé sur :0 ; un marqueur commun empêchait le second compte (:1) de
    verrouiller ou d'ouvrir l'écran de connexion. :0 garde les noms d'origine
    (lus par le service d'alimentation avant la veille)."""
    display = os.environ.get("DISPLAY", ":0").rpartition(":")[2].split(".")[0]
    return "" if display in ("", "0") else "-" + "".join(c for c in display if c.isdigit())


def active_file():
    return path("/run/winlux/lock/active" + _screen_suffix())


def shown_file():
    return path("/run/winlux/lock/shown" + _screen_suffix())


def power_request_file():
    return path("/run/winlux/lock/power" + _screen_suffix())


def power_request_pid():
    """Écran d'arrêt (winlux-power) qui demande la saisie au-dessus du verrou
    (#2254) : pid vivant d'un processus winlux-power appartenant à root ou au
    compte verrouillé, fichier écrit par l'un des deux. Tout le reste (autre
    compte, pid mort, autre programme) est ignoré : le verrou garde la main."""
    request = power_request_file()
    allowed = {0, os.getuid()}
    try:
        if request.stat().st_uid not in allowed:
            return None
        pid = int(request.read_text().strip())
        proc = Path(f"/proc/{pid}")
        if proc.stat().st_uid not in allowed:
            return None
        cmdline = (proc / "cmdline").read_bytes().split(b"\0")
    except (OSError, ValueError):
        return None
    if not any(os.path.basename(arg.decode(errors="replace")) == "winlux-power" for arg in cmdline[:2]):
        return None
    return pid


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
# Dernier refus du contrôle parental (#661) : bon mot de passe, mais hors du
# temps permis pour ce compte.
PARENTAL_REFUSED = []


def relay_check(user, password, op="check"):
    """Mot de passe vérifié par le relais root (le bureau ne lit pas /etc/shadow)."""
    import json
    import socket
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(30)
            client.connect(str(path(RELAY)))
            client.sendall((json.dumps({"op": op, "user": user, "password": password}) + "\n").encode())
            answer = json.loads(client.makefile("r", encoding="utf-8").readline() or "{}")
        PARENTAL_REFUSED[:] = [answer["parental"]] if answer.get("parental") else []
        return bool(answer.get("ok"))
    except (OSError, ValueError):
        return None


def relay_switch(user, password, pin=None, answer_out=None):
    """Ouvre ou rejoint la session d'un autre compte après vérification de son
    mot de passe (ou de son code PIN) par le relais root (winlux-sessions
    switch). Vrai si fait ; la réponse complète va dans answer_out."""
    import json
    import socket
    request = {"op": "switch", "user": user, "password": password}
    if pin is not None:
        request = {"op": "switch", "user": user, "pin": pin}
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(60)
            client.connect(str(path(RELAY)))
            client.sendall((json.dumps(request) + "\n").encode())
            answer = json.loads(client.makefile("r", encoding="utf-8").readline() or "{}")
        PARENTAL_REFUSED[:] = [answer["parental"]] if answer.get("parental") else []
        if answer_out is not None and isinstance(answer, dict):
            answer_out.update(answer)
        return bool(answer.get("ok"))
    except (OSError, ValueError, AttributeError):
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


def open_keyring(password):
    """Trousseau de secrets du compte ouvert avec le mot de passe tout juste
    vérifié, comme pam_gnome_keyring ailleurs (#1488). Le mot de passe passe
    par un tube, jamais en argument ; le déverrouillage ne l'attend pas."""
    helper = os.environ.get("WINLUX_KEYRING", "/usr/bin/winlux-keyring")
    if not os.access(helper, os.X_OK):
        return
    try:
        child = subprocess.Popen([helper, "unlock"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL, start_new_session=True)
        child.stdin.write(password.encode())
        child.stdin.close()
    except OSError:
        pass


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


# Sommet du prisme du fond d'écran par défaut, en 1920x1080. #2160 : c'était
# 367 (fonds Aube et Nuit) ; le fond Aurore installé par défaut a son sommet
# vers la ligne 303, et l'invite passait sous le logo en 1920x1080.
LOGO_TOP_1080 = 300


def clock_scale(width, height):
    """Échelle de l'horloge pour qu'elle finisse au-dessus du prisme du fond
    d'écran Winlux (1920x1080, sommet à LOGO_TOP_1080, image mise à l'échelle
    pour couvrir l'écran). L'horloge mesure ~330 px à l'échelle 1."""
    cover = max(width / 1920, height / 1080)
    logo_top = (height - 1080 * cover) / 2 + LOGO_TOP_1080 * cover
    return max(0.5, min(1.0, (logo_top - 24) / 330))


def clock_css(scale):
    return (f".lock-time {{ font-size: {round(112 * scale)}px; }}\n"
            f".lock-date {{ font-size: {round(26 * scale)}px; }}\n"
            f".lock-hint {{ font-size: {max(11, round(13 * scale))}px; }}\n")


def cover_plan(covered, monitors):
    """Écrans à couvrir après un branchement ou un débranchement (#158).

    `covered` : géométries (x, y, largeur, hauteur) déjà couvertes par une
    fenêtre de l'écran verrouillé ; `monitors` : géométries des écrans
    présents. Rend (à ajouter, à retirer) : aucun écran ne doit montrer le
    bureau, et aucune fenêtre ne doit rester sur un écran disparu."""
    covered, monitors = list(covered), list(monitors)
    add = [geo for geo in monitors if geo not in covered]
    remove = [geo for geo in covered if geo not in monitors]
    return add, remove


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
      .lock-method-hint { color: white; font-size: 14px; margin-bottom: 4px; text-shadow: 0 1px 3px rgba(0,0,0,0.7); }
      .lock-caps { color: white; font-size: 12px; text-shadow: 0 1px 3px rgba(0,0,0,0.7); }
      entry.lock-password { background: rgba(255,255,255,0.88); color: #1b1b1b; border: none;
                            border-bottom: 2px solid #3b6ee8; border-radius: 4px; padding: 8px 10px;
                            min-width: 260px; font-size: 15px; caret-color: #1b1b1b; }
      button.lock-go, button.lock-signin { background: rgba(255,255,255,0.20); color: white; border: none;
                            border-radius: 4px; padding: 8px 14px; box-shadow: none; text-shadow: none; }
      button.lock-go:hover, button.lock-signin:hover { background: rgba(255,255,255,0.32); }
      button.lock-link { background: transparent; color: white; border: none; box-shadow: none;
                         text-shadow: 0 1px 3px rgba(0,0,0,0.7); padding: 2px 6px; font-size: 13px; }
      button.lock-link:hover { text-decoration: underline; background: transparent; }
      button.lock-method { background: rgba(255,255,255,0.14); color: white; border: 2px solid transparent;
                           border-radius: 4px; padding: 6px 10px; box-shadow: none; text-shadow: none; }
      button.lock-method:hover { background: rgba(255,255,255,0.26); }
      button.lock-method.selected { border-color: #B69CFF; }
      .lock-finger { color: white; font-size: 14px; text-shadow: 0 1px 3px rgba(0,0,0,0.7); }
    """

    class Lock:
        def __init__(self, start="clock"):
            self.user = session_account()
            self.owner = self.user  # compte de la session verrouillée
            self.password = requires_password(self.user)
            self.failures = 0
            self.blocked_until = 0
            self.view = "clock"
            # Options de connexion du compte affiché : code PIN, empreinte.
            self.signin_mod = signin_module()
            self.method = "password"
            self.can_pin = False
            self.can_finger = False
            self.finger_request = None
            self.finger_failures = 0
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
            # Écran branché, débranché ou redéfini (xrandr, retour de veille)
            # pendant le verrouillage : chaque écran reste couvert (#158).
            display.connect("monitor-added", lambda *_: GLib.idle_add(self.cover_monitors))
            display.connect("monitor-removed", lambda *_: GLib.idle_add(self.cover_monitors))
            Gdk.Screen.get_default().connect("monitors-changed", lambda *_: GLib.idle_add(self.cover_monitors))
            self.grab_tries = 0
            self.power_yield = None
            GLib.timeout_add(100, self.grab)
            GLib.timeout_add(1000, self.tick)
            GLib.timeout_add(100, self.watch_power)
            self.tick()
            if start == "signin":
                self.show_signin()

        @staticmethod
        def _geo(rect):
            return (rect.x, rect.y, rect.width, rect.height)

        def cover_monitors(self):
            display = Gdk.Display.get_default()
            monitors = [display.get_monitor(i) for i in range(display.get_n_monitors())]
            primary = display.get_primary_monitor() or (monitors[0] if monitors else None)
            geos = [self._geo(m.get_geometry()) for m in monitors]
            # La fenêtre principale suit l'écran principal s'il a bougé ou changé.
            if primary is not None:
                main_geo = self._geo(primary.get_geometry())
                if self.main_geo != main_geo:
                    x, y, width, height = main_geo
                    self.main.move(x, y)
                    self.main.set_size_request(width, height)
                    self.main.resize(width, height)
                    self.main_geo = main_geo
            covers = {geo: window for geo, window in self.covers.items()}
            # Un cache resté sur l'écran où la fenêtre principale vient d'aller
            # la masquerait : il part.
            stale = covers.pop(self.main_geo, None)
            if stale is not None:
                stale.destroy()
            add, remove = cover_plan(list(covers) + [self.main_geo], geos)
            for geo in remove:
                window = covers.pop(geo, None)
                if window is not None:
                    window.destroy()
            for geo in add:
                rect = Gdk.Rectangle()
                rect.x, rect.y, rect.width, rect.height = geo
                covers[geo] = self.make_window(rect, False)
            self.covers = covers
            self.windows = [self.main] + list(covers.values())
            self.main.present()
            return False

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
            if not hasattr(self, "covers"):
                self.covers = {}
            if main:
                self.main = window
                self.main_geo = (geo.x, geo.y, geo.width, geo.height)
                self.area = area
                overlay.add_overlay(self.clock_view(geo.width, geo.height))
                overlay.add_overlay(self.signin_view())
                self.keyboard_view(overlay)
                overlay.add_overlay(self.accounts_view())
                window.add_events(Gdk.EventMask.KEY_PRESS_MASK | Gdk.EventMask.BUTTON_PRESS_MASK)
                window.connect("key-press-event", self.on_key)
                window.connect("button-press-event", self.on_click)
            window.show_all()
            if not main:
                self.covers[(geo.x, geo.y, geo.width, geo.height)] = window
            if main:
                self.signin.hide()
                self.accounts.hide()
                if self.osk is not None:
                    self.osk.hide()
                    self.osk_button.hide()
                self.select_account(self.user)
            return window

        def keyboard_view(self, overlay):
            """Clavier tactile (winlux-osk) en bas de l'écran de connexion, et
            son bouton en bas à droite : ouvert d'office sans clavier physique.
            Deux calques distincts : un calque de la largeur de l'écran
            capterait les clics destinés aux autres comptes, en bas à gauche."""
            self.osk, self.osk_button, self.osk_auto = None, None, False
            module = osk_module()
            if module is None or getattr(self, "password_entry", None) is None:
                return
            try:
                keys = module.keyboard_widget(send=self.osk_send, on_close=self.osk_toggle)
                self.osk_auto = module.icon_wanted()
            except Exception:  # Clavier indisponible : l'écran reste utilisable.
                return
            self.osk_button = Gtk.Button.new_from_icon_name("input-keyboard-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
            self.osk_button.set_halign(Gtk.Align.END)
            self.osk_button.set_valign(Gtk.Align.END)
            self.osk_button.set_margin_end(24)
            self.osk_button.set_margin_bottom(24)
            self.osk_button.get_style_context().add_class("lock-go")
            self.osk_button.set_tooltip_text(_t("Clavier tactile"))
            self.osk_button.connect("clicked", lambda *_: self.osk_toggle())
            overlay.add_overlay(self.osk_button)
            keys.set_valign(Gtk.Align.END)
            keys.set_halign(Gtk.Align.FILL)
            screen = Gdk.Screen.get_default()
            self.osk_height = max(220, min(300, int((screen.get_height() if screen else 768) * 0.36)))
            keys.set_size_request(-1, self.osk_height)
            overlay.add_overlay(keys)
            self.osk = keys

        def osk_toggle(self):
            if self.osk is None:
                return
            self.osk_show(not self.osk.get_visible())
            self.last_typing = time.monotonic()

        def osk_show(self, visible):
            """Clavier affiché : le compte remonte en haut pour garder le champ
            du mot de passe visible au-dessus des touches."""
            if self.osk is None:
                return
            self.osk.set_visible(visible)
            self.signin.set_valign(Gtk.Align.START if visible else Gtk.Align.CENTER)
            self.signin.set_margin_top(32 if visible else 0)
            # Autres comptes (en bas à gauche) et bouton du clavier (en bas à
            # droite) passent au-dessus des touches au lieu de les recouvrir.
            above = self.osk_height + 16 if visible else 0
            self.accounts.set_margin_bottom(above or 28)
            self.osk_button.set_margin_bottom(above or 24)

        def osk_send(self, key, _modifiers):
            """Touches du clavier tactile vers le champ du mot de passe."""
            self.last_typing = time.monotonic()
            if self.entry is not None:
                type_into(self.entry, key, self.submit)

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
            self.stop_finger()
            self.finger_failures = 0
            self.apply_methods(name, None)
            self.refresh_methods(name)
            if self.view == "signin":
                self.password_row.set_visible(self.password)
                self.signin_button.set_visible(not self.password)
                if self.entry is not None:
                    self.entry.grab_focus()

        # ── Code PIN et empreinte ──

        def refresh_methods(self, name):
            """État des options de connexion demandé au relais, hors du fil GTK
            (fprintd peut mettre une seconde à démarrer)."""
            if self.signin_mod is None or not path(RELAY).exists():
                return
            import threading
            module, socket_path = self.signin_mod, str(path(RELAY))

            def work():
                status = module.client_status(name, socket_path)
                GLib.idle_add(lambda: self.apply_methods(name, status) and False)
            threading.Thread(target=work, daemon=True).start()

        def apply_methods(self, name, status):
            if name != self.user:
                return False
            status = status or {}
            self.can_pin = bool(self.password and status.get("pin") and not status.get("pin_locked"))
            self.can_finger = bool(self.password and status.get("reader") and status.get("fingers"))
            self.options_link.set_visible(self.can_pin)
            if not self.can_pin:
                self.options_box.hide()
            self.set_method("pin" if self.can_pin else "password", keep_error=True)
            if self.can_finger:
                self.finger_label.set_text(_t("Posez votre doigt sur le lecteur d’empreintes."))
                self.start_finger()
            else:
                self.finger_label.set_text("")
            self.finger_label.set_visible(self.can_finger and self.view == "signin")
            return False

        def set_method(self, method, keep_error=False):
            self.method = method if (method != "pin" or self.can_pin) else "password"
            entry = self.password_entry
            entry.set_placeholder_text(_t("Code PIN") if self.method == "pin" else _t("Mot de passe"))
            entry.set_input_purpose(Gtk.InputPurpose.PIN if self.method == "pin" else Gtk.InputPurpose.PASSWORD)
            entry.get_accessible().set_name(_t("Code PIN") if self.method == "pin" else _t("Mot de passe"))
            self.method_hint.set_visible(self.method == "pin" and bool(self.password))
            entry.set_text("")
            entry.set_visibility(False)
            if not keep_error:
                self.error.set_text("")
            for key, button in self.method_buttons.items():
                context = button.get_style_context()
                (context.add_class if key == self.method else context.remove_class)("selected")
            if self.view == "signin" and self.entry is not None and self.entry.get_sensitive():
                self.entry.grab_focus()

        def start_finger(self):
            """Le lecteur attend un doigt, sur l'horloge comme à la connexion."""
            if self.finger_request is not None or not self.can_finger or self.signin_mod is None:
                return
            import threading
            user = self.user
            if user == self.owner:
                payload = {"action": "fp-verify", "user": user}
            else:
                payload = {"op": "switch", "user": user, "finger": True}
            request = self.signin_mod.Request(payload, str(path(RELAY)))
            self.finger_request = request

            def event(item):
                if item.get("event") == "retry":
                    GLib.idle_add(lambda: self.finger_label.set_text(self.signin_mod.message("retry")) and False)

            def work():
                answer = request.run(event, timeout=self.signin_mod.VERIFY_SECONDS + 60)
                GLib.idle_add(lambda: self.finger_done(request, user, answer) and False)
            threading.Thread(target=work, daemon=True).start()

        def stop_finger(self):
            request, self.finger_request = self.finger_request, None
            if request is not None:
                request.cancel()

        def finger_done(self, request, user, answer):
            if request is not self.finger_request or request.cancelled or user != self.user:
                return False
            self.finger_request = None
            if answer.get("ok"):
                if user == self.owner:
                    self.unlock()
                else:
                    self.show_clock()
                return False
            error = answer.get("error")
            if answer.get("parental"):
                self.can_finger = False
                self.finger_label.hide()
                if self.view == "clock":
                    self.show_signin()
                self.error.set_text(_t("Contrôle parental : ce compte ne peut pas être utilisé maintenant. Un adulte peut accorder du temps."))
                return False
            if error == "no-match":
                self.finger_failures += 1
                if self.finger_failures >= MAX_TRIES:
                    self.finger_label.set_text(_t("Utilisez votre code PIN ou votre mot de passe."))
                    return False
                self.finger_label.set_text(_t("Empreinte non reconnue. Réessayez."))
                if self.view == "clock":
                    self.show_signin()
            elif error in ("timeout", "busy"):
                pass
            else:
                # Lecteur débranché, relais absent : seul le clavier reste.
                self.can_finger = False
                self.finger_label.hide()
                return False
            GLib.timeout_add(1500 if error == "busy" else 300, lambda: self.start_finger() and False)
            return False

        def clock_view(self, width=1920, height=1080):
            # Heure, date et invite au-dessus du losange du fond d'écran (il
            # commence au tiers de la hauteur). Tailles proportionnelles à la
            # hauteur : en 1366x768 le losange recouvrait la date et l'invite.
            scale = clock_scale(width, height)
            provider = Gtk.CssProvider()
            provider.load_from_data(clock_css(scale).encode())
            Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider, 901)
            self.clock = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, halign=Gtk.Align.CENTER,
                                 valign=Gtk.Align.START, margin_top=round(110 * scale))
            self.time_label = Gtk.Label()
            self.time_label.get_style_context().add_class("lock-time")
            self.date_label = Gtk.Label()
            self.date_label.get_style_context().add_class("lock-date")
            self.clock.pack_start(self.time_label, False, False, 0)
            self.clock.pack_start(self.date_label, False, False, 0)
            hint = Gtk.Label(label=_t("Appuyez sur une touche ou cliquez pour vous connecter"), margin_top=round(24 * scale))
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
            # GTK cache le texte indicatif du champ dès qu'il a le focus, donc
            # dès l'affichage : « Code PIN » est aussi écrit au-dessus (#1499).
            self.method_hint = Gtk.Label(label=_t("Code PIN"), halign=Gtk.Align.CENTER)
            self.method_hint.get_style_context().add_class("lock-method-hint")
            self.method_hint.set_no_show_all(True)
            self.signin.pack_start(self.method_hint, False, False, 0)
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
            # Doigt attendu par le lecteur (affiché seulement s'il y en a un).
            self.finger_label = Gtk.Label(wrap=True, max_width_chars=44, justify=Gtk.Justification.CENTER)
            self.finger_label.get_style_context().add_class("lock-finger")
            self.finger_label.set_no_show_all(True)
            self.signin.pack_start(self.finger_label, False, False, 0)
            # « Options de connexion » : mot de passe ou code PIN.
            self.options_link = Gtk.Button(label=_t("Options de connexion"), halign=Gtk.Align.CENTER)
            self.options_link.get_style_context().add_class("lock-link")
            self.options_link.set_no_show_all(True)
            self.options_link.connect("clicked", lambda *_: self.options_box.set_visible(not self.options_box.get_visible()))
            self.signin.pack_start(self.options_link, False, False, 0)
            self.options_box = Gtk.Box(spacing=8, halign=Gtk.Align.CENTER)
            self.options_box.set_no_show_all(True)
            self.method_buttons = {}
            theme = Gtk.IconTheme.get_default()
            for key, label, icon in (("password", _t("Mot de passe"), "dialog-password-symbolic"),
                                     ("pin", _t("Code PIN"), "input-dialpad-symbolic")):
                button = Gtk.Button(label=label)
                if theme is not None and theme.has_icon(icon):
                    button.set_image(Gtk.Image.new_from_icon_name(icon, Gtk.IconSize.BUTTON))
                    button.set_always_show_image(True)
                button.get_style_context().add_class("lock-method")
                button.get_accessible().set_name(label)
                button.connect("clicked", lambda _b, k=key: self.set_method(k))
                button.show_all()
                self.options_box.pack_start(button, False, False, 0)
                self.method_buttons[key] = button
            self.signin.pack_start(self.options_box, False, False, 0)
            self.entry = self.password_entry if self.password else None
            self.error = Gtk.Label(wrap=True, max_width_chars=44, justify=Gtk.Justification.CENTER)
            self.error.get_style_context().add_class("lock-error")
            self.signin.pack_start(self.error, False, False, 0)
            self.caps = Gtk.Label()
            self.caps.get_style_context().add_class("lock-caps")
            self.signin.pack_start(self.caps, False, False, 0)
            # Message d'erreur juste sous le champ, options de connexion après.
            for widget in (self.finger_label, self.options_link, self.options_box):
                self.signin.reorder_child(widget, -1)
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

        def watch_power(self):
            """Bouton d'alimentation pendant le verrouillage (#2254) : l'écran
            d'arrêt, plein écran et au-dessus du verrou, reçoit la saisie le
            temps de sa carte ; le verrou la reprend dès qu'il disparaît. Il ne
            montre que ses propres boutons (Annuler, Déverrouiller : #2300) : rien
            de la session verrouillée ne devient accessible."""
            pid = power_request_pid()
            if pid is not None and self.power_yield is None:
                self.power_yield = pid
                print(f"winlux-lock : saisie cédée à l'écran d'arrêt (pid {pid})", file=sys.stderr, flush=True)
                Gdk.Display.get_default().get_default_seat().ungrab()
            elif pid is None and self.power_yield is not None:
                print("winlux-lock : écran d'arrêt fermé, saisie reprise", file=sys.stderr, flush=True)
                self.power_yield = None
                self.main.present()
                GLib.timeout_add(50, self.grab)
            return True

        def grab(self):
            if self.power_yield is not None:
                return False
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
            self.finger_label.set_visible(self.can_finger)
            self.options_link.set_visible(self.can_pin)
            if len(self.account_names) > 1:
                self.accounts.show_all()
            self.area.queue_draw()
            if self.entry is not None:
                self.entry.grab_focus()
            if self.osk is not None:
                self.osk_button.show()
                if self.osk_auto and self.entry is not None:
                    self.osk_show(True)
            self.tick()

        def show_clock(self):
            self.view = "clock"
            self.signin.hide()
            if self.osk is not None:
                self.osk_show(False)
                self.osk_button.hide()
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

        def on_click(self, _widget, event=None):
            if self.view == "clock":
                self.show_signin()
                # Le clic qui réveille l'écran vaut aussi pour le compte visé :
                # revenu à l'horloge (30 s sans saisie, propriétaire de nouveau
                # sélectionné), un clic sur « Camille » en bas à gauche ne
                # faisait qu'afficher la connexion, et le mot de passe tapé
                # ensuite partait sur le mauvais compte (VM, 5 novembre).
                if event is not None and len(self.account_names) > 1:
                    where = (event.x_root, event.y_root)
                    GLib.idle_add(lambda: self.select_account_at(*where) and False)
            return False

        def account_at(self, x, y):
            """Compte dont le bouton (en bas à gauche) est sous ce point de l'écran."""
            for button, name in zip(self.accounts.get_children(), self.account_names):
                toplevel = button.get_toplevel().get_window()
                origin = button.translate_coordinates(button.get_toplevel(), 0, 0)
                if toplevel is None or origin is None or not button.get_mapped():
                    continue
                left, top = toplevel.get_origin()[1:]
                left, top = left + origin[0], top + origin[1]
                box = button.get_allocation()
                if left <= x < left + box.width and top <= y < top + box.height:
                    return name
            return None

        def select_account_at(self, x, y):
            name = self.account_at(x, y)
            if name is not None and name != self.user:
                self.select_account(name)
            return name

        def on_key(self, _widget, event):
            self.last_typing = time.monotonic()
            if self.view == "clock":
                # La première touche ne fait que réveiller l'écran : écran éteint
                # ou horloge seule, on ne voit pas le champ. Elle partait dans le
                # mot de passe et le testeur y a tapé « par » sans le voir (VM,
                # 3 novembre).
                self.show_signin()
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
            PARENTAL_REFUSED.clear()
            if self.method == "pin" and self.can_pin:
                self.submit_pin()
                return
            if self.user != self.owner:
                # Autre compte : le relais root vérifie et bascule de session ;
                # celle-ci reste verrouillée derrière.
                if relay_switch(self.user, self.entry.get_text()):
                    self.failures = 0
                    self.show_clock()
                    return
            elif check_password(self.user, self.entry.get_text()):
                open_keyring(self.entry.get_text())
                self.unlock()
                return
            self.entry.set_text("")
            if PARENTAL_REFUSED:
                self.error.set_text(_t("Contrôle parental : ce compte ne peut pas être utilisé maintenant. Un adulte peut accorder du temps."))
                return
            self.failures += 1
            if self.failures >= MAX_TRIES:
                self.blocked_until = time.monotonic() + WAIT_AFTER_TRIES
                self.entry.set_sensitive(False)
                self.tick()
            else:
                self.error.set_text(_t("Le mot de passe est incorrect. Réessayez."))

        def submit_pin(self):
            """Code PIN : le relais compare et compte les essais (5 au plus,
            puis le mot de passe redevient obligatoire)."""
            pin = self.entry.get_text()
            if self.user != self.owner:
                answer = {}
                if relay_switch(self.user, "", pin=pin, answer_out=answer):
                    self.failures = 0
                    self.show_clock()
                    return
            else:
                answer = self.signin_mod.ask("pin-check", str(path(RELAY)), timeout=30, user=self.user, pin=pin)
                if answer.get("ok"):
                    self.unlock()
                    return
            self.entry.set_text("")
            if answer.get("parental"):
                self.error.set_text(_t("Contrôle parental : ce compte ne peut pas être utilisé maintenant. Un adulte peut accorder du temps."))
                return
            error = answer.get("error") or "pin-wrong"
            if answer.get("locked") or error == "pin-locked":
                self.can_pin = False
                self.options_link.hide()
                self.options_box.hide()
                self.set_method("password", keep_error=True)
            self.error.set_text(self.signin_mod.message(error))

        def sign_in(self):
            """Compte sans mot de passe : sa session, ou celle d'un autre."""
            if self.user == self.owner:
                self.unlock()
            elif relay_switch(self.user, ""):
                self.show_clock()
            elif PARENTAL_REFUSED:
                self.error.set_text(_t("Contrôle parental : ce compte ne peut pas être utilisé maintenant. Un adulte peut accorder du temps."))
            else:
                self.error.set_text(_t("Impossible d’ouvrir la session de {name}.").format(name=self.user))

        def unlock(self):
            self.stop_finger()
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
            try:
                # Partagé entre les comptes (un marqueur par écran) : dossier
                # collant comme /tmp, chacun ne touche qu'à ses fichiers.
                if marker.parent.stat().st_uid == os.geteuid():
                    marker.parent.chmod(0o1777)
            except OSError:
                pass
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
