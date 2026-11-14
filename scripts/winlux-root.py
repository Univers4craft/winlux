#!/usr/bin/python3
"""Relais root de Winlux : le bureau tourne sous le compte de l'utilisateur.

Sur un système installé, le bureau n'est plus root. Les rares opérations qui
touchent au système (réglages dans /etc, Boutique et APT, mises à jour,
volumes, partages, bureau à distance…) passent par ce relais unique, qui
tourne en root et n'exécute que des actions connues :

  winlux-root serve                       (root) écoute /run/winlux-root.sock
  winlux-root control ACTION [ARGS…]      action de winlux-control, exécutée en root
  winlux-root exec apt-get|snap ARGS…     installation de logiciels (Boutique)
  winlux-root exec winlux-reset ARGS…     réinitialisation, clé de secours (Récupération)
  winlux-root exec winlux-wifi ARGS…      Wi-Fi : réseaux, clé (entrée standard), WPS, mode avion
  winlux-root exec winlux-vpn ARGS…       VPN WireGuard/OpenVPN (configuration sur l'entrée standard)
  winlux-root exec winlux-hotspot ARGS…   point d'accès sans fil mobile (partage de connexion)
  winlux-root exec winlux-brightness set N   luminosité (rétroéclairage, DDC/CI)
  winlux-root exec winlux-certificates …   autorités ajoutées, heure automatique
  winlux-root exec winlux-parental …      contrôle parental (lecture : administrateur ;
                                          modification : Contrôle de compte d'utilisateur)
  winlux-root conf FICHIER CLÉ VALEUR     préférence de /etc/winlux (sans confirmation)
  winlux-root check COMPTE                mot de passe lu sur l'entrée standard
  winlux-root elevate COMPTE              idem, et ouvre 5 minutes d'élévation
  winlux-root policy ACTION               user | admin | refusé (diagnostic)

Options de connexion (#659) : op « signin » (code PIN, empreinte, par
winlux-signin) et op « switch » avec « pin » ou « finger » au lieu du mot de
passe. Un compte ne touche qu'à son propre code et à ses propres empreintes,
et confirme d'abord son mot de passe pour les créer ou les effacer.

Le relais vérifie l'appelant par SO_PEERCRED : root, ou un compte du bureau
(/etc/winlux/users.conf, UID ≥ 1000) ; le compte isolé du navigateur n'y a
pas accès. Les actions « admin » (installer un logiciel, partager un dossier,
bureau à distance, points de restauration…) demandent d'abord la
confirmation du Contrôle de compte d'utilisateur (winlux-uac --elevate) avec
le mot de passe d'un administrateur, comme sous Windows ; l'élévation dure
5 minutes pour ce compte. Les mots de passe sont vérifiés ici, par crypt()
sur /etc/shadow, que seul root lit : ni l'écran de verrouillage ni le
contrôle de compte n'ont besoin de lire /etc/shadow.

Les descripteurs d'entrée/sortie du client sont transmis au relais
(SCM_RIGHTS) : la commande écrit directement dans le journal du client, et
`apt-get -o APT::Status-Fd=N` garde sa progression.
"""
import array
import ctypes
import ctypes.util
import grp
import hmac
import json
import os
import pwd
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import time

ROOT = os.environ.get("WINLUX_ROOT_PREFIX", "")
SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", ROOT + "/run/winlux-root.sock")
USERS_CONF = ROOT + "/etc/winlux/users.conf"
CONFIG = ROOT + "/etc/winlux"
CONTROL = os.environ.get("WINLUX_ROOT_CONTROL", "/usr/bin/winlux-control")
SHARE = "/usr/bin/winlux-share"
SIGNIN = os.environ.get("WINLUX_ROOT_SIGNIN", "/usr/bin/winlux-signin")
PARENTAL = os.environ.get("WINLUX_ROOT_PARENTAL", "/usr/bin/winlux-parental")
PARENTAL_CONF = ROOT + "/etc/winlux/parental"
ELEVATION_SECONDS = 300
ADMIN_GROUPS = ("sudo", "wheel", "admin")
NAME = re.compile(r"^[a-z_][a-z0-9_.-]{0,31}$")
SAFE_ARG = re.compile(r"^[^\n\r\0]{0,512}$")
PACKAGE = re.compile(r"^[a-z0-9][a-z0-9+.:~-]{0,127}$")

# ── Politique ───────────────────────────────────────────────────────────────

# Actions de winlux-control relayées. « user » : tout compte du bureau, sans
# question (réglages que Windows laisse à un utilisateur standard) ; « admin »
# : confirmation par le Contrôle de compte d'utilisateur.
CONTROL_POLICY = (
    (r"browser-(install|default)-(brave|chrome|edge|firefox|opera|operagx|tor|vivaldi)", "admin"),
    (r"store-catalog-(add|remove)-[a-z0-9-]+", "admin"),
    (r"network-(renew|scan|connect|disconnect)", "user"),
    (r"bluetooth-(on|off|scan|pair|connect|disconnect|remove)", "user"),
    (r"(tz|timezone)-[A-Za-z0-9/_+-]+", "user"),
    (r"keyboard-[a-z]{2}|tap-(on|off)|mouse-(slow|normal|fast)|touchpad-set", "user"),
    (r"(lang|locale)-(fr|en|es|de)|theme-(dark|light)|align-(left|center)", "user"),
    (r"welcome-(done|reset|step-[0-9]{1,2})", "user"),
    (r"privacy-[a-z-]+|lock-after-[0-9]+|lock-resume-(on|off)", "user"),
    (r"overlay-[a-z0-9-]+", "user"),
    (r"cpu-profile-(performance|balanced|powersave)|game-mode-(auto|off)", "user"),
    (r"cpu-hybrid-[a-z]+|cpu-fan-[a-z]+|cpu-restore", "user"),
    (r"gpu-set", "admin"),
    (r"drivers-(refresh|firmware-check|auto-on|auto-off)", "user"),
    (r"drivers-(install-[a-z-]+|remove-nvidia|firmware-update)", "admin"),
    (r"display-[a-z0-9x-]+", "user"),
    (r"power-hibernate-setup", "admin"),
    (r"power-set-(screen|sleep)-(ac|dc)-[0-9]{1,5}", "user"),
    (r"power-[a-z-]+", "user"),
    (r"update-(check|apply|auto-on|auto-off|resume|pause-[1-5])", "user"),
    (r"update-hours-[0-9]{1,2}-[0-9]{1,2}", "user"),
    (r"update-rollback", "admin"),
    (r"sysupdate-[a-z-]+", "user"),
    (r"journal-(status|list|errors|crashes|export)", "user"),
    (r"journal-(clear|rotate)", "admin"),
    (r"restore-(list|status)", "user"),
    (r"restore-(create|restore|delete)", "admin"),
    (r"backup-(status|request|list|prune|drives|run|set|add|remove|share|browse|versions|restore)", "user"),
    (r"session-(register|unregister|switch|logoff|list|display)", "user"),
    (r"remote-[a-z-]+", "admin"),
    (r"share-(status|list|state|ensure)", "user"),
    (r"share-[a-z-]+", "admin"),
    (r"transfer-[a-z-]+", "user"),
    (r"phone-firewall-(on|off)", "user"),
    # Type de réseau (#1103) : passer en Public ne fait que fermer ; Privé
    # ouvre partage et découverte, donc un administrateur confirme.
    (r"firewall-profile-public", "user"),
    (r"firewall-profile-private", "admin"),
    (r"report-(offer|redact|mode|auto)-[a-z]+", "user"),
    # Envoi automatique des plantages (#438) : le rapport arrive sur l'entrée
    # standard et root le vérifie ; le jeton (lu sur l'entrée) exige un admin.
    (r"report-send", "user"),
    (r"report-token-(set|forget)", "admin"),
    (r"account-(add|password|role|remove)", "admin"),
)

