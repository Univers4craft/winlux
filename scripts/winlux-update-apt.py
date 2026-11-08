#!/usr/bin/python3
"""Winlux updates on a native APT installation: update only its Debian package."""
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

STATE = Path("/var/lib/winlux")
RUN = Path("/run/winlux")
CONFIG = Path("/etc/winlux/update.conf")
CACHE = Path("/var/cache/winlux")
CMDLINE = Path("/proc/cmdline")
REPOSITORY = "Univers4craft/winlux"
# Une version installée reste « à l'essai » jusqu'à ce que le bureau ait tenu
# une minute. Après TRIAL_BOOTS démarrages sans bureau, retour automatique.
TRIAL = STATE / "update-trial"
BLOCKED = STATE / "update-blocked"
ROLLED_BACK = STATE / "update-rolled-back"
RESTORE_POINT = STATE / "update-restore-point"
TRIAL_BOOTS = 2
RESTORE_PREFIX = "Avant Winlux "
RESTORE_KEEP = 3
# Page Mises à jour (#875-#882) : historique, date de la dernière recherche,
# notes de version et avis pour la notification du bureau. Lisibles par le
# compte (0644), écrits seulement ici, en root.
HISTORY = STATE / "update-history"
HISTORY_KEEP = 50
LAST_CHECK = STATE / "update-last-check"
NOTES = STATE / "notes"
NOTES_KEEP = 10
NOTES_MAX = 24576
NOTICE = RUN / "update-notice"
# Suspension : 1 à 5 semaines, comme le menu de la page. Heures d'activité :
# aucune installation automatique entre début et fin (8 h – 18 h par défaut).
PAUSE_WEEKS = (1, 2, 3, 4, 5)
ACTIVE_HOURS = (8, 18)
# Droits spéciaux de l'Ubuntu Base (ubuntu-base-26.04.1, `tar tv`). La
# construction de l'image les efface (chown -R 0:0 avant APT) : passwd, chfn
# ou gpasswd sans setuid, unix_chkpwd et /etc/shadow sans le groupe shadow,
# et seuls les fichiers réinstallés ensuite par APT (su, mount, sudo)
# figuraient dans apt-base-permissions.json. Version 2 du marqueur : un
# système déjà installé les reçoit aussi au démarrage qui suit la mise à jour.
# Version 3 : « su » vers root passe par sudo (install_su_wrapper).
BASE_MODES_VERSION = "3"
BASE_MODES = (
    ("etc/shadow", "root", "shadow", 0o640),
    ("etc/gshadow", "root", "shadow", 0o640),
    ("usr/bin/chage", "root", "shadow", 0o2755),
    ("usr/bin/expiry", "root", "shadow", 0o2755),
    ("usr/sbin/unix_chkpwd", "root", "shadow", 0o2755),
    ("usr/sbin/pam_extrausers_chkpwd", "root", "shadow", 0o2755),
    ("usr/bin/chfn", "root", "root", 0o4755),
    ("usr/bin/chsh", "root", "root", 0o4755),
    ("usr/bin/gpasswd", "root", "root", 0o4755),
    ("usr/bin/passwd", "root", "root", 0o4755),
    ("usr/bin/newgrp", "root", "root", 0o4755),
    ("usr/bin/su", "root", "root", 0o4755),
    ("usr/bin/su.util-linux", "root", "root", 0o4755),
    ("usr/bin/mount", "root", "root", 0o4755),
    ("usr/bin/umount", "root", "root", 0o4755),
    ("usr/bin/sudo.ws", "root", "root", 0o4755),
    ("usr/bin/sudo-rs", "root", "root", 0o4755),
    ("usr/bin/pkexec", "root", "root", 0o4755),
    ("var/log/btmp", "root", "utmp", 0o660),
    ("var/log/lastlog", "root", "utmp", 0o664),
    ("var/log/wtmp", "root", "utmp", 0o664),
    ("var/mail", "root", "mail", 0o2775),
)

def run(*args):
    return subprocess.run(args, check=True)

def read(path):
    try: return Path(path).read_text().strip()
    except FileNotFoundError: return ""

def publish_run(path, text):
    """Fichier d'état de /run/winlux (collant) : temporaire à nom imprévisible
    puis renommé (#644), jamais un fichier qu'un compte aurait créé d'avance."""
    RUN.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as out: out.write(text)
        os.chmod(temporary, 0o644)
        os.replace(temporary, path)
    except BaseException:
        try: os.unlink(temporary)
        except OSError: pass
        raise

def root_file(path):
    """Drapeau croyable : posé par root (ou par ce compte, pour les tests),
    pas par un autre compte dans /run/winlux (#643)."""
    try: info = os.lstat(path)
    except OSError: return False
    return not os.path.islink(path) and info.st_uid in (0, os.geteuid())

def ids(path):
    """{nom: numéro} d'après un fichier passwd ou group (racine quelconque)."""
    found = {}
    for line in read(path).splitlines():
        fields = line.split(":")
        if len(fields) > 2 and fields[2].isdigit(): found.setdefault(fields[0], int(fields[2]))
    return found

def restore_base_modes(root=Path("/")):
    """Remet les droits de BASE_MODES ; renvoie les chemins corrigés.
    Un groupe absent de /etc/group laisse le fichier tel quel."""
    users, groups = ids(root / "etc/passwd"), ids(root / "etc/group")
    fixed = []
    for relative, user, group, mode in BASE_MODES:
        path = root / relative
        if path.is_symlink() or not path.exists() or user not in users or group not in groups: continue
        # Le su de Winlux est un script : jamais de setuid sur un script.
        if path.is_file() and path.read_bytes()[:2] == b"#!": continue
        info = path.stat()
        if (info.st_uid, info.st_gid, info.st_mode & 0o7777) == (users[user], groups[group], mode): continue
        os.chown(path, users[user], groups[group], follow_symlinks=False)
        # chmod après chown : chown efface les bits setuid et setgid.
        path.chmod(mode)
        fixed.append(relative)
    return fixed

