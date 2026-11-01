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

def run(*args):
    return subprocess.run(args, check=True)

def read(path):
    try: return Path(path).read_text().strip()
    except FileNotFoundError: return ""

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
    pid = read(RUN / "update-inhibit")
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

def phase(name, percent, item):
    RUN.mkdir(parents=True, exist_ok=True)
    destination = RUN / "update-progress"
    temporary = destination.with_suffix(".new")
    temporary.write_text(f"phase={name}\npercent={percent}\ndone=0\ntotal=0\nitem={item}\n")
    temporary.replace(destination)

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

def offline_error(error):
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

def release_metadata(work):
    """Manifeste de la dernière release, ou repli sur l'API GitHub s'il n'a pas été publié."""
    metadata = work / "manifest"
    try:
        download(MANIFEST_URL, metadata)
        return manifest(metadata.read_text()), metadata.read_text()
    except subprocess.CalledProcessError as error:
        offline = offline_error(error)
        if offline: raise offline from None
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
        print("Version disponible : " + data["version"])
        if data["version"] == read("/usr/share/winlux/version"): print("Winlux est à jour.")
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
    print(f"Retour automatique : {failed} -> {installed_version()} ({reason})")
    return True

def confirm_boot():
    """Le bureau a tenu : la version à l'essai devient la version de référence."""
    if fields(read(TRIAL)).get("version") == installed_version():
        TRIAL.unlink(missing_ok=True)
        print("Mise à jour validée : " + installed_version())

def session_failed():
    """Le bureau n'a pas pu s'ouvrir : retour immédiat si la version est à l'essai."""
    if not fields(read(TRIAL)).get("version") or fields(read(TRIAL))["version"] != installed_version(): return False
    if not rollback_offline("session"): return False
    restart()
    return True

def skipped_automatically(version):
    """Une version écartée (échec ou retour manuel) ne se réinstalle pas toute seule."""
    return read(BLOCKED) == version

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
    automatic = args == ["--auto-run"]
    if automatic:
        if "auto=enabled" not in read(CONFIG).splitlines(): return 0
        args = ["--apply"]
    allowed = [["--check"], ["--apply"], ["--rollback"], ["--boot"], ["--auto", "enable"], ["--auto", "disable"],
               ["--confirm-boot"], ["--boot-failed"]]
    if args not in allowed: raise ValueError("Arguments de mise à jour invalides")
    STATE.mkdir(parents=True, exist_ok=True); RUN.mkdir(parents=True, exist_ok=True)
    with (STATE / "apt-update.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args[0] == "--auto":
            CONFIG.parent.mkdir(parents=True, exist_ok=True)
            CONFIG.write_text("\n".join(line for line in read(CONFIG).splitlines() if not line.startswith("auto="))
                              + "\nauto=" + ("enabled" if args[1] == "enable" else "disabled") + "\n")
            return 0
        if args == ["--confirm-boot"]: confirm_boot(); return 0
        if args == ["--boot-failed"]: return 0 if session_failed() else 1
        if args == ["--boot"]:
            boot_guard()
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
                restored.touch()
            # dpkg --audit sort en erreur dès qu'il a quelque chose à signaler :
            # une installation inachevée se termine ici, sans bloquer le démarrage.
            audit = subprocess.run(["dpkg", "--audit"], capture_output=True, text=True).stdout.strip()
            if audit:
                subprocess.run(["dpkg", "--configure", "-a"])
                audit = subprocess.run(["dpkg", "--audit"], capture_output=True, text=True).stdout.strip()
            if audit: print("Installation APT inachevée. Exécutez dpkg --configure -a.\n" + audit, file=sys.stderr)
            version = read("/usr/share/winlux/version")
            if read(STATE / "apt-desktop-cache-version") != version:
                run("update-desktop-database", "/usr/share/applications")
                run("update-mime-database", "/usr/share/mime")
                (STATE / "apt-desktop-cache-version").write_text(version)
            (RUN / "update-action").unlink(missing_ok=True)
            return 0
        if busy(): raise RuntimeError("Une installation APT est déjà en cours")
        (RUN / "update-inhibit").write_text(str(os.getpid()))
        try:
            with tempfile.TemporaryDirectory(prefix="apt-update-", dir=STATE) as temporary:
                work = Path(temporary)
                cache = CACHE; cache.mkdir(parents=True, exist_ok=True)
                if args == ["--rollback"]:
                    if not (cache / "previous.deb").is_file(): raise RuntimeError("Aucune sauvegarde du paquet précédent n’est disponible")
                    phase("restoring", -1, "Restauration du paquet précédent")
                    left = installed_version()
                    install(cache / "previous.deb", rollback=True)
                    # La mise à jour automatique ne doit pas réinstaller la version quittée.
                    BLOCKED.write_text(left + "\n")
                    TRIAL.unlink(missing_ok=True)
                    previous = cache / "previous.deb"
                    shutil.copy2(cache / "installed.deb", work / "current.deb")
                    cache_copy(previous, cache / "installed.deb")
                    cache_copy(work / "current.deb", previous)
                else:
                    phase("checking", 0, "Recherche de la mise à jour Winlux")
                    data, _ = release_metadata(work)
                    shutil.copy2(work / "manifest", STATE / "available-manifest")
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
                    source = f"https://github.com/{REPOSITORY}/releases/download/{data['version']}/{data['asset']}"
                    run(helper, "download", source, data["download_bytes"], str(package))
                    if package.stat().st_size != int(data["download_bytes"]): raise ValueError("Taille du paquet incorrecte")
                    digest = subprocess.check_output([helper, "hash", str(package), "0"], text=True).strip()
                    if digest != data["sha256"]:
                        package.unlink()
                        raise ValueError("Empreinte SHA-256 incorrecte : le paquet téléchargé a été supprimé")
                    restore_point(data["version"])
                    previous = installed_version()
                    phase("installing", -1, "Installation du paquet Winlux avec APT ; consultez le journal pour les détails")
                    install(package, version=data["version"])
                    if (cache / "installed.deb").exists(): cache_copy(cache / "installed.deb", cache / "previous.deb")
                    cache_copy(package, cache / "installed.deb")
                    package.unlink()
                    phase("verifying", -1, "Vérification des fichiers installés")
                    if verify_installed():
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
                    write_fields(TRIAL, {"version": data["version"], "previous": previous, "boots": "0"})
                (RUN / "update-action").write_text("reboot\n")
                phase("ready", 100, "Winlux installé ; redémarrage conseillé")
        except Exception:
            current = read(RUN / "update-progress")
            current_phase = next((line[6:] for line in current.splitlines() if line.startswith("phase=")), "")
            (RUN / "update-error-phase").write_text(current_phase + "\n")
            phase("error", 0, "Échec : consulter le journal")
            raise
        finally:
            (RUN / "update-inhibit").unlink(missing_ok=True)
    return 0

if __name__ == "__main__":
    try: sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr); sys.exit(1)
