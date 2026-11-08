#!/usr/bin/python3
"""Mises à jour A/B de Winlux (#31) : deux emplacements système, une réserve.

    winlux-ab prepare PAQUET.deb VERSION   installe la mise à jour dans l'emplacement B
    winlux-ab status                       état (clé=valeur)
    winlux-ab confirm                      le bureau a tenu : la version à l'essai est validée
    winlux-ab rollback [RAISON]            revenir à la réserve au prochain démarrage (manual, session)
    winlux-ab discard                      oublier une mise à jour préparée non appliquée

Le système actif n'est jamais modifié pendant une mise à jour. `prepare`
construit l'emplacement B (`/.winlux-ab/b`) à partir du système actif : liens
physiques pour /usr et /opt/winlux (seuls les fichiers que le paquet change
prennent de la place, dpkg remplaçant chaque fichier par renommage), copie de
/var/lib/dpkg et de /init. dpkg y installe le paquet sans chroot
(`--root`, `--force-script-chrootless` : les scripts du paquet respectent
DPKG_ROOT), puis vérifie chaque fichier installé. Au redémarrage, `/init`
appelle `winlux --ab-boot` (runtime/src/abslots.rs) qui échange atomiquement
les deux emplacements : l'ancienne version devient la réserve, la seule
gardée. Trois démarrages sans bureau, ou un bureau en échec, ramènent la
réserve.

Un paquet qui demande un paquet Ubuntu absent ne peut pas s'installer ainsi
(dpkg n'a pas le réseau) : code de sortie 3, le client de mise à jour garde
alors l'installation APT habituelle.
"""
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

ROOT = Path(os.environ.get("WINLUX_AB_ROOT", "/"))
DIR = ROOT / ".winlux-ab"
SLOT = DIR / "b"
STATE = DIR / "state"
PACKAGE = "winlux-desktop"
# Chemins échangés au démarrage : même liste que runtime/src/abslots.rs.
PAIRS = ("usr", "var/lib/dpkg", "opt/winlux", "init")
HARDLINKED = ("usr", "opt/winlux")
NEEDS_APT = 3