SU_WRAPPER = "usr/share/winlux/native/shell/su"
REAL_SU = "usr/bin/su.util-linux"

def divert_su():
    """Détourne le su d'util-linux (survit aux mises à jour du paquet)."""
    run("dpkg-divert", "--quiet", "--local", "--rename", "--divert", "/" + REAL_SU, "--add", "/usr/bin/su")

def install_su_wrapper(root=Path("/"), divert=divert_su):
    """Root est verrouillé : /usr/bin/su devient la bascule vers sudo de
    Winlux, le vrai su reste en /usr/bin/su.util-linux (setuid conservé par
    le renommage). Renvoie True si /usr/bin/su a changé."""
    wrapper, su, real = root / SU_WRAPPER, root / "usr/bin/su", root / REAL_SU
    if not wrapper.is_file(): return False
    content = wrapper.read_bytes()
    if not real.exists():
        if not su.is_file() or su.is_symlink() or su.read_bytes()[:2] == b"#!": return False
        divert()
        if not real.exists(): return False
    if su.is_file() and not su.is_symlink() and su.read_bytes() == content and (su.stat().st_mode & 0o7777) == 0o755:
        return False
    temporary = su.with_name(".su.winlux")
    temporary.write_bytes(content)
    temporary.chmod(0o755)
    os.replace(temporary, su)
    return True

def manifest(text):
    fields = {}
    for line in text.splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in fields: raise ValueError("Manifeste incorrect")
        fields[key] = value
    if fields.get("format") != "3" or fields.get("backend") != "apt" or fields.get("base") != "ubuntu-resolute":
        raise ValueError("Cette mise à jour ne correspond pas à la base APT de Winlux")
    for key in ("version", "asset"):
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+-]*", fields.get(key, "")):
            raise ValueError("Nom de version ou de paquet invalide")
    if not fields["asset"].endswith(".deb") or not re.fullmatch(r"[0-9a-f]{64}", fields.get("sha256", "")):
        raise ValueError("Paquet ou empreinte invalide")
    if not re.fullmatch(r"[0-9]{1,12}", fields.get("download_bytes", "")):
        raise ValueError("Taille du paquet invalide")
    if not re.fullmatch(r"[0-9]{1,12}", fields.get("unpacked_kib", "")):
        raise ValueError("Taille décompressée invalide")
    return fields

def busy():
    pid = read(RUN / "update-inhibit") if root_file(RUN / "update-inhibit") else ""
    if pid.isdigit() and int(pid) > 0:
        try: os.kill(int(pid), 0); return True
        except ProcessLookupError: pass
    # APT uses POSIX record locks, not flock. Respect manual terminal installs.
    # Les verrous ne sont jamais créés ici : un fichier absent signifie « libre ».
    for path in ("/var/lib/dpkg/lock-frontend", "/var/lib/dpkg/lock"):
        try:
            handle = open(path, "r+")
            mode = fcntl.LOCK_EX
        except FileNotFoundError:
            continue
        except PermissionError:
            try: handle = open(path, "r")
            except OSError: continue
            # Un verrou partagé est refusé tant qu'APT tient le verrou exclusif.
            mode = fcntl.LOCK_SH
        with handle:
            try: fcntl.lockf(handle, mode | fcntl.LOCK_NB)
            except (BlockingIOError, PermissionError): return True
    return False

def public_write(path, text):
    """Fichier d'état lisible par le compte, remplacé d'un coup."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as out: out.write(text)
        os.chmod(temporary, 0o644)
        os.replace(temporary, path)
    except BaseException:
        try: os.unlink(temporary)
        except OSError: pass
        raise

def config_values():
    return fields(read(CONFIG))

def set_config(key, value):
    """Une clé de update.conf (auto=, pause_until=, active_hours=…) ; None l'efface."""
    CONFIG.parent.mkdir(parents=True, exist_ok=True)
    lines = [line for line in read(CONFIG).splitlines() if line.strip() and not line.startswith(key + "=")]
    if value is not None: lines.append(f"{key}={value}")
    CONFIG.write_text("\n".join(lines) + "\n")

def record(result, version, previous="", detail=""):
    """Une ligne d'historique : installée, échec, restaurée (à la main ou toute seule)."""
    entry = {"date": int(time.time()), "version": version or "?", "from": previous or "", "result": result}
    if detail: entry["detail"] = " ".join(str(detail).split())[:300]
    lines = [line for line in read(HISTORY).splitlines() if line.strip()][-(HISTORY_KEEP - 1):]
    try: public_write(HISTORY, "\n".join(lines + [json.dumps(entry, ensure_ascii=False)]) + "\n")
    except OSError as error: print(f"Historique non écrit : {error}", file=sys.stderr)

def notice(kind, version):
    """Avis lu par la session (notification « disponible » ou « redémarrez »)."""
    try: publish_run(NOTICE, f"kind={kind}\nversion={version}\ndate={int(time.time())}\n")
    except OSError as error: print(f"Avis non publié : {error}", file=sys.stderr)

def paused_until(now=None):
    value = config_values().get("pause_until", "")
    until = int(value) if value.isdigit() else 0
    return until if until > (now or time.time()) else 0

def active_hours():
    match = re.fullmatch(r"([0-9]{1,2})-([0-9]{1,2})", config_values().get("active_hours", ""))
    if match and int(match.group(1)) < 24 and int(match.group(2)) < 24:
        return int(match.group(1)), int(match.group(2))
    return ACTIVE_HOURS

def in_active_hours(hour=None, hours=None):
    start, end = hours or active_hours()
    hour = time.localtime().tm_hour if hour is None else hour
    if start == end: return False
    return start <= hour < end if start < end else (hour >= start or hour < end)