# Préférences que le bureau écrit lui-même (winlux-control save) : fichier → clés.
CONF_KEYS = {
    "preferences.conf": None,
    "input.conf": None,
    "locale.conf": ("LANG",),
    "privacy.conf": None,
    "lock.conf": ("LOCK_AFTER", "LOCK_ON_RESUME"),
    "overlay.conf": None,
    "cpu.conf": ("GAME_MODE", "HYBRID_POLICY"),
}
# Réglages valables pour tout l'appareil (#2269) : un administrateur les
# change, un compte standard passe par le Contrôle de compte d'utilisateur,
# comme l'accès à la caméra ou au micro de l'appareil sous Windows. Les
# autres fichiers de CONF_KEYS sont des réglages de l'utilisateur (langue
# d'affichage, thème, souris, verrouillage) et restent sans élévation.
CONF_MACHINE = {"privacy.conf"}
CONF_VALUE = re.compile(r"^[A-Za-z0-9._/+-]{0,64}$")
CONF_KEY = re.compile(r"^[A-Z][A-Z0-9_]{0,40}$")

APT_SUBCOMMANDS = {"install": "admin", "remove": "admin", "purge": "admin", "upgrade": "admin",
                   "dist-upgrade": "admin", "full-upgrade": "admin", "autoremove": "admin",
                   "update": "user"}
# --mark-auto : paquets compagnons de la Boutique (#2075), retirés avec
# l'application. --autoremove et --purge : la désinstallation du Store (apt-get purge
# --autoremove) était refusée par le relais, sans demande ni message.
APT_FLAGS = {"-y", "-q", "-qq", "--no-install-recommends", "--reinstall", "-f", "--fix-broken",
             "--only-upgrade", "--no-remove", "--autoremove", "--purge", "--mark-auto"}
APT_OPTIONS = re.compile(r"^(DPkg::Lock::Timeout=[0-9]{1,4}|Dpkg::Use-Pty=0|APT::Status-Fd=[0-9]{1,3})$")
SNAP_SUBCOMMANDS = {"install": "admin", "remove": "admin", "refresh": "admin"}
# Réinitialiser ce PC, clé de secours, réparation du démarrage (Paramètres >
# Récupération) : tout ce qui modifie un disque demande un administrateur.
RESET_SUBCOMMANDS = {"status": "user", "sources": "user", "list-keys": "user",
                     "prepare": "admin", "cancel": "admin", "repair-boot": "admin",
                     "reboot-recovery": "admin", "make-key": "admin"}
RESET_ARG = re.compile(r"^(--(mode|source|user|fullname|device)|--clean|--scan|keep|wipe|cloud|auto|"
                       r"(media|iso):/[^\n\r\0]{1,400}|/dev/[A-Za-z0-9/_-]{1,64}|[^\n\r\0:,-][^\n\r\0:,]{0,63})$")


# Wi-Fi (Paramètres > Wi-Fi) : rejoindre un réseau est permis à un compte
# standard, comme sous Windows. La clé arrive sur l'entrée standard.
WIFI_SUBCOMMANDS = {"status": 0, "list": 1, "known": 0, "connect": 21, "up": 1, "wps": 1, "forget": 1,
                    "autoconnect": 2, "disconnect": 0, "radio": 1, "airplane": 1}
WIFI_OPTIONS = {"--rescan", "--hidden", "--no-auto", "--key-stdin", "--security", "--identity",
                "--eap", "--phase2", "--ca", "--domain", "--client-cert", "--private-key"}
# Fichiers de certificat : chemins absolus ; winlux-wifi vérifie que le compte
# appelant peut les lire avant de les copier.
WIFI_FILE = re.compile(r"^/[^\n\r\0]{1,400}$")


def wifi_level(rest):
    """Niveau d'une commande winlux-wifi, None si elle est refusée."""
    if not rest or rest[0] not in WIFI_SUBCOMMANDS or len(rest) - 1 > WIFI_SUBCOMMANDS[rest[0]]:
        return None
    command, args = rest[0], rest[1:]
    if command in ("radio", "airplane") and args not in (["on"], ["off"]):
        return None
    if command == "autoconnect" and (len(args) != 2 or args[1] not in ("on", "off")):
        return None
    if command == "list" and args not in ([], ["--rescan"]):
        return None
    if command == "connect":
        if not args or args[0].startswith("-"):
            return None
        it = iter(args[1:])
        for option in it:
            if option not in WIFI_OPTIONS:
                return None
            if option == "--security" and next(it, "") not in ("open", "wpa", "sae", "wep", "owe", "eap"):
                return None
            if option == "--identity" and not next(it, ""):
                return None
            if option == "--eap" and next(it, "") not in ("peap", "ttls", "tls"):
                return None
            if option == "--phase2" and next(it, "") not in ("mschapv2", "pap", "gtc"):
                return None
            if option == "--ca":
                value = next(it, "")
                if value not in ("system", "none") and not WIFI_FILE.match(value):
                    return None
            if option in ("--client-cert", "--private-key") and not WIFI_FILE.match(next(it, "")):
                return None
            if option == "--domain" and not re.fullmatch(r"[A-Za-z0-9.*-]{1,253}", next(it, "")):
                return None
    if command in ("up", "forget", "wps", "autoconnect") and args and args[0].startswith("-"):
        return None
    return "user"


