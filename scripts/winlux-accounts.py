#!/usr/bin/python3
"""Comptes locaux de Winlux (Paramètres › Comptes), comme sous Windows 11.

    winlux-accounts list                         comptes du bureau (lecture seule)
    winlux-accounts add NOM admin|user [NOM COMPLET]   mot de passe sur l'entrée standard
    winlux-accounts password NOM                 nouveau mot de passe sur l'entrée standard
    winlux-accounts role NOM admin|user          administrateur ou standard
    winlux-accounts remove NOM keep|delete       garde ou efface le dossier personnel

Comptes locaux uniquement : aucun compte en ligne. Les actions qui modifient
un compte tournent en root, par `winlux-control account-*` relayé par
winlux-root après le Contrôle de compte d'utilisateur (mot de passe d'un
administrateur). /etc/winlux/users.conf (écran de verrouillage, partages,
relais root) suit chaque changement. Un mot de passe vide donne un compte
sans mot de passe, ouvert d'un clic.

Tests : WINLUX_ACCOUNTS_ROOT (racine factice : etc/passwd, etc/group,
etc/winlux/users.conf) et WINLUX_ACCOUNTS_LOG (les commandes système y sont
notées au lieu d'être lancées).
"""
import os
import re
import subprocess
import sys

ROOT = os.environ.get("WINLUX_ACCOUNTS_ROOT", "")
LOG = os.environ.get("WINLUX_ACCOUNTS_LOG", "")
USERS_CONF = ROOT + "/etc/winlux/users.conf"
NAME = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
FULLNAME = re.compile(r"^[^:\n\r\0,=]{0,64}$")
ADMIN_GROUPS = ("sudo", "wheel", "admin")
BASE_GROUPS = ("audio", "video", "plugdev", "input", "netdev", "bluetooth", "lpadmin", "scanner")
RESERVED = {"root", "winlux-web", "nobody", "daemon", "bin", "sys", "admin", "administrateur", "administrator"}
FOLDERS = ("Bureau", "Documents", "Images", "Musique", "Téléchargements", "Vidéos")


class AccountError(Exception):
    pass