def save_notes(work, data):
    """Notes de version (docs/releases/<v>.md) : fichier publié et vérifié
    (notes_asset + notes_sha256), sinon le texte de la release GitHub.
    Jamais bloquant : sans notes, la mise à jour reste installable."""
    version = data["version"]
    target = NOTES / f"{version}.md"
    if target.is_file(): return
    text = ""
    try:
        asset, digest = data.get("notes_asset", ""), data.get("notes_sha256", "")
        local = local_manifest()
        if asset and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+-]*", asset) and re.fullmatch(r"[0-9a-f]{64}", digest):
            notes = work / "notes"
            if local: shutil.copyfile(local_asset(local, asset), notes)
            else: download(f"https://github.com/{REPOSITORY}/releases/download/{version}/{asset}", notes)
            import hashlib
            if hashlib.sha256(notes.read_bytes()).hexdigest() != digest:
                raise ValueError("SHA-256 des notes incorrect")
            text = notes.read_text(errors="replace")
        elif not local:
            body = work / "release-body"
            if not body.is_file():
                listing_file = work / "release-notes.json"
                download(RELEASE_API, listing_file, "application/vnd.github+json")
                release = json.loads(listing_file.read_text())
                if release.get("tag_name") == version: body.write_text(release.get("body") or "")
            if body.is_file(): text = body.read_text(errors="replace")
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Notes de version indisponibles : {error}", file=sys.stderr)
        return
    if not text.strip(): return
    NOTES.mkdir(parents=True, exist_ok=True); NOTES.chmod(0o755)
    public_write(target, text[:NOTES_MAX])
    for old in sorted(NOTES.glob("*.md"), key=lambda path: path.stat().st_mtime)[:-NOTES_KEEP]:
        old.unlink(missing_ok=True)

def state():
    """État lisible par la page Mises à jour : une clé=valeur par ligne."""
    installed = read("/usr/share/winlux/version")
    values = {"installed": installed}
    cached = read(STATE / "available-manifest")
    if cached:
        try:
            data = manifest(cached)
            if data["version"] != installed:
                values.update(available=data["version"], download_bytes=data["download_bytes"],
                              unpacked_kib=data["unpacked_kib"])
                if (NOTES / f"{data['version']}.md").is_file(): values["notes"] = data["version"]
        except ValueError: pass
    if (NOTES / f"{installed}.md").is_file(): values["installed_notes"] = installed
    values["last_check"] = read(LAST_CHECK) or "0"
    config = config_values()
    values["auto"] = "enabled" if config.get("auto") == "enabled" else "disabled"
    values["paused_until"] = str(paused_until())
    values["active_hours"] = "%d-%d" % active_hours()
    values["blocked"] = read(BLOCKED)
    rolled = fields(read(ROLLED_BACK))
    values["rolled_failed"], values["rolled_restored"] = rolled.get("failed", ""), rolled.get("restored", "")
    values["trial"] = fields(read(TRIAL)).get("version", "")
    values["restore_point"] = fields(read(RESTORE_POINT)).get("date", "")
    previous = package_version(CACHE / "previous.deb")
    values["previous"] = "v" + previous if previous else ""
    values["reboot"] = "1" if read(RUN / "update-action") == "reboot" else "0"
    for key, value in values.items(): print(f"{key}={value}")

def phase(name, percent, item):
    publish_run(RUN / "update-progress", f"phase={name}\npercent={percent}\ndone=0\ntotal=0\nitem={item}\n")

def download(url, destination, accept=None):
    command = ["curl", "--fail", "--location", "--silent", "--show-error", "--proto", "=https",
               "--connect-timeout", "20", "--max-time", "900", "--output", str(destination)]
    if accept: command += ["--header", "Accept: " + accept]
    run(*command, url)

MANIFEST_URL = f"https://github.com/{REPOSITORY}/releases/latest/download/winlux-update-manifest.txt"
RELEASE_API = f"https://api.github.com/repos/{REPOSITORY}/releases/latest"
# Codes de curl qui signifient « pas de réseau », pas « fichier absent ».
OFFLINE_CODES = {5: "proxy introuvable", 6: "nom github.com introuvable (DNS)", 7: "connexion refusée",
                 28: "délai dépassé", 35: "échec TLS", 56: "connexion coupée"}

def certificate_error(error, url=MANIFEST_URL):
    """Échec TLS (35, 60) : la raison en clair (date fausse, portail, autorité…)."""
    if error.returncode not in (35, 51, 60): return None
    tool = shutil.which("winlux-certificates") or str(Path(__file__).resolve().with_name("winlux-certificates.py"))
    if not Path(tool).exists(): return None
    try:
        result = subprocess.run([sys.executable, "-B", tool, "explain-curl", str(error.returncode), url],
                                capture_output=True, text=True, timeout=40)
        text = json.loads(result.stdout.strip().splitlines()[-1]).get("message", "")
    except (OSError, ValueError, IndexError, subprocess.SubprocessError):
        return None
    return RuntimeError(text) if text else None

def offline_error(error):
    explained = certificate_error(error)
    if explained: return explained
    reason = OFFLINE_CODES.get(error.returncode)
    if reason is None: return None
    return RuntimeError("GitHub est injoignable (" + reason + ") : vérifiez la connexion réseau puis relancez la recherche.")

def manifest_from_release(release):
    """Manifeste reconstruit depuis la release GitHub, quand le fichier manque."""
    tag = release.get("tag_name", "")
    debs = [asset for asset in release.get("assets", []) if re.fullmatch(r"winlux-desktop-[A-Za-z0-9._+-]+\.deb", asset.get("name", ""))]
    if not debs:
        raise RuntimeError(f"La dernière version publiée ({tag or 'inconnue'}) ne contient ni manifeste ni paquet "
                           "winlux-desktop .deb : aucune mise à jour installable pour l’instant.")
    deb = debs[0]
    digest = deb.get("digest") or ""
    checksum = digest[7:] if digest.startswith("sha256:") else ""
    return deb, tag, checksum