# VPN (Paramètres > VPN, #660) : comme sous Windows, un compte standard ajoute
# une connexion VPN et s'y connecte. Configuration et mots de passe arrivent
# sur l'entrée standard, jamais dans la ligne de commande ; le service ne lit
# aucun fichier du compte (la page les lui envoie).
VPN_UUID = re.compile(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")


def vpn_level(rest):
    """Niveau d'une commande winlux-vpn, None si elle est refusée."""
    if rest in (["status"], ["import", "--stdin"]):
        return "user"
    if len(rest) == 2 and rest[0] in ("connect", "disconnect", "remove") and VPN_UUID.match(rest[1]):
        return "user"
    if len(rest) == 3 and rest[0] == "credentials" and VPN_UUID.match(rest[1]) and rest[2] == "--stdin":
        return "user"
    return None


def hotspot_level(rest):
    """Point d'accès sans fil mobile : permis à un compte standard, comme sous Windows."""
    return "user" if rest in (["status"], ["start"], ["stop"], ["configure", "--stdin"]) else None


def control_level(args):
    """Niveau requis pour une action de winlux-control, None si elle est refusée."""
    if not args or not all(SAFE_ARG.match(a) for a in args):
        return None
    action = args[0]
    for pattern, level in CONTROL_POLICY:
        if re.fullmatch(pattern, action):
            return level
    return None


def exec_level(args):
    """Niveau requis pour une commande de la Boutique, None si elle est refusée."""
    if not args or not all(SAFE_ARG.match(a) for a in args):
        return None
    tool, rest = args[0], args[1:]
    if tool == "apt-get":
        level, after_dashes, subcommand = None, False, None
        it = iter(rest)
        for arg in it:
            if after_dashes:
                if not (PACKAGE.match(arg) or (arg.startswith("/") and arg.endswith(".deb") and ".." not in arg)):
                    return None
                continue
            if arg == "--":
                after_dashes = True
            elif arg == "-o":
                if not APT_OPTIONS.match(next(it, "")):
                    return None
            elif arg.startswith("-o"):
                if not APT_OPTIONS.match(arg[2:]):
                    return None
            elif arg in APT_FLAGS:
                continue
            elif subcommand is None and arg in APT_SUBCOMMANDS:
                subcommand = arg
                level = APT_SUBCOMMANDS[arg]
            elif subcommand is not None and PACKAGE.match(arg):
                continue
            else:
                return None
        return level
    if tool == "winlux-reset" and rest and rest[0] in RESET_SUBCOMMANDS:
        if all(RESET_ARG.match(a) for a in rest[1:]):
            return RESET_SUBCOMMANDS[rest[0]]
        return None
    if tool == "winlux-wifi":
        return wifi_level(rest)
    if tool == "winlux-vpn":
        return vpn_level(rest)
    if tool == "winlux-hotspot":
        return hotspot_level(rest)
    if tool == "winlux-certificates":
        # Ajouter une autorité de confiance touche à la sécurité de tout le
        # système : mot de passe d'un administrateur, comme sous Windows.
        if len(rest) == 2 and rest[0] == "add" and re.fullmatch(r"/[^\n\r\0]{1,400}", rest[1]):
            return "admin"
        if len(rest) == 2 and rest[0] == "remove" and re.fullmatch(r"[A-Za-z0-9_-][A-Za-z0-9._-]{0,79}", rest[1]):
            return "admin"
        if rest in (["time", "auto", "on"], ["time", "auto", "off"]):
            return "admin"
        if rest in (["time", "sync"], ["time", "rtc"]):
            return "user"
        return None
    if tool == "winlux-brightness":
        # Luminosité : le compte n'écrit pas dans /sys/class/backlight ni sur l'i2c.
        if rest == ["detect-ddc"] or (len(rest) == 2 and rest[0] == "set" and re.fullmatch(r"\d{1,3}", rest[1])):
            return "user"
        return None
    if tool == "winlux-parental":
        return parental_level(rest)
    if tool == "winlux-store-repos":
        # Dépôt d'un logiciel d'éditeur désinstallé par le Store (#1056).
        # « check » : autorisation demandée avant le téléchargement (#1451).
        ident = r"[a-z0-9][a-z0-9-]{0,40}"
        if len(rest) == 2 and rest[0] in ("clean", "check") and re.fullmatch(ident, rest[1]):
            return "admin"
        # « grant » laisse un accord à usage unique pour ce logiciel et cette
        # empreinte ; « install » l'utilise (#1453) ; « revoke » le retire.
        if len(rest) == 3 and rest[0] == "grant" and re.fullmatch(ident, rest[1]) \
                and re.fullmatch(r"[0-9a-f]{64}", rest[2]):
            return "admin"
        if len(rest) in (4, 5) and rest[0] == "install" and re.fullmatch(ident, rest[1]) \
                and re.fullmatch(r"[0-9a-f]{64}", rest[2]) and re.fullmatch(r"/[^\n\r\0]{1,400}\.deb", rest[3]) \
                and ".." not in rest[3].split("/") \
                and (len(rest) == 4 or re.fullmatch(r"APT::Status-Fd=[0-9]{1,3}", rest[4])):
            return "admin"
        if len(rest) == 2 and rest[0] == "revoke" and re.fullmatch(ident, rest[1]):
            return "user"
        return None
    if tool == "snap" and rest and rest[0] in SNAP_SUBCOMMANDS:
        if all(PACKAGE.match(a) or a in ("--classic",) for a in rest[1:]):
            return SNAP_SUBCOMMANDS[rest[0]]
    return None


def parental_level(rest):
    """Contrôle parental (#661) : le compte voit son temps restant ; lire les
    réglages demande un administrateur, les changer le Contrôle de compte."""
    if rest in (["status"],):
        return "user"
    if rest == ["list"]:
        return "admin-read"
    if len(rest) < 2 or not NAME.match(rest[1]):
        return None
    if rest[0] == "get" and len(rest) == 2:
        return "admin-read"
    if rest[0] == "report" and (len(rest) == 2 or (len(rest) == 3 and re.fullmatch(r"[0-9]{1,2}", rest[2]))):
        return "admin-read"
    if rest[0] in ("set", "enable", "disable", "forget") and len(rest) == 2:
        return "admin"
    if rest[0] == "grant" and len(rest) == 3 and re.fullmatch(r"[0-9]{1,3}", rest[2]):
        return "admin"
    return None


# ── Contrôle parental ───────────────────────────────────────────────────────

def supervised(name):
    """Compte surveillé par le contrôle parental (configuration root, activée)."""
    if not isinstance(name, str) or not NAME.match(name) or name == "root":
        return False
    path = os.path.join(PARENTAL_CONF, name + ".json")
    try:
        if os.path.islink(path):
            return False
        with open(path, encoding="utf-8") as conf:
            return bool(json.load(conf).get("enabled", True))
    except (OSError, ValueError, AttributeError):
        return False


def parental_env():
    env = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "LANG": os.environ.get("LANG", "C.UTF-8")}
    if ROOT:
        env["WINLUX_PARENTAL_ROOT"] = ROOT
        for key in ("WINLUX_PARENTAL_NOW",):
            if os.environ.get(key):
                env[key] = os.environ[key]
    return env


