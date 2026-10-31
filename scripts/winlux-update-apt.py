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

STATE = Path("/var/lib/winlux")
RUN = Path("/run/winlux")
CONFIG = Path("/etc/winlux/update.conf")
REPOSITORY = "Univers4craft/winlux"

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
    for path in ("/var/lib/dpkg/lock-frontend", "/var/lib/dpkg/lock"):
        with open(path, "a") as handle:
            try: fcntl.lockf(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError: return True
    return False

def phase(name, percent, item):
    RUN.mkdir(parents=True, exist_ok=True)
    destination = RUN / "update-progress"
    temporary = destination.with_suffix(".new")
    temporary.write_text(f"phase={name}\npercent={percent}\ndone=0\ntotal=0\nitem={item}\n")
    temporary.replace(destination)

def download(url, destination):
    run("curl", "--fail", "--location", "--silent", "--show-error", "--proto", "=https",
        "--connect-timeout", "20", "--max-time", "900", "--output", str(destination), url)

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
    else: print("Aucune vérification disponible. Lancez Rechercher.")

def install(package, rollback=False, version=None):
    if subprocess.check_output(["dpkg-deb", "-f", str(package), "Package"], text=True).strip() != "winlux-desktop":
        raise ValueError("Le paquet téléchargé n’est pas Winlux")
    actual = subprocess.check_output(["dpkg-deb", "-f", str(package), "Version"], text=True).strip()
    if version and actual != version.removeprefix("v"): raise ValueError("Version du paquet incorrecte")
    command = ["apt-get", "--no-remove", "--reinstall", "-y", "-o", "Dpkg::Options::=--force-confold"]
    if rollback: command.append("--allow-downgrades")
    subprocess.run(command + ["install", str(package)], check=True,
                   env=dict(os.environ, DEBIAN_FRONTEND="noninteractive"))

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
    if args == ["--auto-run"]:
        if "auto=enabled" not in read(CONFIG).splitlines(): return 0
        args = ["--apply"]
    allowed = [["--check"], ["--apply"], ["--rollback"], ["--boot"], ["--auto", "enable"], ["--auto", "disable"]]
    if args not in allowed: raise ValueError("Arguments de mise à jour invalides")
    STATE.mkdir(parents=True, exist_ok=True); RUN.mkdir(parents=True, exist_ok=True)
    with (STATE / "apt-update.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args[0] == "--auto":
            CONFIG.parent.mkdir(parents=True, exist_ok=True)
            CONFIG.write_text("\n".join(line for line in read(CONFIG).splitlines() if not line.startswith("auto="))
                              + "\nauto=" + ("enabled" if args[1] == "enable" else "disabled") + "\n")
            return 0
        if args == ["--boot"]:
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
            audit = subprocess.check_output(["dpkg", "--audit"], text=True)
            if audit: raise RuntimeError("Installation APT inachevée. Exécutez dpkg --configure -a.\n" + audit)
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
                cache = Path("/var/cache/winlux"); cache.mkdir(parents=True, exist_ok=True)
                if args == ["--rollback"]:
                    if not (cache / "previous.deb").is_file(): raise RuntimeError("Aucune sauvegarde du paquet précédent n’est disponible")
                    phase("restoring", -1, "Restauration du paquet précédent")
                    install(cache / "previous.deb", rollback=True)
                    previous = cache / "previous.deb"
                    shutil.copy2(cache / "installed.deb", work / "current.deb")
                    cache_copy(previous, cache / "installed.deb")
                    cache_copy(work / "current.deb", previous)
                else:
                    phase("checking", 0, "Recherche de la mise à jour Winlux")
                    metadata = work / "manifest"
                    download(f"https://github.com/{REPOSITORY}/releases/latest/download/winlux-update-manifest.txt", metadata)
                    data = manifest(metadata.read_text())
                    shutil.copy2(metadata, STATE / "available-manifest")
                    listing()
                    if args == ["--check"] or data["version"] == read("/usr/share/winlux/version"):
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
                    phase("installing", -1, "Installation du paquet Winlux avec APT ; consultez le journal pour les détails")
                    install(package, version=data["version"])
                    if (cache / "installed.deb").exists(): cache_copy(cache / "installed.deb", cache / "previous.deb")
                    cache_copy(package, cache / "installed.deb")
                    package.unlink()
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