def local_manifest():
    """Manifeste local, pour tester une version avant de la publier.

    WINLUX_UPDATE_MANIFEST (chemin absolu ou file://) quand l'outil est lancé
    directement en root, ou « manifest=… » dans /etc/winlux/update.conf, que
    seul root peut écrire, pour le chemin Paramètres > Mises à jour (le relais
    root ne transmet pas l'environnement). Le paquet est cherché dans
    WINLUX_UPDATE_ASSET_DIR, sinon à côté du manifeste. Taille et SHA-256 du
    manifeste restent vérifiées comme pour GitHub.
    """
    value = os.environ.get("WINLUX_UPDATE_MANIFEST", "")
    if not value:
        value = next((line[9:] for line in read(CONFIG).splitlines() if line.startswith("manifest=")), "")
    value = value.strip()
    if not value: return None
    if value.startswith("file://"): value = value[7:]
    path = Path(value)
    if not path.is_absolute() or not path.is_file():
        raise RuntimeError(f"Manifeste local introuvable : {value} (WINLUX_UPDATE_MANIFEST ou manifest= dans {CONFIG})")
    return path

def local_asset(manifest_path, asset):
    directory = Path(os.environ.get("WINLUX_UPDATE_ASSET_DIR") or manifest_path.parent)
    source = directory / asset
    if not source.is_file(): raise RuntimeError(f"Paquet local introuvable : {source}")
    return source

# Clé publique Ed25519 des mises à jour (#31). Tant qu'elle est absente de
# l'image, les manifestes ne sont pas vérifiés ; dès qu'elle y est, un
# manifeste sans signature valide est refusé.
PUBLIC_KEY = Path(os.environ.get("WINLUX_UPDATE_PUBLIC_KEY") or "/usr/share/winlux/update-signing.pub")

def signature_tool():
    import runpy
    for candidate in (Path("/usr/bin/winlux-signature"), Path(__file__).resolve().with_name("winlux-signature.py")):
        if candidate.is_file(): return runpy.run_path(str(candidate))
    raise RuntimeError("Outil de vérification des signatures introuvable (winlux-signature)")

def check_signature(metadata, signature):
    """Vérifie metadata (manifeste téléchargé) avec le fichier signature."""
    if not PUBLIC_KEY.is_file():
        return False
    if not signature.is_file():
        raise RuntimeError("Le manifeste de mise à jour n’est pas signé : mise à jour refusée par sécurité.")
    try: good = signature_tool()["verify_file"](PUBLIC_KEY, metadata, signature)
    except ValueError: good = False
    if not good:
        raise RuntimeError("La signature du manifeste de mise à jour est invalide : mise à jour refusée par sécurité.")
    print("Signature Ed25519 du manifeste vérifiée.", flush=True)
    return True

def release_metadata(work):
    """Manifeste de la dernière release, ou repli sur l'API GitHub s'il n'a pas été publié."""
    metadata = work / "manifest"
    signature = work / "manifest.sig"
    local = local_manifest()
    if local:
        print(f"Manifeste local utilisé : {local}", flush=True)
        text = local.read_text()
        data = manifest(text)
        metadata.write_text(text)
        local_signature = local.with_name(local.name + ".sig")
        if local_signature.is_file(): shutil.copyfile(local_signature, signature)
        check_signature(metadata, signature)
        return data, text
    try:
        download(MANIFEST_URL, metadata)
        if PUBLIC_KEY.is_file():
            try: download(MANIFEST_URL + ".sig", signature)
            except subprocess.CalledProcessError as error:
                offline = offline_error(error)
                if offline: raise offline from None
        check_signature(metadata, signature)
        return manifest(metadata.read_text()), metadata.read_text()
    except subprocess.CalledProcessError as error:
        offline = offline_error(error)
        if offline: raise offline from None
    if PUBLIC_KEY.is_file():
        raise RuntimeError("La dernière version ne publie pas de manifeste signé : mise à jour refusée par sécurité.")
    print("Manifeste absent de la dernière version : informations lues dans la release GitHub.", flush=True)
    listing_file = work / "release.json"
    try: download(RELEASE_API, listing_file, "application/vnd.github+json")
    except subprocess.CalledProcessError as error:
        raise (offline_error(error) or RuntimeError("Aucune version publiée de Winlux n’a été trouvée sur GitHub.")) from None
    deb, tag, checksum = manifest_from_release(json.loads(listing_file.read_text()))
    if not checksum:
        # Anciennes releases : la somme vient du fichier .sha256 publié à côté.
        sums = work / "package.sha256"
        try: download(f"https://github.com/{REPOSITORY}/releases/download/{tag}/{deb['name']}.sha256", sums)
        except subprocess.CalledProcessError as error:
            raise (offline_error(error) or RuntimeError(f"La version {tag} ne publie pas l’empreinte SHA-256 de son paquet : "
                                                         "mise à jour refusée par sécurité.")) from None
        checksum = (sums.read_text().split() or [""])[0].lower()
    size = int(deb.get("size") or 0)
    text = (f"format=3\nbackend=apt\nbase=ubuntu-resolute\nversion={tag}\nscope=system\naction=reboot\n"
            f"asset={deb['name']}\nsha256={checksum}\ndownload_bytes={size}\n"
            # Taille décompressée inconnue : estimation prudente (×3).
            f"unpacked_kib={size * 3 // 1024}\nsource=github-api\n")
    metadata.write_text(text)
    return manifest(text), text