def run_waited(command, **options):
    """subprocess.run avec un vrai code de retour (#811) : le service ignore
    SIGCHLD, le noyau récupérait l'enfant seul et wait() rendait toujours 0."""
    previous = signal.signal(signal.SIGCHLD, signal.SIG_DFL)
    try:
        return subprocess.run(command, **options)
    finally:
        signal.signal(signal.SIGCHLD, previous)
        # Enfants d'autres demandes terminés pendant l'attente : récupérés.
        try:
            while os.waitpid(-1, os.WNOHANG)[0]:
                pass
        except ChildProcessError:
            pass


def parental_command(*args):
    return ([sys.executable, "-B", PARENTAL] if PARENTAL.endswith(".py") else [PARENTAL]) + list(args)


def parental_block(user):
    """None si le compte peut utiliser sa session maintenant, sinon la raison
    du contrôle parental (temps écoulé, hors des heures permises)."""
    if not supervised(user):
        return None
    try:
        result = run_waited(parental_command("check-login", user), env=parental_env(), capture_output=True,
                            text=True, timeout=20)
    except (OSError, subprocess.SubprocessError):
        return {"reason": "limit"}  # en cas de doute, le compte reste bloqué
    try:
        answer = json.loads(result.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        return {"reason": "limit"}
    return None if answer.get("allowed") is True else answer


# ── Comptes et mots de passe ────────────────────────────────────────────────

def users_conf():
    """{nom: (rôle, mot de passe ou marqueur)} de /etc/winlux/users.conf."""
    out = {}
    try:
        with open(USERS_CONF, encoding="utf-8", errors="replace") as conf:
            for line in conf:
                parts = line.rstrip("\n").split(":", 2)
                if len(parts) == 3 and parts[0]:
                    out[parts[0]] = (parts[1], parts[2])
    except OSError:
        pass
    return out


def account(uid):
    try:
        return pwd.getpwuid(uid).pw_name
    except KeyError:
        return None


def desktop_account(uid):
    """Nom du compte si cet UID a le droit de parler au relais."""
    if uid == 0:
        return "root"
    name = account(uid)
    if name is None or uid < 1000 or name not in users_conf():
        return None
    return name


def is_admin(name):
    if name == "root":
        return True
    role = users_conf().get(name, ("", ""))[0]
    if role in ("root", "admin", "administrateur"):
        return True
    if role == "user" and ROOT:
        return False
    for group in ADMIN_GROUPS:
        try:
            if name in grp.getgrnam(group).gr_mem:
                return True
        except KeyError:
            continue
    return False


def shadow_hash(user):
    try:
        with open(ROOT + "/etc/shadow", encoding="utf-8", errors="replace") as shadow:
            for line in shadow:
                parts = line.split(":")
                if len(parts) >= 2 and parts[0] == user:
                    return parts[1]
    except OSError:
        pass
    return None


def _crypt(password, setting):
    lib = ctypes.CDLL(ctypes.util.find_library("crypt") or "libcrypt.so.1")
    lib.crypt.restype = ctypes.c_char_p
    lib.crypt.argtypes = (ctypes.c_char_p, ctypes.c_char_p)
    out = lib.crypt(password.encode(), setting.encode())
    return out.decode(errors="replace") if out else ""


def password_ok(user, password):
    """Mot de passe local du compte : /etc/shadow, sinon fichier Live."""
    stored = shadow_hash(user)
    if stored and stored.startswith("$"):
        try:
            return hmac.compare_digest(_crypt(password, stored), stored)
        except OSError:
            return False
    live = users_conf().get(user, (None, None))[1]
    if live is not None and live not in ("shadow", "nopass", ""):
        return hmac.compare_digest(live.encode(), password.encode())
    if live == "nopass" or (stored is not None and stored == ""):
        return password == ""
    return False


# ── Serveur ─────────────────────────────────────────────────────────────────

FAILURES = {}
ELEVATED = {}


def throttle(uid):
    """Après des échecs, chaque essai attend un peu plus (au plus 10 s)."""
    count = FAILURES.get(uid, 0)
    if count >= 3:
        time.sleep(min(10, count - 2))


def sync_share_password(user, password):
    """Le mot de passe réseau du compte suit celui de la session (partages)."""
    if os.path.exists(SHARE) and users_conf().get(user, ("", ""))[1] == "shadow":
        try:
            subprocess.run([SHARE, "set-password", user], input=password + "\n", text=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
        except (OSError, subprocess.SubprocessError):
            pass


def check_request(uid, name, request):
    user = request.get("user") or name
    password = request.get("password", "")
    if not isinstance(user, str) or not NAME.match(user) or not isinstance(password, str):
        return {"ok": False}
    # Un compte standard ne vérifie que son propre mot de passe ; pour un
    # autre compte, il passe par « elevate » (mot de passe d'administrateur).
    if user != name and uid != 0 and request.get("op") == "check":
        return {"ok": False}
    # « admin-check » : la fenêtre UAC d'un compte standard vérifie le mot de
    # passe de l'administrateur choisi sans ouvrir d'élévation au compte (#2292).
    if request.get("op") in ("elevate", "admin-check") and not is_admin(user):
        return {"ok": False}
    throttle(uid)
    ok = password_ok(user, password)
    if ok and request.get("op") == "check":
        # Hors du temps permis par le contrôle parental, le compte surveillé
        # ne déverrouille pas sa session (le mot de passe reste bon).
        block = parental_block(user)
        if block is not None:
            FAILURES.pop(uid, None)
            return {"ok": False, "parental": block.get("reason") or "limit", "until": block.get("until")}
    if ok:
        FAILURES.pop(uid, None)
        pin_unblock(user)
        if request.get("op") == "elevate":
            ELEVATED[uid] = time.monotonic() + ELEVATION_SECONDS
        sync_share_password(user, password)
    else:
        FAILURES[uid] = FAILURES.get(uid, 0) + 1
    return {"ok": ok}


# Accords de la Boutique (#1453) : compte → (logiciel, SHA-256, échéance).
# Donné par « winlux-store-repos grant » sous élévation, il ne permet qu'un
# seul « winlux-store-repos install » du même logiciel avec la même empreinte,
# que winlux-store-repos revérifie sur sa propre copie du paquet. Gardé en
# mémoire seulement : jamais écrit dans un fichier ni un journal.
STORE_GRANTS = {}
STORE_GRANT_SECONDS = 3600


def store_grant(uid, args, allowed):
    """Tient les accords de la Boutique ; vrai si un accord admet la commande."""
    if len(args) < 2 or args[0] != "winlux-store-repos":
        return False
    now = time.monotonic()
    for key in [k for k, v in STORE_GRANTS.items() if v[2] <= now]:
        del STORE_GRANTS[key]
    verb = args[1]
    if verb == "grant" and allowed and len(args) == 4:
        STORE_GRANTS[uid] = (args[2], args[3], now + STORE_GRANT_SECONDS)
    elif verb == "revoke" and len(args) == 3 and STORE_GRANTS.get(uid, ("",))[0] == args[2]:
        del STORE_GRANTS[uid]
    elif verb == "install" and len(args) >= 5 and STORE_GRANTS.get(uid, ("", ""))[:2] == (args[2], args[3]):
        # Consommé à la première utilisation.
        del STORE_GRANTS[uid]
        return True
    return False


def admitted(op, args, uid, name, level):
    """La demande passe : niveau suffisant, élévation en cours ou accord de la Boutique."""
    allowed = level != "admin" or elevated(uid, name)
    if op == "exec":
        allowed = store_grant(uid, args, allowed) or allowed
    return allowed


def elevated(uid, name):
    # Accordée après le mot de passe d'un administrateur, même tapé par un
    # compte standard (comme le Contrôle de compte d'utilisateur de Windows).
    return uid == 0 or ELEVATED.get(uid, 0) > time.monotonic()


def root_env(request, name):
    env = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "HOME": "/root", "USER": "root", "LOGNAME": "root",
           "WINLUX_CONTROL_RELAYED": "1", "WINLUX_PEER_USER": name,
           "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/run/dbus/system_bus_socket"}
    display = request.get("display", "")
    if isinstance(display, str) and re.fullmatch(r":[0-9]{1,2}(\.[0-9])?", display):
        env["DISPLAY"] = display
    lang = request.get("lang", "")
    if isinstance(lang, str) and re.fullmatch(r"[A-Za-z_]{1,8}(\.UTF-8|\.utf8)?", lang):
        env["LANG"] = lang
    try:
        with open("/etc/winlux/proxy.env", encoding="utf-8") as proxy:
            for line in proxy:
                match = re.match(r"^(?:export\s+)?([a-zA-Z_]+_proxy|[A-Z_]+_PROXY)=['\"]?([^'\"\n]*)", line)
                if match:
                    env[match.group(1)] = match.group(2)
    except OSError:
        pass
    return env


def write_conf(name, key, value):
    path = os.path.join(CONFIG, name)
    lines = []
    try:
        with open(path, encoding="utf-8", errors="replace") as conf:
            lines = [line.rstrip("\n") for line in conf if not line.startswith(key + "=")]
    except OSError:
        pass
    lines.append(f"{key}={value}")
    os.makedirs(CONFIG, exist_ok=True)
    tmp = f"{path}.{os.getpid()}"
    with open(tmp, "w", encoding="utf-8") as out:
        out.write("\n".join(lines) + "\n")
    os.chmod(tmp, 0o644)
    os.rename(tmp, path)


def run_command(command, fds, extra, env, peer_uid, cwd="/"):
    """Lance la commande avec les descripteurs du client ; renvoie son code."""
    stdin, stdout, stderr = fds[:3]
    mapping = list(zip(extra, fds[3:]))

    def child_setup():
        os.setsid()
        os.umask(0o022)
        # Descripteurs supplémentaires remis à leur numéro chez le client.
        staged = [(target, os.dup(fd)) for target, fd in mapping]
        for target, fd in staged:
            os.dup2(fd, target)
            os.set_inheritable(target, True)

    started = time.time()
    proc = subprocess.Popen(command, stdin=stdin, stdout=stdout, stderr=stderr, env=env,
                            preexec_fn=child_setup, pass_fds=[t for t, _ in mapping], cwd=cwd)
    code = proc.wait()
    give_back(peer_uid, started)
    return code


# Fichiers d'état écrits pendant une action relayée et réécrits ensuite par
# le bureau : ils sont rendus au compte (le dossier /run/winlux est collant).
GIVE_BACK = ("updates.txt", "update-state.txt", "networks.txt", "cpu.txt", "control.txt", "action.txt", "sessions.txt",
             "update-error-phase", "browser.log", "sysupdate.txt", "remote-state.txt", "drivers.txt",
             "drivers.log")


def give_back(uid, since):
    if uid == 0:
        return
    run = ROOT + "/run/winlux"
    for name in GIVE_BACK:
        path = os.path.join(run, name)
        try:
            st = os.lstat(path)
        except OSError:
            continue
        if st.st_uid == 0 and st.st_mtime >= since - 1 and not os.path.islink(path):
            try:
                os.chown(path, uid, -1, follow_symlinks=False)
                os.chmod(path, 0o644)
            except OSError:
                pass


def switch_request(uid, name, request, connection=None):
    """« Changer d'utilisateur » depuis l'écran de verrouillage : le mot de
    passe du compte choisi (ou son code PIN, ou son doigt) est vérifié ici,
    puis sa session est ouverte ou rejointe (winlux-sessions switch)."""
    user = request.get("user")
    password = request.get("password", "")
    if not isinstance(user, str) or not NAME.match(user) or user not in users_conf() or \
            not isinstance(password, str):
        return {"ok": False}
    if "pin" in request:
        if password_ok(user, ""):
            return {"ok": False, "error": "no-password"}
        answer = signin_run("pin-check", user, request.get("pin") if isinstance(request.get("pin"), str) else "")
        if not answer.get("ok"):
            return {"ok": False, "error": answer.get("error", "pin-wrong"), "locked": bool(answer.get("locked"))}
    elif request.get("finger"):
        if connection is None or password_ok(user, ""):
            return {"ok": False, "error": "failed"}
        answer = signin_stream(connection, "fp-verify", user)
        if not answer.get("ok"):
            return {"ok": False, "error": answer.get("error", "no-match")}
    else:
        throttle(uid)
        if not password_ok(user, password):
            FAILURES[uid] = FAILURES.get(uid, 0) + 1
            return {"ok": False}
        FAILURES.pop(uid, None)
        pin_unblock(user)
    block = parental_block(user)
    if block is not None:
        return {"ok": False, "parental": block.get("reason") or "limit", "until": block.get("until")}
    try:
        result = run_waited([CONTROL, "session-switch", user], env=root_env(request, name),
                                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                timeout=90)
    except (OSError, subprocess.SubprocessError):
        return {"ok": False}
    return {"ok": result.returncode == 0}


# ── Options de connexion : code PIN et empreinte (winlux-signin, #659) ──────

SIGNIN_ACTIONS = ("status", "pin-check", "pin-set", "pin-remove", "fp-verify", "fp-enroll", "fp-delete")
# Le code et les empreintes d'un compte ne regardent que lui.
SIGNIN_OWN = ("pin-check", "pin-set", "pin-remove", "fp-verify", "fp-enroll", "fp-delete")
# Créer ou effacer : le mot de passe du compte d'abord, comme sous Windows.
SIGNIN_PASSWORD = ("pin-set", "pin-remove", "fp-enroll", "fp-delete")
# Le lecteur peut attendre un doigt une minute : processus à part.
SIGNIN_FORK = ("status", "fp-verify", "fp-enroll", "fp-delete")


def signin_env():
    env = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "HOME": "/root", "LANG": "C.UTF-8",
           "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/run/dbus/system_bus_socket"}
    for key in ("WINLUX_ROOT_PREFIX", "WINLUX_FPRINT_FAKE"):
        if os.environ.get(key):
            env[key] = os.environ[key]
    return env


