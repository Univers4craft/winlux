#!/usr/bin/python3 -I
"""Retire le dépôt APT qu'un logiciel d'éditeur a déclaré (Store, #1056).

    winlux-store-repos clean <id>
    winlux-store-repos check <id>
    winlux-store-repos grant <id> <sha256>
    winlux-store-repos install <id> <sha256> <fichier.deb> [APT::Status-Fd=N]
    winlux-store-repos revoke <id>

« grant » (et l'ancien « check ») ne modifie rien : le Store le lance en
administrateur juste après la confirmation, pour que le Contrôle de compte
d'utilisateur vienne avant le téléchargement (#1451). Le relais winlux-root en
garde un accord à usage unique, lié au compte, à ce logiciel et à cette
empreinte, valable une heure (#1453) : « install » passe ensuite sans
redemander, même après un long téléchargement. « revoke » retire l'accord
(téléchargement annulé ou échoué). « install » recopie le paquet dans un
dossier de root, revérifie l'empreinte et le nom du paquet, puis lance
apt-get install sur cette copie seulement.

Lancé en administrateur (relais winlux-root) après la purge du paquet. Les
adresses autorisées viennent de /usr/share/winlux/store-editeurs.json (champ
« hosts ») : un fichier de /etc/apt/sources.list.d n'est supprimé que si
toutes ses sources pointent vers l'éditeur, et une clé (signed-by) seulement
si plus aucune source ne s'en sert. Refus tant que le paquet est installé."""
import json
import os
import hashlib
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.parse
from pathlib import Path

ROOT = os.environ.get("WINLUX_CONTROL_ROOT", "")
DEFS = Path(os.environ.get("WINLUX_STORE_EDITEURS", ROOT + "/usr/share/winlux/store-editeurs.json"))
SOURCES = Path(ROOT + "/etc/apt/sources.list.d")
KEY_DIRS = ("/usr/share/keyrings/", "/etc/apt/keyrings/", "/etc/apt/trusted.gpg.d/")


def sources_of(text, suffix):
    """(adresses, clés signed-by) d'un fichier .list ou .sources."""
    uris, keys = [], []
    if suffix == ".sources":
        for line in text.splitlines():
            key, _, value = line.partition(":")
            if key.strip().lower() == "uris":
                uris += value.split()
            elif key.strip().lower() == "signed-by" and value.strip().startswith("/"):
                keys.append(value.strip().split()[0])
        return uris, keys
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if not re.match(r"^deb(-src)?\s", line):
            continue
        rest = line.split(None, 1)[1]
        if rest.startswith("["):
            options, _, rest = rest[1:].partition("]")
            for option in options.split():
                if option.startswith("signed-by="):
                    keys += [k for k in option[10:].split(",") if k.startswith("/")]
        if rest.split():
            uris.append(rest.split()[0])
    return uris, keys


def installed(package):
    try:
        out = subprocess.run(["dpkg-query", "-W", "-f", "${db:Status-Abbrev}", package],
                             capture_output=True, text=True, timeout=30).stdout
    except (OSError, subprocess.SubprocessError):
        return False
    return out.startswith("ii")


def clean(ident):
    entry = next((e for e in json.loads(DEFS.read_text()).get("apps", []) if e.get("id") == ident), None)
    if entry is None:
        print(f"logiciel inconnu : {ident}", file=sys.stderr)
        return 2
    if installed(entry["package"]):
        print(f"{entry['package']} est encore installé : dépôt gardé", file=sys.stderr)
        return 3
    hosts = set(entry.get("hosts", []))
    files = sorted(p for p in SOURCES.glob("*") if p.suffix in (".list", ".sources") and p.is_file()) \
        if SOURCES.is_dir() else []
    removed, freed = [], set()
    for path in files:
        uris, keys = sources_of(path.read_text(errors="replace"), path.suffix)
        if uris and all((urllib.parse.urlparse(u).hostname or "") in hosts for u in uris):
            path.unlink()
            removed.append(str(path))
            freed.update(keys)
    still = set()
    for path in (p for p in files if p.exists()):
        still.update(sources_of(path.read_text(errors="replace"), path.suffix)[1])
    for key in sorted(freed - still):
        if key.startswith(KEY_DIRS) and ".." not in key.split("/"):
            target = Path(ROOT + key)
            if target.is_file():
                target.unlink()
                removed.append(str(target))
    for line in removed:
        print(f"retiré : {line}")
    return 0


