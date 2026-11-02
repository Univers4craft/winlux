#!/usr/bin/python3
"""Relais root de Winlux : le bureau tourne sous le compte de l'utilisateur.

Sur un système installé, le bureau n'est plus root. Les rares opérations qui
touchent au système (réglages dans /etc, Boutique et APT, mises à jour,
volumes, partages, bureau à distance…) passent par ce relais unique, qui
tourne en root et n'exécute que des actions connues :

  winlux-root serve                       (root) écoute /run/winlux-root.sock
  winlux-root control ACTION [ARGS…]      action de winlux-control, exécutée en root
  winlux-root exec apt-get|snap ARGS…     installation de logiciels (Boutique)
  winlux-root conf FICHIER CLÉ VALEUR     préférence de /etc/winlux (sans confirmation)
  winlux-root check COMPTE                mot de passe lu sur l'entrée standard
  winlux-root elevate COMPTE              idem, et ouvre 5 minutes d'élévation
  winlux-root policy ACTION               user | admin | refusé (diagnostic)

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
    (r"browser-(install|default)-(edge|chrome|firefox)", "admin"),
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
    (r"display-[a-z0-9x-]+", "user"),
    (r"power-hibernate-setup", "admin"),
    (r"power-[a-z-]+", "user"),
    (r"update-(check|apply|auto-on|auto-off)", "user"),
    (r"update-rollback", "admin"),
    (r"sysupdate-[a-z-]+", "user"),
    (r"journal-(status|list|errors|crashes|export)", "user"),
    (r"journal-(clear|rotate)", "admin"),
    (r"restore-(list|status)", "user"),
    (r"restore-(create|restore|delete)", "admin"),
    (r"backup-(status|request|list|prune|drives|run)", "user"),
    (r"session-(register|unregister|switch|logoff|list|display)", "user"),
    (r"remote-[a-z-]+", "admin"),
    (r"share-(status|list|state|ensure)", "user"),
    (r"share-[a-z-]+", "admin"),
    (r"transfer-[a-z-]+", "user"),
    (r"report-(offer|redact|mode)-[a-z]+", "user"),
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
CONF_VALUE = re.compile(r"^[A-Za-z0-9._/+-]{0,64}$")
CONF_KEY = re.compile(r"^[A-Z][A-Z0-9_]{0,40}$")

APT_SUBCOMMANDS = {"install": "admin", "remove": "admin", "purge": "admin", "upgrade": "admin",
                   "dist-upgrade": "admin", "full-upgrade": "admin", "autoremove": "admin",
                   "update": "user"}
APT_FLAGS = {"-y", "-q", "-qq", "--no-install-recommends", "--reinstall", "-f", "--fix-broken",
             "--only-upgrade", "--no-remove"}
APT_OPTIONS = re.compile(r"^(DPkg::Lock::Timeout=[0-9]{1,4}|Dpkg::Use-Pty=0|APT::Status-Fd=[0-9]{1,3})$")
SNAP_SUBCOMMANDS = {"install": "admin", "remove": "admin", "refresh": "admin"}


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
    if tool == "snap" and rest and rest[0] in SNAP_SUBCOMMANDS:
        if all(PACKAGE.match(a) or a in ("--classic",) for a in rest[1:]):
            return SNAP_SUBCOMMANDS[rest[0]]
    return None


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
    if request.get("op") == "elevate" and not is_admin(user):
        return {"ok": False}
    throttle(uid)
    ok = password_ok(user, password)
    if ok:
        FAILURES.pop(uid, None)
        if request.get("op") == "elevate":
            ELEVATED[uid] = time.monotonic() + ELEVATION_SECONDS
        sync_share_password(user, password)
    else:
        FAILURES[uid] = FAILURES.get(uid, 0) + 1
    return {"ok": ok}


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
GIVE_BACK = ("updates.txt", "networks.txt", "cpu.txt", "control.txt", "action.txt", "sessions.txt",
             "update-error-phase", "browser.log", "sysupdate.txt", "remote-state.txt")


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


def handle(connection, uid, name, request, fds):
    op = request.get("op")
    if op in ("check", "elevate"):
        return check_request(uid, name, request)
    if op == "conf":
        file, key, value = request.get("file"), request.get("key"), request.get("value", "")
        allowed = CONF_KEYS.get(file, False)
        if allowed is False or not isinstance(key, str) or not CONF_KEY.match(key) or \
                not isinstance(value, str) or not CONF_VALUE.match(value) or (allowed and key not in allowed):
            return {"error": "Réglage refusé"}
        write_conf(file, key, value)
        return {"status": 0}
    if op in ("control", "exec"):
        args = request.get("args")
        if not isinstance(args, list) or not all(isinstance(a, str) for a in args):
            return {"error": "Demande invalide"}
        level = control_level(args) if op == "control" else exec_level(args)
        if level is None:
            return {"error": "Action non autorisée : " + (args[0] if args else "?")}
        # Une sauvegarde ne concerne que le compte qui la demande.
        if op == "control" and args[0].startswith("backup-") and len(args) > 1 and args[1] != name:
            level = "admin"
        if level == "admin" and not elevated(uid, name):
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
            tool = shutil.which(args[0], path=env["PATH"])
            if not tool:
                return {"status": 127}
            command = [tool, *args[1:]]
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
    old = os.umask(0o111)
    server.bind(SOCKET)
    os.umask(old)
    os.chmod(SOCKET, 0o666)
    server.listen(16)
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
                if request.get("op") in ("control", "exec"):
                    # Les commandes longues (APT) tournent à part ; l'élévation
                    # est décidée ici, dans le processus qui la mémorise.
                    args = request.get("args") if isinstance(request.get("args"), list) else []
                    level = (control_level(args) if request.get("op") == "control" else exec_level(args)) \
                        if all(isinstance(a, str) for a in args) else None
                    if level == "admin" and not elevated(uid, name):
                        reply(connection, {"auth": "required"})
                        close_all(fds)
                        continue
                    if request.get("op") == "control" and args and args[0].startswith("backup-") and \
                            len(args) > 1 and args[1] != name and not elevated(uid, name):
                        reply(connection, {"auth": "required"})
                        close_all(fds)
                        continue
                    if os.fork() == 0:
                        try:
                            signal.signal(signal.SIGCHLD, signal.SIG_DFL)
                            server.close()
                            connection.settimeout(None)
                            reply(connection, handle(connection, uid, name, request, fds))
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
        answer = request({"op": "conf", "file": rest[0], "key": rest[1], "value": rest[2]})
        if answer is None:
            print("winlux-root : le service système ne répond pas", file=sys.stderr)
            return 125
        if "error" in answer:
            print(answer["error"], file=sys.stderr)
            return 1
        return int(answer.get("status", 1))
    if command in ("check", "elevate") and len(rest) == 1:
        password = sys.stdin.readline().rstrip("\n")
        return 0 if verify(command, rest[0], password) else 1
    if command == "policy" and rest:
        level = control_level(rest) or exec_level(rest)
        print(level or "refusé")
        return 0 if level else 1
    print(__doc__.strip(), file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