def _last_answer(text):
    for line in reversed((text or "").splitlines()):
        try:
            answer = json.loads(line)
        except ValueError:
            continue
        if isinstance(answer, dict) and "ok" in answer:
            return answer
    return {"ok": False, "error": "failed"}


def signin_run(action, user, secret=None):
    """winlux-signin ACTION COMPTE, secret sur l'entrée standard ; réponse finale."""
    try:
        result = run_waited([SIGNIN, action, user], input=(secret or "") + "\n", text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=signin_env(), timeout=60)
    except (OSError, subprocess.SubprocessError):
        return {"ok": False, "error": "failed"}
    return _last_answer(result.stdout)


def pin_unblock(user):
    """Un mot de passe juste débloque le code PIN du compte."""
    if os.path.exists(os.path.join(ROOT + "/var/lib/winlux/signin", user + ".pin")):
        signin_run("pin-unblock", user)


def signin_stream(connection, action, user):
    """winlux-signin en flux : chaque événement (doigt à reposer, étape
    d'enregistrement) part tout de suite au client ; s'il raccroche (mot de
    passe tapé, écran quitté), l'opération est arrêtée et le lecteur relâché."""
    import select
    try:
        proc = subprocess.Popen([SIGNIN, action, user], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, env=signin_env(), text=True)
    except OSError:
        return {"ok": False, "error": "failed"}
    final = {"ok": False, "error": "failed"}
    try:
        while True:
            readable, _, _ = select.select([proc.stdout, connection], [], [], 1.0)
            if connection in readable:
                try:
                    gone = not connection.recv(1, socket.MSG_PEEK)
                except OSError:
                    gone = True
                if gone:
                    proc.terminate()
                    proc.wait(timeout=10)
                    return {"ok": False, "error": "cancelled"}
            if proc.stdout in readable:
                line = proc.stdout.readline()
                if not line:
                    break
                try:
                    event = json.loads(line)
                except ValueError:
                    continue
                if not isinstance(event, dict):
                    continue
                if "ok" in event:
                    final = event
                else:
                    reply(connection, event)
        proc.wait(timeout=10)
    except (OSError, subprocess.SubprocessError):
        proc.kill()
    return final