def check(ident):
    known = any(e.get("id") == ident for e in json.loads(DEFS.read_text()).get("apps", []))
    if not known:
        print(f"logiciel inconnu : {ident}", file=sys.stderr)
        return 2
    return 0


IDENT = re.compile(r"[a-z0-9][a-z0-9-]{0,40}")
SHA = re.compile(r"[0-9a-f]{64}")
STATUS_FD = re.compile(r"APT::Status-Fd=([0-9]{1,3})")
WORK = Path(ROOT + "/var/cache/winlux-store")


def install(ident, sha, source, status=None):
    entry = next((e for e in json.loads(DEFS.read_text()).get("apps", []) if e.get("id") == ident), None)
    if entry is None:
        print(f"logiciel inconnu : {ident}", file=sys.stderr)
        return 2
    # Copie hors de portée du compte : le fichier vérifié est celui qu'apt lit.
    WORK.mkdir(mode=0o755, parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="editeur-", dir=WORK))
    target = work / f"{entry['package']}.deb"
    digest = hashlib.sha256()
    try:
        try:
            fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        except OSError as error:
            print(f"paquet illisible : {error.strerror}", file=sys.stderr)
            return 7
        with os.fdopen(fd, "rb") as src:
            if not stat.S_ISREG(os.fstat(src.fileno()).st_mode):
                print("paquet : pas un fichier ordinaire", file=sys.stderr)
                return 7
            with open(target, "wb") as out:
                for block in iter(lambda: src.read(1 << 20), b""):
                    digest.update(block)
                    out.write(block)
        if digest.hexdigest() != sha:
            print("empreinte SHA-256 différente : paquet refusé", file=sys.stderr)
            return 5
        try:
            package = subprocess.run(["dpkg-deb", "-f", str(target), "Package"], capture_output=True,
                                     text=True, timeout=60).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            package = ""
        if package != entry["package"]:
            print("paquet inattendu : refusé", file=sys.stderr)
            return 10
        os.chmod(work, 0o755)
        os.chmod(target, 0o644)
        cmd = ["apt-get", "-y", "-q", "-o", "DPkg::Lock::Timeout=120", "-o", "Dpkg::Use-Pty=0"]
        fds = ()
        if status is not None:
            cmd += ["-o", f"APT::Status-Fd={status}"]
            fds = (status,)
        try:
            return subprocess.run(cmd + ["install", "--", str(target)], pass_fds=fds,
                                  stdin=subprocess.DEVNULL).returncode
        except OSError:
            return 127
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main(argv):
    if len(argv) == 2 and argv[0] == "clean" and IDENT.fullmatch(argv[1]):
        return clean(argv[1])
    if len(argv) == 2 and argv[0] in ("check", "revoke") and IDENT.fullmatch(argv[1]):
        return check(argv[1])
    if len(argv) == 3 and argv[0] == "grant" and IDENT.fullmatch(argv[1]) and SHA.fullmatch(argv[2]):
        return check(argv[1])
    if len(argv) in (4, 5) and argv[0] == "install" and IDENT.fullmatch(argv[1]) and SHA.fullmatch(argv[2]) \
            and argv[3].startswith("/") and argv[3].endswith(".deb"):
        status = None
        if len(argv) == 5:
            match = STATUS_FD.fullmatch(argv[4])
            if not match:
                print(__doc__, file=sys.stderr)
                return 64
            status = int(match.group(1))
        return install(argv[1], argv[2], argv[3], status)
    print(__doc__, file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