def listing():
    print("Version installée : " + read("/usr/share/winlux/version"))
    cached = read(STATE / "available-manifest")
    if cached:
        data = manifest(cached)
        if data["version"] == read("/usr/share/winlux/version"):
            # À jour : ni taille ni composants, qui laissaient croire à une
            # mise à jour en attente (« Taille à télécharger : 463 Mo »).
            print("Winlux est à jour.")
        else:
            print("Version disponible : " + data["version"])
            print("Composants : bureau et runtime Winlux (APT)")
            print("Taille à télécharger : " + data["download_bytes"] + " octets")
            print("Taille décompressée : " + data["unpacked_kib"] + " Kio (hors sauvegarde et marge)")
            print("Action après installation : redémarrage conseillé")
        if read(BLOCKED) and read(BLOCKED) == data["version"]:
            print("Installation automatique suspendue : " + data["version"])
    else: print("Aucune vérification disponible. Lancez Rechercher.")
    rolled = fields(read(ROLLED_BACK))
    if rolled.get("failed"):
        print("Version écartée (le bureau n’a pas démarré) : " + rolled["failed"])
        print("Version restaurée automatiquement : " + rolled.get("restored", "?"))
    trial = fields(read(TRIAL))
    if trial.get("version"):
        print("Validation au prochain démarrage du bureau : " + trial["version"])
    point = fields(read(RESTORE_POINT))
    if point.get("date"):
        print("Point de restauration avant mise à jour : " + point["date"])
    previous = package_version(CACHE / "previous.deb")
    if previous:
        print("Version précédente conservée : v" + previous)

def fields(text):
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)

def write_fields(path, values):
    temporary = path.with_name(path.name + ".new")
    temporary.write_text("".join(f"{key}={value}\n" for key, value in values.items()))
    with temporary.open("rb") as handle: os.fsync(handle.fileno())
    temporary.replace(path)

def package_version(package):
    if not Path(package).is_file(): return ""
    try: return subprocess.check_output(["dpkg-deb", "-f", str(package), "Version"], text=True,
                                        stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.SubprocessError): return ""

def installed_version():
    return read("/usr/share/winlux/version")

def restore_point(version):
    """Point de restauration maison (configuration + liste des paquets) avant l'installation."""
    tool = shutil.which("winlux-restore")
    if tool is None: return
    phase("restore-point", -1, "Création d’un point de restauration")
    output = subprocess.run([tool, "create", RESTORE_PREFIX + version], check=True,
                            capture_output=True, text=True).stdout
    created = fields(output)
    if not created.get("id"): raise RuntimeError("Point de restauration impossible : mise à jour annulée")
    write_fields(RESTORE_POINT, {"id": created["id"], "version": installed_version(),
                                 "date": time.strftime("%d/%m/%Y %H:%M")})
    # Seuls les points créés par les mises à jour sont élagués, jamais ceux de l'utilisateur.
    listed = subprocess.run([tool, "list"], capture_output=True, text=True).stdout.splitlines()
    ours = sorted(line.split("|")[0] for line in listed
                  if line.count("|") >= 3 and line.split("|")[1].startswith(RESTORE_PREFIX))
    for old in ours[:-RESTORE_KEEP]:
        subprocess.run([tool, "delete", old], capture_output=True)

def verify_installed():
    """Vérifie chaque fichier installé contre les empreintes du paquet (hors /etc modifiable)."""
    report = subprocess.run(["dpkg", "--verify", "winlux-desktop"], capture_output=True, text=True)
    problems = [line for line in report.stdout.splitlines()
                if line.strip() and not (len(line) > 10 and line[10] == "c")]
    return problems

def restart():
    os.sync()
    subprocess.run(["/bin/busybox", "reboot", "-f"])

def rollback_offline(reason):
    """Retour à la version précédente sans réseau ni hooks APT, au démarrage."""
    previous = CACHE / "previous.deb"
    trial = fields(read(TRIAL))
    if not previous.is_file():
        TRIAL.unlink(missing_ok=True)
        print("Aucune version précédente conservée : retour automatique impossible", file=sys.stderr)
        return False
    failed = installed_version()
    phase("restoring", -1, "Retour à la version précédente")
    subprocess.run(["dpkg", "--force-confold", "--install", str(previous)], check=True,
                   env=dict(os.environ, DEBIAN_FRONTEND="noninteractive"))
    with tempfile.TemporaryDirectory(prefix="apt-rollback-", dir=STATE) as temporary:
        current = Path(temporary) / "current.deb"
        if (CACHE / "installed.deb").is_file(): shutil.copy2(CACHE / "installed.deb", current)
        cache_copy(previous, CACHE / "installed.deb")
        if current.is_file(): cache_copy(current, previous)
    BLOCKED.write_text(trial.get("version", failed) + "\n")
    write_fields(ROLLED_BACK, {"failed": trial.get("version", failed), "restored": installed_version(), "reason": reason})
    TRIAL.unlink(missing_ok=True)
    record("auto-restored", installed_version(), trial.get("version", failed), reason)
    print(f"Retour automatique : {failed} -> {installed_version()} ({reason})")
    return True

# Mises à jour A/B (#31) : la version est installée dans l'emplacement B
# (winlux-ab prepare) et /init bascule au démarrage ; l'ancienne version reste
# en réserve. Sans winlux-ab, ou si le paquet demande un paquet Ubuntu absent,
# installation APT sur place comme avant.
AB_TOOL = os.environ.get("WINLUX_AB_TOOL", "/usr/bin/winlux-ab")
AB_NEEDS_APT = 3

def ab_state():
    if not os.access(AB_TOOL, os.X_OK): return {}
    output = subprocess.run([AB_TOOL, "status"], capture_output=True, text=True).stdout
    return fields(output)

def ab(*args):
    return subprocess.run([AB_TOOL, *args], capture_output=True, text=True)

def ab_prepare(package, version):
    """Vrai si la mise à jour est prête dans l'emplacement B ; faux s'il faut APT."""
    if not os.access(AB_TOOL, os.X_OK) or not Path("/init").is_file(): return False
    phase("installing", -1, "Préparation de la nouvelle version à côté du système actuel")
    result = ab("prepare", str(package), version)
    if result.returncode == AB_NEEDS_APT:
        print("Paquet Ubuntu nouveau requis : installation APT sur place", file=sys.stderr)
        return False
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout).strip() or "Préparation A/B impossible")
    return True