def parental_gate(user, answer):
    """Code ou doigt justes : le contrôle parental s'applique comme au mot
    de passe (#661), sinon le code PIN contournerait les horaires permis."""
    if answer.get("ok"):
        block = parental_block(user)
        if block is not None:
            return {"ok": False, "parental": block.get("reason") or "limit", "until": block.get("until")}
    return answer


def signin_target(uid, name, request):
    action = request.get("action")
    user = request.get("user") or name
    if action not in SIGNIN_ACTIONS or not isinstance(user, str) or not NAME.match(user) or \
            (user not in users_conf() and user != name):
        return None, None
    if action in SIGNIN_OWN and user != name and uid != 0:
        return None, None
    return action, user


def signin_prepare(uid, name, request):
    """Partie rapide, dans le relais : contrôles, mot de passe (avec la même
    temporisation que les autres vérifications), code PIN. None : la suite
    tourne dans un processus à part (signin_child)."""
    action, user = signin_target(uid, name, request)
    if action is None:
        return {"ok": False, "error": "failed"}
    if action in SIGNIN_PASSWORD:
        password = request.get("password", "")
        if not isinstance(password, str):
            return {"ok": False, "error": "password"}
        if password_ok(user, ""):
            if action in ("pin-set", "fp-enroll"):
                return {"ok": False, "error": "no-password"}
        else:
            throttle(uid)
            if not password_ok(user, password):
                FAILURES[uid] = FAILURES.get(uid, 0) + 1
                return {"ok": False, "error": "password"}
            FAILURES.pop(uid, None)
            pin_unblock(user)
    if action == "pin-check" and password_ok(user, ""):
        return {"ok": False, "error": "no-password"}
    pin = request.get("pin") if isinstance(request.get("pin"), str) else ""
    if action == "pin-check":
        return parental_gate(user, signin_run(action, user, pin))
    if action == "pin-set":
        return signin_run(action, user, pin)
    if action == "pin-remove":
        return signin_run(action, user)
    return None


