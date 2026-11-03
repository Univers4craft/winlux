#!/usr/bin/python3
"""Réparation des fichiers de comptes (/etc/passwd, shadow, group, gshadow).

Lancé par l'init à chaque démarrage d'un système installé, avant de choisir
le compte du bureau. Après une coupure de courant ou une remise à zéro juste
après l'installation, ext4 peut garder la taille d'un fichier sans son
contenu : /etc/passwd contenait alors des octets nuls à la place de la ligne
du compte, le bureau ne trouvait plus l'utilisateur et winlux-native-config
empilait des comptes système à moitié créés (UID 1002, 1003, 1004…).

Ce que fait l'outil, sans rien toucher quand tout est sain :
- supprime les verrous et fichiers temporaires de shadow-utils laissés par
  un arrêt brutal (passwd.lock, passwd+, .pwd.lock…) ;
- retire les octets nuls, les lignes mal formées et les doublons de nom ;
- quand un fichier était abîmé, reprend les comptes perdus dans sa copie
  de sauvegarde (passwd-, shadow-…, écrites par l'installateur) ;
- recrée le compte choisi à l'installation (install-config.json) s'il
  manque encore, avec l'UID du dossier personnel ; sans sauvegarde du mot
  de passe, le compte reste verrouillé et l'ouverture automatique le sert.
Chaque fichier corrigé est réécrit par fichier temporaire + fsync + rename.
"""
import json
import os
import re
import sys
import time

FIELDS = {"passwd": 7, "group": 4, "shadow": 9, "gshadow": 4}
NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_.-]*\$?$")
LOCKS = (".pwd.lock", "passwd.lock", "shadow.lock", "group.lock", "gshadow.lock",
         "subuid.lock", "subgid.lock", "passwd+", "shadow+", "group+", "gshadow+",
         "ptmp", "stmp", "gtmp", "sgtmp")


def log(message):
    print(f"winlux-accounts-repair : {message}", flush=True)


def write_atomic(path, text, like=None):
    """Remplace path par text sans jamais laisser un fichier à moitié écrit."""
    reference = like or path
    try:
        info = os.stat(reference)
        mode, uid, gid = info.st_mode & 0o7777, info.st_uid, info.st_gid
    except OSError:
        mode, uid, gid = (0o640 if os.path.basename(path).startswith(("shadow", "gshadow")) else 0o644), 0, 0
    directory = os.path.dirname(path) or "."
    temporary = os.path.join(directory, f".{os.path.basename(path)}.winlux-{os.getpid()}")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", errors="surrogateescape") as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        try:
            os.chown(temporary, uid, gid)
        except OSError:
            pass
        os.chmod(temporary, mode)
        os.rename(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise
    try:
        handle = os.open(directory, os.O_RDONLY)
        try:
            os.fsync(handle)
        finally:
            os.close(handle)
    except OSError:
        pass


def valid(kind, fields):
    if len(fields) != FIELDS[kind] or not NAME.match(fields[0]):
        return False
    if kind == "passwd":
        return fields[2].isdigit() and fields[3].isdigit()
    if kind == "group":
        return fields[2].isdigit()
    return True


def parse(kind, raw):
    """(lignes valides, abîmé ?) ; une ligne par nom, la première gardée."""
    text = raw.decode("utf-8", "surrogateescape")
    damaged = "\0" in text
    text = text.replace("\0", "\n")
    entries, names = [], set()
    for line in text.split("\n"):
        if not line.strip():
            continue
        fields = line.split(":")
        if not valid(kind, fields):
            damaged = True
            continue
        if fields[0] in names:
            damaged = True
            continue
        names.add(fields[0])
        entries.append(fields)
    if raw and not raw.endswith(b"\n"):
        damaged = True
    return entries, damaged


def read(path):
    try:
        with open(path, "rb") as stream:
            return stream.read()
    except OSError:
        return None


def render(entries):
    return "".join(":".join(fields) + "\n" for fields in entries)


def repair(etc="/etc", home="/home", config=None):
    """Répare les fichiers de comptes de etc ; renvoie la liste des corrections."""
    fixes = []
    for name in LOCKS:
        path = os.path.join(etc, name)
        if os.path.lexists(path):
            try:
                os.unlink(path)
                fixes.append(f"verrou supprimé : {path}")
            except OSError as error:
                log(f"{path} : {error}")
    tables = {}
    for kind in FIELDS:
        path = os.path.join(etc, kind)
        raw = read(path)
        if raw is None:
            continue
        entries, damaged = parse(kind, raw)
        if damaged:
            backup, _ = parse(kind, read(path + "-") or b"")
            present = {fields[0] for fields in entries}
            restored = [fields for fields in backup if fields[0] not in present]
            if kind in ("passwd", "group"):
                # Un identifiant déjà pris dans le fichier courant n'est pas
                # repris : la ligne courante (plus récente) l'emporte.
                taken = {fields[2] for fields in entries}
                restored = [fields for fields in restored if fields[2] not in taken]
            entries += restored
            fixes.append(f"{path} réparé ({len(restored)} compte(s) repris de {kind}-)")
        tables[kind] = [entries, damaged, path]
    restore_install_account(tables, etc, home, config, fixes)
    for kind, (entries, damaged, path) in tables.items():
        if damaged:
            write_atomic(path, render(entries))
    return fixes


def restore_install_account(tables, etc, home, config, fixes):
    """Recrée le compte de l'installation s'il a disparu de passwd/shadow."""
    if "passwd" not in tables:
        return
    config = config or os.path.join(etc, "winlux/install-config.json")
    try:
        with open(config, encoding="utf-8") as stream:
            settings = json.load(stream)
    except (OSError, ValueError):
        return
    username = settings.get("username") or ""
    if not NAME.match(username):
        return
    passwd = tables["passwd"]
    if any(fields[0] == username for fields in passwd[0]):
        return
    used = {int(fields[2]) for fields in passwd[0]}
    groups = tables.get("group", [[], False, ""])[0]
    try:
        uid = os.stat(os.path.join(home, username)).st_uid
    except OSError:
        uid = 1000
    if uid < 1000 or uid in used:
        uid = 1000
        while uid in used:
            uid += 1
    gid = next((fields[2] for fields in groups if fields[0] == username), str(uid))
    fullname = str(settings.get("fullname") or username).replace(":", " ").replace("\n", " ")
    passwd[0].append([username, "x", str(uid), gid, fullname, f"/home/{username}", "/bin/bash"])
    passwd[1] = True
    fixes.append(f"compte {username} recréé dans passwd (UID {uid})")
    if "shadow" in tables and not any(fields[0] == username for fields in tables["shadow"][0]):
        days = str(int(time.time() // 86400))
        tables["shadow"][0].append([username, "!", days, "0", "99999", "7", "", "", ""])
        tables["shadow"][1] = True
        fixes.append(f"compte {username} sans mot de passe enregistré : verrouillé (ouverture automatique)")


def main(argv):
    etc = "/etc"
    if len(argv) > 2 and argv[1] == "--root":
        etc = os.path.join(argv[2], "etc")
        home = os.path.join(argv[2], "home")
    else:
        home = "/home"
    try:
        fixes = repair(etc, home)
    except Exception as error:  # jamais d'échec du démarrage pour ça
        log(f"échec : {error}")
        return 1
    for fix in fixes:
        log(fix)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