def read_state():
    try:
        text = STATE.read_text()
    except OSError:
        return {}
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def write_state(state):
    DIR.mkdir(mode=0o700, exist_ok=True)
    temporary = STATE.with_suffix(".tmp")
    with temporary.open("w") as handle:
        handle.write("".join(f"{k}={v}\n" for k, v in sorted(state.items())))
        handle.flush()
        os.fsync(handle.fileno())
    temporary.replace(STATE)
    descriptor = os.open(DIR, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def link_tree(source, target):
    """Copie de `source` en liens physiques : dossiers recréés avec leurs
    droits et propriétaires, fichiers liés, liens symboliques recopiés."""
    for folder, directories, files in os.walk(source):
        relative = Path(folder).relative_to(source)
        destination = target / relative
        info = os.lstat(folder)
        destination.mkdir(exist_ok=True)
        for name in files + [d for d in directories if os.path.islink(os.path.join(folder, d))]:
            path = Path(folder) / name
            if path.is_symlink():
                os.symlink(os.readlink(path), destination / name)
                os.lchown(destination / name, path.lstat().st_uid, path.lstat().st_gid)
            elif stat.S_ISREG(path.lstat().st_mode):
                os.link(path, destination / name)
            # Fichiers spéciaux (tubes, périphériques) : jamais dans /usr.
        directories[:] = [d for d in directories if not os.path.islink(os.path.join(folder, d))]
        os.chown(destination, info.st_uid, info.st_gid)
        os.chmod(destination, stat.S_IMODE(info.st_mode))


def copy_path(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cp", "-a", str(source), str(target)], check=True)


def package_files(admin):
    path = admin / "info" / f"{PACKAGE}.list"
    try:
        return path.read_text().splitlines()
    except OSError:
        return []


def copy_etc(files, source_root, target):
    """Fichiers du paquet sous /etc, depuis la racine donnée."""
    for name in files:
        if not name.startswith("/etc/"):
            continue
        path = source_root / name.lstrip("/")
        if path.is_file() or path.is_symlink():
            copy_path(path, target / name[len("/etc/"):])


def remove(path):
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    elif path.exists() or path.is_symlink():
        path.unlink()


def dpkg_stamp():
    info = (ROOT / "var/lib/dpkg/status").stat()
    return f"{int(info.st_mtime)}.{info.st_mtime_ns % 1_000_000_000}"


def dpkg(*args):
    command = ["dpkg", "--root=" + str(SLOT), "--log=" + str(SLOT / "var/log/dpkg.log")]
    if os.geteuid() != 0:
        command.append("--force-not-root")
    return subprocess.run(command + list(args), capture_output=True, text=True)


def refresh_caches():
    """Ce que le postinst fait sur un système actif, appliqué à B."""
    tools = [["update-desktop-database", str(SLOT / "usr/share/applications")],
             ["update-mime-database", str(SLOT / "usr/share/mime")]]
    for theme in ("Winlux", "Winlux-Sombre"):
        if (SLOT / "usr/share/icons" / theme / "index.theme").is_file():
            tools.append(["gtk-update-icon-cache", "-f", "-q", str(SLOT / "usr/share/icons" / theme)])
    for tool in tools:
        if shutil.which(tool[0]):
            subprocess.run(tool, capture_output=True)


def tree_bytes(path):
    total = 0
    for folder, _, files in os.walk(path):
        for name in files:
            try:
                total += os.lstat(os.path.join(folder, name)).st_size
            except OSError:
                pass
    return total


def check_space(package):
    """Place nécessaire : le paquet décompressé, la copie de /var/lib/dpkg et
    une marge de 256 Mio. Refus clair plutôt qu'un emplacement B à moitié écrit."""
    field = subprocess.run(["dpkg-deb", "-f", str(package), "Installed-Size"],
                           capture_output=True, text=True).stdout.strip()
    unpacked = int(field) * 1024 if field.isdigit() else package.stat().st_size * 4
    needed = unpacked + tree_bytes(ROOT / "var/lib/dpkg") + 256 * 1024 * 1024
    free = shutil.disk_usage(ROOT).free
    if free < needed:
        raise RuntimeError(f"Espace insuffisant pour préparer la mise à jour : {needed // 1048576} Mio "
                           f"nécessaires, {free // 1048576} Mio libres. Rien n’a été modifié.")


def prepare(package, version):
    check_space(package)
    state = read_state()
    if state.get("state") == "trial":
        raise RuntimeError("La version précédente est encore à l’essai : redémarrez et attendez sa validation.")
    current_version = (ROOT / "usr/share/winlux/version").read_text().strip() if (ROOT / "usr/share/winlux/version").is_file() else ""
    # L'ancienne réserve laisse la place : on ne garde qu'une version précédente.
    DIR.mkdir(mode=0o700, exist_ok=True)
    os.chmod(DIR, 0o700)
    STATE.unlink(missing_ok=True)
    remove(SLOT)
    SLOT.mkdir(mode=0o755)
    for relative in HARDLINKED:
        if (ROOT / relative).is_dir():
            (SLOT / relative).parent.mkdir(parents=True, exist_ok=True)
            link_tree(ROOT / relative, SLOT / relative)
    copy_path(ROOT / "var/lib/dpkg", SLOT / "var/lib/dpkg")
    if (ROOT / "init").is_file():
        copy_path(ROOT / "init", SLOT / "init")
    for relative in ("bin", "lib", "lib64", "sbin"):
        if (ROOT / relative).is_symlink():
            os.symlink(os.readlink(ROOT / relative), SLOT / relative)
    (SLOT / "var/log").mkdir(parents=True, exist_ok=True)
    (SLOT / "tmp").mkdir(mode=0o1777, exist_ok=True)
    before = package_files(ROOT / "var/lib/dpkg")
    copy_etc(before, ROOT, SLOT / "etc-current")
    # dpkg lit sa configuration et les fichiers de configuration du paquet
    # dans <racine>/etc.
    if (ROOT / "etc/dpkg").is_dir():
        copy_path(ROOT / "etc/dpkg", SLOT / "etc/dpkg")
    copy_etc(before, ROOT, SLOT / "etc")
    result = dpkg("--force-script-chrootless", "--no-triggers", "--force-confold", "--install", str(package))
    if result.returncode != 0:
        output = result.stdout + result.stderr
        remove(SLOT)
        if "dependency problems" in output or "dépendances" in output:
            print(output, file=sys.stderr)
            return NEEDS_APT
        raise RuntimeError("Installation dans l’emplacement B impossible :\n" + output[-2000:])
    installed = (SLOT / "usr/share/winlux/version").read_text().strip() if (SLOT / "usr/share/winlux/version").is_file() else ""
    if version and installed.removeprefix("v") != version.removeprefix("v"):
        remove(SLOT)
        raise RuntimeError(f"Version installée dans l’emplacement B incorrecte : {installed}")
    refresh_caches()
    report = dpkg("--verify", PACKAGE)
    problems = [line for line in report.stdout.splitlines()
                if line.strip() and not (len(line) > 10 and line[10] == "c")]
    if problems:
        remove(SLOT)
        raise RuntimeError("Fichiers incorrects dans l’emplacement B :\n" + "\n".join(problems[:20]))
    copy_etc(package_files(SLOT / "var/lib/dpkg"), SLOT, SLOT / "etc-next")
    remove(SLOT / "etc")
    state = {"state": "prepared", "version": installed, "previous_version": current_version,
             "dpkg_status": dpkg_stamp()}
    for pair in PAIRS:
        if (SLOT / pair).exists() and (ROOT / pair).exists():
            state[f"next_ino.{pair}"] = str((SLOT / pair).lstat().st_ino)
            state[f"current_ino.{pair}"] = str((ROOT / pair).lstat().st_ino)
    os.sync()
    write_state(state)
    print(f"Mise à jour {installed} prête dans l’emplacement B : elle s’appliquera au prochain démarrage.")
    return 0


def confirm():
    state = read_state()
    if state.get("state") == "trial":
        state["state"] = "confirmed"
        state.pop("boots", None)
        write_state(state)
        print("Version validée : " + state.get("version", ""))
    return 0


def rollback(reason="manual"):
    state = read_state()
    if state.get("state") not in ("trial", "confirmed"):
        raise RuntimeError("Aucune version de réserve disponible.")
    state["state"] = "rollback"
    state["reason"] = reason
    write_state(state)
    print("Retour à " + state.get("previous_version", "la version précédente") + " au prochain démarrage.")
    return 0


def discard():
    state = read_state()
    if state.get("state") in ("prepared", "stale"):
        STATE.unlink(missing_ok=True)
        remove(SLOT)
    return 0


def status():
    state = read_state()
    for key in ("state", "version", "previous_version", "boots", "reason", "error"):
        if key in state:
            print(f"{key}={state[key]}")
    print("reserve=" + ("yes" if state.get("state") in ("trial", "confirmed") else "no"))
    return 0


def main(argv):
    if len(argv) >= 2 and argv[1] == "status":
        return status()
    if os.geteuid() != 0 and ROOT == Path("/"):
        print("Droits administrateur requis.", file=sys.stderr)
        return 1
    if len(argv) == 4 and argv[1] == "prepare":
        return prepare(Path(argv[2]), argv[3])
    if len(argv) == 3 and argv[1] == "rollback" and argv[2] in ("manual", "session"):
        return rollback(argv[2])
    commands = {"confirm": confirm, "rollback": rollback, "discard": discard}
    if len(argv) == 2 and argv[1] in commands:
        return commands[argv[1]]()
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