def signin_child(connection, uid, name, request):
    action, user = signin_target(uid, name, request)
    if action == "status":
        answer = signin_run("status", user)
        if answer.get("ok"):
            answer["password"] = not password_ok(user, "")
        return answer
    if action == "fp-verify" and password_ok(user, ""):
        return {"ok": False, "error": "no-password"}
    if action == "fp-verify":
        return parental_gate(user, signin_stream(connection, action, user))
    if action == "fp-enroll":
        return signin_stream(connection, action, user)
    if action == "fp-delete":
        return signin_run(action, user)
    return {"ok": False, "error": "failed"}


def needs_child(request):
    if request.get("op") == "signin":
        return request.get("action") in SIGNIN_FORK
    return request.get("op") == "switch" and bool(request.get("finger"))


def effective_level(op, args, uid, name):
    """Niveau d'une demande « control » ou « exec » pour cet appelant, None si refusée."""
    if not isinstance(args, list) or not all(isinstance(a, str) for a in args):
        return None
    level = control_level(args) if op == "control" else exec_level(args)
    if level is None:
        return None
    if level == "admin-read":
        # Un administrateur consulte sans confirmation ; un autre compte
        # (le compte surveillé en premier) doit fournir le mot de passe d'un administrateur.
        level = "user" if uid == 0 or is_admin(name) else "admin"
    # Une sauvegarde ne concerne que le compte qui la demande.
    if op == "control" and args[0].startswith("backup-") and len(args) > 1 and args[1] != name:
        level = "admin"
    # Ouvrir ou fermer la session d'un autre compte : son mot de passe
    # passe par l'écran de verrouillage (op « switch »), sinon un administrateur.
    if op == "control" and args[0] in ("session-switch", "session-logoff") and len(args) > 1 and args[1] != name:
        level = "admin"
    # Compte surveillé : changer de fuseau horaire décalerait ses horaires.
    if op == "control" and re.match(r"(tz|timezone)-", args[0]) and supervised(name):
        level = "admin"
    return level


def handle(connection, uid, name, request, fds, admitted_here=False):
    op = request.get("op")
    if op in ("check", "elevate", "admin-check"):
        return check_request(uid, name, request)
    if op == "switch":
        return switch_request(uid, name, request, connection)
    if op == "signin":
        return signin_prepare(uid, name, request) or signin_child(connection, uid, name, request)
    if op == "conf":
        file, key, value = request.get("file"), request.get("key"), request.get("value", "")
        allowed = CONF_KEYS.get(file, False)
        if allowed is False or not isinstance(key, str) or not CONF_KEY.match(key) or \
                not isinstance(value, str) or not CONF_VALUE.match(value) or (allowed and key not in allowed):
            return {"error": "Réglage refusé"}
        if file in CONF_MACHINE and not admitted_here and not (is_admin(name) or elevated(uid, name)):
            return {"auth": "required"}
        write_conf(file, key, value)
        return {"status": 0}
    if op in ("control", "exec"):
        args = request.get("args")
        if not isinstance(args, list) or not all(isinstance(a, str) for a in args):
            return {"error": "Demande invalide"}
        level = effective_level(op, args, uid, name)
        if level is None:
            return {"error": "Action non autorisée : " + (args[0] if args else "?")}
        if level == "admin" and not admitted_here and not elevated(uid, name):
            return {"auth": "required"}
        if len(fds) < 3:
            return {"error": "Descripteurs manquants"}
        extra = request.get("fds", [])
        if not isinstance(extra, list) or len(extra) != len(fds) - 3 or \
                not all(isinstance(n, int) and 3 <= n < 256 for n in extra):
            return {"error": "Descripteurs invalides"}
        env = root_env(request, name)
        if op == "control":
            command = [CONTROL, *args]
        else:
            tool = PARENTAL if args[0] == "winlux-parental" else shutil.which(args[0], path=env["PATH"])
            if not tool:
                return {"status": 127}
            command = [tool, *args[1:]]
            if args[0] == "winlux-parental":
                command = parental_command(*args[1:])
                env.update({key: value for key, value in parental_env().items() if key != "PATH"})
            env["DEBIAN_FRONTEND"] = "noninteractive"
        # Dossier courant du client (« share-add » sans chemin, Explorateur).
        cwd = request.get("cwd")
        if not isinstance(cwd, str) or not cwd.startswith("/") or not os.path.isdir(cwd):
            cwd = "/"
        try:
            return {"status": run_command(command, fds, extra, env, uid, cwd)}
        except OSError as error:
            return {"error": str(error)}
    return {"error": "Opération inconnue"}


def serve():
    if os.geteuid() != 0 and not ROOT:
        print("winlux-root serve doit tourner en root", file=sys.stderr)
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
    signal.signal(signal.SIGCHLD, signal.SIG_IGN)
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    # Le socket n'apparaît sous son vrai nom qu'une fois à l'écoute (#693) :
    # un client qui le voyait entre bind() et listen() recevait « connexion
    # refusée » (CI de test-accounts.py, bascule au démarrage de session).
    pending = SOCKET + ".new"
    try:
        os.unlink(pending)
    except FileNotFoundError:
        pass
    old = os.umask(0o111)
    server.bind(pending)
    os.umask(old)
    os.chmod(pending, 0o666)
    server.listen(16)
    os.replace(pending, SOCKET)
    while True:
        try:
            connection, _ = server.accept()
        except InterruptedError:
            continue
        with connection:
            try:
                credentials = connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i"))
                _pid, uid, _gid = struct.unpack("3i", credentials)
                connection.settimeout(10)
                message, fds = receive(connection)
                name = desktop_account(uid)
                # Tests seulement (racine factice) : appelant simulé « uid:nom ».
                if ROOT and os.environ.get("WINLUX_ROOT_TEST_PEER"):
                    uid_text, name = os.environ["WINLUX_ROOT_TEST_PEER"].split(":", 1)
                    uid = int(uid_text)
                    name = name if name in users_conf() else None
                if name is None:
                    reply(connection, {"error": "Accès refusé"})
                    close_all(fds)
                    continue
                request = json.loads(message or "{}")
                if not isinstance(request, dict):
                    raise ValueError("demande invalide")
                if request.get("op") == "signin":
                    # Contrôles et mot de passe ici (la temporisation des échecs
                    # vit dans ce processus) ; l'attente du doigt à part.
                    answer = signin_prepare(uid, name, request)
                    if answer is not None:
                        reply(connection, answer)
                        close_all(fds)
                        continue
                if needs_child(request):
                    close_all(fds)
                    if os.fork() == 0:
                        try:
                            signal.signal(signal.SIGCHLD, signal.SIG_DFL)
                            server.close()
                            connection.settimeout(None)
                            if request.get("op") == "signin":
                                reply(connection, signin_child(connection, uid, name, request))
                            else:
                                reply(connection, switch_request(uid, name, request, connection))
                        finally:
                            os._exit(0)
                    continue
                if request.get("op") in ("control", "exec"):
                    # Les commandes longues (APT) tournent à part ; l'élévation
                    # est décidée ici, dans le processus qui la mémorise.
                    args = request.get("args") if isinstance(request.get("args"), list) else []
                    level = effective_level(request.get("op"), args, uid, name) if args else None
                    if level is not None and not admitted(request.get("op"), args, uid, name, level):
                        reply(connection, {"auth": "required"})
                        close_all(fds)
                        continue
                    if os.fork() == 0:
                        try:
                            signal.signal(signal.SIGCHLD, signal.SIG_DFL)
                            server.close()
                            connection.settimeout(None)
                            reply(connection, handle(connection, uid, name, request, fds, level is not None))
                        finally:
                            os._exit(0)
                    close_all(fds)
                    continue
                reply(connection, handle(connection, uid, name, request, fds))
                close_all(fds)
            except (OSError, ValueError) as error:
                try:
                    reply(connection, {"error": str(error)})
                except OSError:
                    pass