def ab_rolled_back():
    """/init est revenu sur la réserve : noter l'échec une seule fois."""
    state = ab_state()
    if state.get("state") != "rolled-back" or read(STATE / "ab-recorded") == state.get("version", ""): return
    failed = state.get("version", "")
    BLOCKED.write_text(failed + "\n")
    write_fields(ROLLED_BACK, {"failed": failed, "restored": installed_version(), "reason": state.get("reason", "boot")})
    record("rolled-back" if state.get("reason") != "manual" else "restored", installed_version(), failed)
    (STATE / "ab-recorded").write_text(failed + "\n")

def confirm_boot():
    """Le bureau a tenu : la version à l'essai devient la version de référence."""
    if ab_state().get("state") == "trial":
        ab("confirm")
        print("Mise à jour validée : " + installed_version())
    if fields(read(TRIAL)).get("version") == installed_version():
        TRIAL.unlink(missing_ok=True)
        print("Mise à jour validée : " + installed_version())

def session_failed():
    """Le bureau n'a pas pu s'ouvrir : retour immédiat si la version est à l'essai."""
    if ab_state().get("state") == "trial":
        if ab("rollback", "session").returncode != 0: return False
        restart()
        return True
    if not fields(read(TRIAL)).get("version") or fields(read(TRIAL))["version"] != installed_version(): return False
    if not rollback_offline("session"): return False
    restart()
    return True

def skipped_automatically(version):
    """Une version écartée (échec ou retour manuel) ne se réinstalle pas toute seule."""
    return read(BLOCKED) == version

def readable_cache(cache=CACHE):
    """Paquets conservés lisibles par le compte (anciens fichiers en 0600)."""
    if not cache.is_dir(): return
    cache.chmod(0o755)
    for name in ("installed.deb", "previous.deb"):
        package = cache / name
        if package.is_file() and not package.is_symlink() and package.stat().st_mode & 0o777 != 0o644:
            package.chmod(0o644)

DESKTOP_CACHES = 'update-desktop-database /usr/share/applications; update-mime-database /usr/share/mime && printf "%s\\n" "$1" >"$2"'

def refresh_desktop_caches_later(version, marker, log=RUN / "desktop-caches.log"):
    """Caches MIME et applications après une mise à jour, en arrière-plan.

    update-mime-database prenait ~30 s au premier démarrage après la mise à
    jour v0.2.18 → v0.2.19, logo seul à l'écran. APT les a déjà refaits
    (déclencheurs dpkg) : ceci n'est qu'une vérification, qui ne doit pas
    retarder le bureau. Le repère n'est écrit qu'à la fin ; interrompu, le
    travail est refait au démarrage suivant."""
    command = ["sh", "-c", DESKTOP_CACHES, "winlux-desktop-caches", version, str(marker)]
    if shutil.which("ionice"): command = ["ionice", "-c", "3"] + command
    if shutil.which("nice"): command = ["nice", "-n", "19"] + command
    try:
        with open(log, "ab") as output:
            subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                             start_new_session=True)
        return True
    except OSError as error:
        print(f"Caches du bureau non relancés : {error}", file=sys.stderr)
        return False

def timed(label, started):
    """Durée de chaque étape dans boot-update.log (temps de démarrage)."""
    print(f"{label} : {time.monotonic() - started:.1f} s", file=sys.stderr, flush=True)
    return time.monotonic()

def boot_guard():
    """Compte les démarrages d'une version à l'essai ; revient en arrière si le bureau n'arrive jamais."""
    trial = fields(read(TRIAL))
    if not trial.get("version"): return
    if "winlux.interactive=1" not in read(CMDLINE).split(): return
    if trial["version"] != installed_version():
        TRIAL.unlink(missing_ok=True); return
    counted = trial.get("boots", "")
    boots = (int(counted) if counted.isdigit() else TRIAL_BOOTS) + 1
    if boots > TRIAL_BOOTS:
        try:
            if rollback_offline("boot"): restart()
        except (OSError, subprocess.SubprocessError) as error:
            # Ne jamais bloquer le démarrage : la nouvelle version reste en place.
            TRIAL.unlink(missing_ok=True)
            print("Retour automatique impossible : " + str(error), file=sys.stderr)
        return
    trial["boots"] = str(boots)
    write_fields(TRIAL, trial)

def install(package, rollback=False, version=None):
    if subprocess.check_output(["dpkg-deb", "-f", str(package), "Package"], text=True).strip() != "winlux-desktop":
        raise ValueError("Le paquet téléchargé n’est pas Winlux")
    actual = subprocess.check_output(["dpkg-deb", "-f", str(package), "Version"], text=True).strip()
    if version and actual != version.removeprefix("v"): raise ValueError("Version du paquet incorrecte")
    command = ["apt-get", "--no-remove", "--reinstall", "-y", "-o", "Dpkg::Options::=--force-confold"]
    if rollback: command.append("--allow-downgrades")
    # APT n'accepte un fichier local que s'il se termine par .deb : le
    # téléchargement (download-<sha>.part) est exposé sous ce nom le temps de
    # l'installation. Sans cela, la v0.2.12 ne pouvait installer aucune mise à jour.
    package = Path(package)
    staged = None
    if package.suffix != ".deb":
        staged = package.with_name(package.stem + ".deb")
        staged.unlink(missing_ok=True)
        os.link(package, staged)
    try:
        subprocess.run(command + ["install", str(staged or package)], check=True,
                       env=dict(os.environ, DEBIAN_FRONTEND="noninteractive"))
    finally:
        if staged: staged.unlink(missing_ok=True)