def read_lines(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            return handle.read().splitlines()
    except OSError:
        return []


def groups():
    """{groupe: [membres]} de /etc/group."""
    out = {}
    for line in read_lines(ROOT + "/etc/group"):
        parts = line.split(":")
        if len(parts) >= 4:
            out[parts[0]] = [member for member in parts[3].split(",") if member]
    return out


def users_conf():
    out = {}
    for line in read_lines(USERS_CONF):
        parts = line.split(":", 2)
        if len(parts) == 3 and parts[0]:
            out[parts[0]] = (parts[1], parts[2])
    return out


def accounts():
    """Comptes du bureau : [{name, fullname, admin, home, uid}] triés par nom."""
    members = groups()
    admins = {name for group in ADMIN_GROUPS for name in members.get(group, [])}
    conf = users_conf()
    out = []
    for line in read_lines(ROOT + "/etc/passwd"):
        parts = line.split(":")
        if len(parts) < 7:
            continue
        try:
            uid = int(parts[2])
        except ValueError:
            continue
        if not 1000 <= uid < 60000 or parts[0] == "winlux-web" or parts[6].endswith(("nologin", "false")):
            continue
        role = conf.get(parts[0], ("", ""))[0]
        out.append({"name": parts[0], "fullname": parts[4].split(",")[0], "uid": uid, "home": parts[5],
                    "admin": parts[0] in admins or role in ("root", "admin", "administrateur")})
    return sorted(out, key=lambda account: account["name"])


def run(args, stdin=None):
    if LOG:
        with open(LOG, "a", encoding="utf-8") as log:
            log.write(" ".join(args) + ("" if stdin is None else " <mot de passe>") + "\n")
        return
    try:
        result = subprocess.run(args, input=stdin, text=True, capture_output=True, timeout=120)
    except (OSError, subprocess.SubprocessError) as error:
        raise AccountError(f"{args[0]} : {error}") from error
    if result.returncode:
        raise AccountError((result.stderr or result.stdout).strip() or f"{args[0]} : code {result.returncode}")


def write_users_conf(entries):
    """Réécrit users.conf atomiquement (lisible par tous : aucun mot de passe en clair)."""
    os.makedirs(os.path.dirname(USERS_CONF), exist_ok=True)
    temporary = USERS_CONF + ".new"
    with open(temporary, "w", encoding="utf-8") as handle:
        for name, (role, secret) in entries.items():
            handle.write(f"{name}:{role}:{secret}\n")
    os.chmod(temporary, 0o644)
    os.replace(temporary, USERS_CONF)


def set_conf_role(name, admin):
    entries = users_conf()
    entries[name] = ("root" if admin else "user", "shadow")
    write_users_conf(entries)


def find(name):
    return next((account for account in accounts() if account["name"] == name), None)


def check_name(name):
    if not NAME.match(name or "") or name in RESERVED:
        raise AccountError("Nom de compte invalide : lettres minuscules, chiffres, - et _ (32 au plus).")


def admin_count(excluding=None):
    return sum(1 for account in accounts() if account["admin"] and account["name"] != excluding)


def read_password():
    """Première ligne de l'entrée standard (vide : compte sans mot de passe)."""
    line = sys.stdin.readline()
    password = line.rstrip("\n")
    if "\0" in password or "\r" in password or len(password) > 256:
        raise AccountError("Mot de passe invalide.")
    return password


def apply_password(name, password):
    if password:
        run(["chpasswd"], f"{name}:{password}\n")
    else:
        run(["passwd", "-d", name])
    # Les partages réseau (Samba) suivent le mot de passe de session.
    if password and os.path.exists("/usr/bin/winlux-share") and not LOG:
        try:
            subprocess.run(["/usr/bin/winlux-share", "set-password", name], input=password + "\n", text=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
        except (OSError, subprocess.SubprocessError):
            pass


def add(name, role, fullname=""):
    check_name(name)
    if role not in ("admin", "user"):
        raise AccountError("Type de compte inconnu : admin ou user.")
    if not FULLNAME.match(fullname):
        raise AccountError("Nom complet invalide.")
    if find(name) or any(line.split(":")[0] == name for line in read_lines(ROOT + "/etc/passwd")):
        raise AccountError(f"Le compte {name} existe déjà.")
    password = read_password()
    existing = groups()
    wanted = [group for group in BASE_GROUPS if group in existing]
    if role == "admin":
        wanted.append(next((group for group in ADMIN_GROUPS if group in existing), "sudo"))
    args = ["useradd", "-m", "-s", "/bin/bash", "-c", fullname or name]
    if wanted:
        args += ["-G", ",".join(wanted)]
    run(args + [name])
    apply_password(name, password)
    if not LOG:
        home = os.path.expanduser("~" + name)
        for folder in FOLDERS:
            run(["runuser", "-u", name, "--", "mkdir", "-p", os.path.join(home, folder)])
    set_conf_role(name, role == "admin")


def password(name):
    if not find(name):
        raise AccountError(f"Compte inconnu : {name}")
    apply_password(name, read_password())


def role(name, kind):
    account = find(name)
    if not account:
        raise AccountError(f"Compte inconnu : {name}")
    if kind not in ("admin", "user"):
        raise AccountError("Type de compte inconnu : admin ou user.")
    if kind == "user" and account["admin"] and admin_count(excluding=name) == 0:
        raise AccountError("Il faut garder au moins un compte administrateur.")
    existing = groups()
    for group in ADMIN_GROUPS:
        if group not in existing:
            continue
        if kind == "admin" and group == next(g for g in ADMIN_GROUPS if g in existing):
            run(["gpasswd", "-a", name, group])
        elif kind == "user" and name in existing[group]:
            run(["gpasswd", "-d", name, group])
    set_conf_role(name, kind == "admin")


def session_open(name):
    return os.path.exists(f"{ROOT}/run/winlux/sessions/{name}")


def remove(name, files):
    account = find(name)
    if not account:
        raise AccountError(f"Compte inconnu : {name}")
    if files not in ("keep", "delete"):
        raise AccountError("Choisissez keep ou delete pour le dossier personnel.")
    if account["admin"] and admin_count(excluding=name) == 0:
        raise AccountError("Il faut garder au moins un compte administrateur.")
    if session_open(name):
        raise AccountError(f"{name} a une session ouverte : fermez-la d’abord (Changer d’utilisateur).")
    run(["userdel"] + (["-r"] if files == "delete" else []) + [name])
    entries = users_conf()
    entries.pop(name, None)
    write_users_conf(entries)


def main(argv):
    if not argv:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    command, rest = argv[0], argv[1:]
    try:
        if command == "list":
            for account in accounts():
                print(f"{account['name']}:{'admin' if account['admin'] else 'user'}:{account['fullname']}")
        elif command == "add" and len(rest) in (2, 3):
            add(rest[0], rest[1], rest[2] if len(rest) == 3 else "")
        elif command == "password" and len(rest) == 1:
            password(rest[0])
        elif command == "role" and len(rest) == 2:
            role(rest[0], rest[1])
        elif command == "remove" and len(rest) == 2:
            remove(rest[0], rest[1])
        else:
            print(__doc__.strip(), file=sys.stderr)
            return 2
    except AccountError as error:
        print(f"winlux-accounts : {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"winlux-accounts : {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