def receive(connection):
    fds = array.array("i")
    data = b""
    received = []
    while not data.endswith(b"\n"):
        chunk, ancdata, _flags, _addr = connection.recvmsg(65536, socket.CMSG_SPACE(16 * fds.itemsize))
        for level, kind, payload in ancdata:
            if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                part = array.array("i")
                part.frombytes(payload[:len(payload) - (len(payload) % part.itemsize)])
                received.extend(part)
        if not chunk:
            break
        data += chunk
        if len(data) > 65536:
            raise ValueError("demande trop longue")
    return data.decode("utf-8", errors="replace").strip(), received


def reply(connection, answer):
    connection.sendall((json.dumps(answer, ensure_ascii=False) + "\n").encode())


def close_all(fds):
    for fd in fds:
        try:
            os.close(fd)
        except OSError:
            pass


# ── Client ──────────────────────────────────────────────────────────────────

def request(payload, fds=()):
    """Envoie une demande au relais ; None s'il ne répond pas."""
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.connect(SOCKET)
            data = (json.dumps(payload) + "\n").encode()
            if fds:
                client.sendmsg([data], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", fds))])
            else:
                client.sendall(data)
            answer = b""
            while not answer.endswith(b"\n"):
                chunk = client.recv(4096)
                if not chunk:
                    break
                answer += chunk
    except OSError:
        return None
    try:
        return json.loads(answer.decode("utf-8", errors="replace") or "null")
    except ValueError:
        return None


def available():
    return os.path.exists(SOCKET)


def ask_elevation():
    """Contrôle de compte d'utilisateur ; True si un administrateur a confirmé."""
    uac = shutil.which("winlux-uac") or "/usr/bin/winlux-uac"
    app = os.environ.get("WINLUX_ROOT_APP", "Paramètres Winlux")
    try:
        return subprocess.run([uac, "--elevate", "--app", app]).returncode == 0
    except OSError:
        return False


def cwd():
    try:
        return os.getcwd()
    except OSError:
        return "/"


def run_relayed(op, args):
    extra = []
    for arg in args:
        match = re.fullmatch(r"(?:-o)?APT::Status-Fd=([0-9]+)", arg)
        if match:
            extra.append(int(match.group(1)))
    payload = {"op": op, "args": args, "display": os.environ.get("DISPLAY", ""),
               "lang": os.environ.get("LANG", ""), "fds": extra, "cwd": cwd()}
    for attempt in range(2):
        answer = request(payload, [0, 1, 2, *extra])
        if answer is None:
            print("winlux-root : le service système ne répond pas", file=sys.stderr)
            return 125
        if answer.get("auth") == "required" and attempt == 0 and os.environ.get("DISPLAY"):
            if not ask_elevation():
                print("Opération annulée : confirmation d'un administrateur refusée.", file=sys.stderr)
                return 1223 % 256
            continue
        if answer.get("auth") == "required":
            print("Droits administrateur requis.", file=sys.stderr)
            return 1
        if "error" in answer:
            print(answer["error"], file=sys.stderr)
            return 1
        return int(answer.get("status", 1))
    return 1


def verify(op, user, password):
    if os.geteuid() == 0 and op == "check" and not ROOT:
        return password_ok(user, password)
    answer = request({"op": op, "user": user, "password": password})
    return bool(answer and answer.get("ok"))


def main(argv):
    args = argv[1:]
    if not args:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    command, rest = args[0], args[1:]
    if command == "serve":
        return serve()
    if command in ("control", "exec"):
        if os.geteuid() == 0 and command == "control" and not ROOT:
            os.execv(CONTROL, [CONTROL, *rest])
        return run_relayed(command, rest)
    if command == "conf" and len(rest) == 3:
        for attempt in range(2):
            answer = request({"op": "conf", "file": rest[0], "key": rest[1], "value": rest[2]})
            if answer is None:
                print("winlux-root : le service système ne répond pas", file=sys.stderr)
                return 125
            # Réglage de l'appareil demandé par un compte standard (#2269).
            if answer.get("auth") == "required" and attempt == 0 and os.environ.get("DISPLAY"):
                if not ask_elevation():
                    print("Opération annulée : confirmation d'un administrateur refusée.", file=sys.stderr)
                    return 1223 % 256
                continue
            if answer.get("auth") == "required":
                print("Droits administrateur requis.", file=sys.stderr)
                return 1
            if "error" in answer:
                print(answer["error"], file=sys.stderr)
                return 1
            return int(answer.get("status", 1))
        return 1
    if command in ("check", "elevate") and len(rest) == 1:
        password = sys.stdin.readline().rstrip("\n")
        return 0 if verify(command, rest[0], password) else 1
    if command == "policy" and rest:
        level = control_level(rest) or exec_level(rest)
        if level == "admin-read":
            level = "admin"
        print(level or "refusé")
        return 0 if level else 1
    print(__doc__.strip(), file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