def cache_copy(source, destination):
    with tempfile.NamedTemporaryFile(dir=destination.parent, prefix=".package-", delete=False) as handle:
        temporary = Path(handle.name)
    try:
        shutil.copyfile(source, temporary)
        # Paquet public : lisible par le compte (`--list` lit sa version avec
        # dpkg-deb) ; mkstemp le créait en 0600.
        temporary.chmod(0o644)
        with temporary.open("rb") as handle: os.fsync(handle.fileno())
        temporary.replace(destination)
    finally: temporary.unlink(missing_ok=True)

def main():
    if os.environ.get("WINLUX_UPDATE_ROOT", "/") != "/":
        raise ValueError("Le moteur APT doit être exécuté dans le système cible")
    if read("/usr/share/winlux/update-backend") != "apt": raise ValueError("Installation APT requise")
    args = sys.argv[1:]
    if args == ["--auto"]: args.append("status")
    if args == ["--busy"]: return 0 if busy() else 1
    if args == ["--auto", "status"]: print("enabled" if "auto=enabled" in read(CONFIG).splitlines() else "disabled"); return 0
    if args == ["--list"]: listing(); return 0
    if args == ["--state"]: state(); return 0
    automatic = args == ["--auto-run"]
    if automatic:
        if "auto=enabled" not in read(CONFIG).splitlines(): return 0
        # Suspendues, ou pendant les heures d'activité : rien ne s'installe tout seul.
        if paused_until(): return 0
        if in_active_hours(): return 0
        args = ["--apply"]
    allowed = [["--check"], ["--apply"], ["--rollback"], ["--boot"], ["--auto", "enable"], ["--auto", "disable"],
               ["--confirm-boot"], ["--boot-failed"], ["--resume"]]
    if len(args) == 2 and args[0] == "--pause" and args[1].isdigit() and int(args[1]) in PAUSE_WEEKS:
        allowed.append(args)
    if len(args) == 3 and args[0] == "--active-hours" and all(value.isdigit() and int(value) < 24 for value in args[1:]) \
            and args[1] != args[2]:
        allowed.append(args)
    if args not in allowed: raise ValueError("Arguments de mise à jour invalides")
    # dpkg et les caches (MIME, icônes, applications) doivent rester lisibles
    # par le bureau, qui tourne sous le compte de l'utilisateur.
    os.umask(0o022)
    STATE.mkdir(parents=True, exist_ok=True); RUN.mkdir(parents=True, exist_ok=True)
    with (STATE / "apt-update.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args[0] == "--auto":
            set_config("auto", "enabled" if args[1] == "enable" else "disabled")
            return 0
        if args[0] == "--pause":
            # Fin de la suspension à minuit, au bout de N semaines.
            day = time.localtime(time.time() + int(args[1]) * 7 * 86400)
            set_config("pause_until", int(time.mktime((day.tm_year, day.tm_mon, day.tm_mday, 23, 59, 59, 0, 0, -1))))
            return 0
        if args == ["--resume"]: set_config("pause_until", None); return 0
        if args[0] == "--active-hours": set_config("active_hours", f"{int(args[1])}-{int(args[2])}"); return 0
        if args == ["--confirm-boot"]: confirm_boot(); return 0
        if args == ["--boot-failed"]: return 0 if session_failed() else 1
        if args == ["--boot"]:
            step = time.monotonic()
            ab_rolled_back()
            boot_guard()
            step = timed("Version à l'essai", step)
            restored = STATE / "apt-base-permissions-restored"
            if not restored.exists():
                permissions = json.loads(Path("/usr/share/winlux/apt-base-permissions.json").read_text())
                for relative, (uid, gid, mode) in permissions.items():
                    path = Path("/") / relative
                    if path.exists() and not path.is_symlink():
                        os.chown(path, uid, gid, follow_symlinks=False)
                        path.chmod(mode)
                if not Path("/usr/lib/locale/fr_FR.utf8").is_dir():
                    run("localedef", "--no-archive", "-i", "fr_FR", "-f", "UTF-8", "/usr/lib/locale/fr_FR.utf8")
                # The image builder defers native cache triggers to the guest;
                # never execute distribution maintainer scripts on the host.
                run("dpkg", "--configure", "--pending")
                restored.write_text("1\n")
            if read(restored) != BASE_MODES_VERSION:
                # Jamais bloquant : un bureau sans sudo vaut mieux que pas de bureau.
                try:
                    restore_base_modes()
                    install_su_wrapper()
                    restored.write_text(BASE_MODES_VERSION + "\n")
                except (OSError, subprocess.CalledProcessError) as error:
                    print(f"Droits de la base Ubuntu ou su non rétablis : {error}", file=sys.stderr)
            try: readable_cache()
            except OSError as error: print(f"Paquets conservés non rendus lisibles : {error}", file=sys.stderr)
            step = timed("Droits de la base et su", step)
            # dpkg --audit sort en erreur dès qu'il a quelque chose à signaler :
            # une installation inachevée se termine ici, sans bloquer le démarrage.
            audit = subprocess.run(["dpkg", "--audit"], capture_output=True, text=True).stdout.strip()
            if audit:
                subprocess.run(["dpkg", "--configure", "-a"])
                audit = subprocess.run(["dpkg", "--audit"], capture_output=True, text=True).stdout.strip()
            if audit: print("Installation APT inachevée. Exécutez dpkg --configure -a.\n" + audit, file=sys.stderr)
            step = timed("Vérification dpkg", step)
            version = read("/usr/share/winlux/version")
            if read(STATE / "apt-desktop-cache-version") != version:
                refresh_desktop_caches_later(version, STATE / "apt-desktop-cache-version")
                timed("Caches du bureau (lancés en arrière-plan)", step)
            (RUN / "update-action").unlink(missing_ok=True)
            return 0
        if busy(): raise RuntimeError("Une installation APT est déjà en cours")
        publish_run(RUN / "update-inhibit", str(os.getpid()))
        try:
            with tempfile.TemporaryDirectory(prefix="apt-update-", dir=STATE) as temporary:
                work = Path(temporary)
                cache = CACHE; cache.mkdir(parents=True, exist_ok=True)
                if args == ["--rollback"] and ab_state().get("reserve") == "yes":
                    phase("restoring", -1, "Retour à la version de réserve au prochain démarrage")
                    result = ab("rollback")
                    if result.returncode != 0: raise RuntimeError(result.stderr.strip())
                    BLOCKED.write_text(installed_version() + "\n")
                elif args == ["--rollback"]:
                    if not (cache / "previous.deb").is_file(): raise RuntimeError("Aucune sauvegarde du paquet précédent n’est disponible")
                    phase("restoring", -1, "Restauration du paquet précédent")
                    left = installed_version()
                    install(cache / "previous.deb", rollback=True)
                    # La mise à jour automatique ne doit pas réinstaller la version quittée.
                    BLOCKED.write_text(left + "\n")
                    TRIAL.unlink(missing_ok=True)
                    record("restored", installed_version(), left)
                    previous = cache / "previous.deb"
                    shutil.copy2(cache / "installed.deb", work / "current.deb")
                    cache_copy(previous, cache / "installed.deb")
                    cache_copy(work / "current.deb", previous)
                else:
                    phase("checking", 0, "Recherche de la mise à jour Winlux")
                    data, _ = release_metadata(work)
                    shutil.copy2(work / "manifest", STATE / "available-manifest")
                    public_write(LAST_CHECK, str(int(time.time())) + "\n")
                    if data["version"] != read("/usr/share/winlux/version"):
                        save_notes(work, data)
                        published = fields(read(NOTICE))
                        if (published.get("kind"), published.get("version")) != ("available", data["version"]):
                            notice("available", data["version"])
                    listing()
                    if args == ["--check"] or data["version"] == read("/usr/share/winlux/version"):
                        phase("idle", 100, "Recherche terminée")
                        return 0
                    if automatic and skipped_automatically(data["version"]):
                        print("Version " + data["version"] + " écartée après un échec : installation manuelle seulement")
                        phase("idle", 100, "Recherche terminée")
                        return 0
                    required = int(data["download_bytes"]) * 2 + int(data["unpacked_kib"]) * 2048 + 256 * 1024 * 1024
                    for location in (STATE, cache, Path("/")):
                        available = shutil.disk_usage(location).free
                        if available < required:
                            raise RuntimeError(f"Espace insuffisant sur {location} : {required // 1048576} Mio requis, {available // 1048576} Mio disponibles")
                    package = cache / ("download-" + data["sha256"] + ".part")
                    if package.is_symlink() or (package.exists() and not package.is_file()):
                        raise ValueError("Cache de téléchargement invalide")
                    helper = "/usr/bin/winlux-update-progress"
                    local = local_manifest()
                    if local:
                        phase("downloading", -1, "Copie du paquet local")
                        shutil.copyfile(local_asset(local, data["asset"]), package)
                    else:
                        source = f"https://github.com/{REPOSITORY}/releases/download/{data['version']}/{data['asset']}"
                        run(helper, "download", source, data["download_bytes"], str(package))
                    if package.stat().st_size != int(data["download_bytes"]): raise ValueError("Taille du paquet incorrecte")
                    digest = subprocess.check_output([helper, "hash", str(package), "0"], text=True).strip()
                    if digest != data["sha256"]:
                        package.unlink()
                        raise ValueError("Empreinte SHA-256 incorrecte : le paquet téléchargé a été supprimé")
                    restore_point(data["version"])
                    previous = installed_version()
                    prepared = ab_prepare(package, data["version"])
                    if not prepared:
                        phase("installing", -1, "Installation du paquet Winlux avec APT ; consultez le journal pour les détails")
                        install(package, version=data["version"])
                    if (cache / "installed.deb").exists(): cache_copy(cache / "installed.deb", cache / "previous.deb")
                    cache_copy(package, cache / "installed.deb")
                    package.unlink()
                    phase("verifying", -1, "Vérification des fichiers installés")
                    if not prepared and verify_installed():
                        # Une seule réparation (réinstallation) avant d'abandonner la version.
                        install(cache / "installed.deb", version=data["version"])
                        problems = verify_installed()
                        if problems:
                            if (cache / "previous.deb").is_file():
                                install(cache / "previous.deb", rollback=True)
                                cache_copy(cache / "previous.deb", cache / "installed.deb")
                            BLOCKED.write_text(data["version"] + "\n")
                            raise RuntimeError("Fichiers installés incorrects, version précédente remise en place :\n"
                                               + "\n".join(problems[:20]))
                    BLOCKED.unlink(missing_ok=True); ROLLED_BACK.unlink(missing_ok=True)
                    # En A/B, c'est /init qui compte les démarrages à l'essai.
                    if not prepared:
                        write_fields(TRIAL, {"version": data["version"], "previous": previous, "boots": "0"})
                    (STATE / "ab-recorded").unlink(missing_ok=True)
                    record("installed", data["version"], previous)
                    notice("reboot", data["version"])
                publish_run(RUN / "update-action", "reboot\n")
                phase("ready", 100, "Winlux installé ; redémarrage conseillé")
        except Exception:
            current = read(RUN / "update-progress")
            current_phase = next((line[6:] for line in current.splitlines() if line.startswith("phase=")), "")
            publish_run(RUN / "update-error-phase", current_phase + "\n")
            phase("error", 0, "Échec : consulter le journal")
            if args == ["--apply"] and current_phase not in ("", "checking", "idle"):
                failed = fields(read(STATE / "available-manifest")).get("version", "")
                record("failed", failed, installed_version(), current_phase + " : " + str(sys.exc_info()[1]))
            raise
        finally:
            (RUN / "update-inhibit").unlink(missing_ok=True)
    return 0

if __name__ == "__main__":
    try: sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr); sys.exit(1)
