#!/usr/bin/python3
"""Téléchargements du navigateur dans le dossier de l'utilisateur.

Depuis la session principale (root), Edge, Chrome et Firefox tournent sous le
compte isolé ``winlux-web`` (sandbox du navigateur conservée). Sans ce module,
leurs fichiers tombaient dans /home/winlux-web/Downloads, invisible depuis
C:\\ et l'Explorateur, et « Ouvrir le dossier » ne faisait rien.

- ``prepare`` (root) : le dossier Téléchargements de l'utilisateur
  (XDG_DOWNLOAD_DIR, soit C:\\Users\\<nom>\\Downloads) est monté en lecture et
  écriture dans /home/winlux-web/Téléchargements, qui devient le dossier de
  téléchargement XDG du navigateur. Les anciens téléchargements restés dans le
  compte du navigateur y sont déplacés.
- ``serve`` (root) : pendant que le navigateur tourne, ouvre l'Explorateur sur
  les dossiers demandés par le navigateur, uniquement dans Téléchargements.
- ``filemanager`` (winlux-web) : service D-Bus org.freedesktop.FileManager1 du
  bus privé du navigateur (« Afficher dans le dossier », « Ouvrir le dossier »).
- ``open CHEMIN`` (winlux-web) : gestionnaire des dossiers (xdg-open).
"""
import os
import pwd
import shutil
import subprocess
import sys
import time
import uuid
from pathlib import Path
from urllib.parse import unquote, urlparse

BROWSER_USER = "winlux-web"
BROWSER_HOME = Path(os.environ.get("WINLUX_BROWSER_HOME", "/home/winlux-web"))
BROWSER_DOWNLOADS = BROWSER_HOME / "Téléchargements"
# /run/winlux est réservé à root (700) : boîte aux lettres à part.
SPOOL = Path(os.environ.get("WINLUX_BROWSER_SPOOL", "/run/winlux-browser-open"))
HANDLER = "winlux-browser-folder.desktop"
SELF = Path(__file__).resolve()


def user_downloads(home):
    """XDG_DOWNLOAD_DIR de l'utilisateur, sinon ~/Téléchargements."""
    config = Path(os.environ.get("XDG_CONFIG_HOME") or home / ".config")
    try:
        lines = (config / "user-dirs.dirs").read_text(encoding="utf-8").splitlines()
    except OSError:
        lines = []
    for line in lines:
        key, _, value = line.strip().partition("=")
        if key.strip() != "XDG_DOWNLOAD_DIR":
            continue
        value = value.strip().strip('"')
        if value.startswith("$HOME"):
            path = home / value[5:].lstrip("/")
        elif value.startswith("/"):
            path = Path(value)
        else:
            continue
        if path != home:
            return path
    return home / "Téléchargements"


def is_mount(path):
    try:
        with open("/proc/self/mountinfo", encoding="utf-8") as mounts:
            target = str(path)
            for line in mounts:
                fields = line.split()
                if len(fields) > 4 and fields[4].replace("\\040", " ") == target:
                    return True
    except OSError:
        pass
    return False


def same_dir(a, b):
    try:
        sa, sb = a.stat(), b.stat()
    except OSError:
        return False
    return (sa.st_dev, sa.st_ino) == (sb.st_dev, sb.st_ino)


def unique_target(folder, name):
    target = folder / name
    stem, dot, ext = name.rpartition(".")
    if not dot or not stem:
        stem, ext = name, ""
    number = 1
    while target.exists() or target.is_symlink():
        number += 1
        target = folder / (f"{stem} ({number}).{ext}" if ext else f"{stem} ({number})")
    return target


def move_contents(source, destination):
    """Déplace les anciens téléchargements, sans rien écraser."""
    if not source.is_dir() or source.is_symlink() or is_mount(source):
        return
    for entry in sorted(source.iterdir()):
        try:
            shutil.move(str(entry), str(unique_target(destination, entry.name)))
        except OSError as error:
            print(f"{entry} : {error}", file=sys.stderr)
    try:
        source.rmdir()
    except OSError:
        pass


def write_if_changed(path, text, uid, gid):
    try:
        if path.read_text(encoding="utf-8") == text:
            return
    except OSError:
        pass
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(text, encoding="utf-8")
    os.chown(temporary, uid, gid)
    temporary.replace(path)
    os.chown(path.parent, uid, gid)


def set_mime_default(path, uid, gid):
    """inode/directory → notre gestionnaire, sans toucher au reste."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        lines = []
    wanted = f"inode/directory={HANDLER};"
    if wanted in lines:
        return
    lines = [line for line in lines if not line.startswith("inode/directory=")]
    if "[Default Applications]" not in lines:
        lines.insert(0, "[Default Applications]")
    lines.insert(lines.index("[Default Applications]") + 1, wanted)
    write_if_changed(path, "\n".join(lines) + "\n", uid, gid)


def prepare():
    if os.getuid() != 0:
        return 0
    try:
        web = pwd.getpwnam(BROWSER_USER)
    except KeyError:
        return 0
    uid, gid = web.pw_uid, web.pw_gid
    home = Path(os.environ.get("HOME") or "/root")
    downloads = user_downloads(home)
    downloads.mkdir(parents=True, exist_ok=True)
    # Le navigateur écrit dans le dossier de l'utilisateur par son groupe ;
    # setgid : les nouveaux fichiers restent dans ce groupe.
    os.chown(downloads, -1, gid)
    os.chmod(downloads, 0o2775)
    BROWSER_HOME.mkdir(parents=True, exist_ok=True)
    # Anciennes versions : le navigateur enregistrait dans ~/Downloads.
    move_contents(BROWSER_HOME / "Downloads", downloads)
    if not same_dir(BROWSER_DOWNLOADS, downloads):
        if is_mount(BROWSER_DOWNLOADS):
            subprocess.run(["/usr/bin/umount", "-l", str(BROWSER_DOWNLOADS)], check=False)
        move_contents(BROWSER_DOWNLOADS, downloads)
        BROWSER_DOWNLOADS.mkdir(exist_ok=True)
        os.chown(BROWSER_DOWNLOADS, uid, gid)
        result = subprocess.run(["/usr/bin/mount", "--bind", str(downloads), str(BROWSER_DOWNLOADS)], check=False)
        if result.returncode != 0:
            print("Dossier Téléchargements non partagé avec le navigateur", file=sys.stderr)
    config = BROWSER_HOME / ".config"
    write_if_changed(config / "user-dirs.dirs", 'XDG_DOWNLOAD_DIR="$HOME/Téléchargements"\n', uid, gid)
    # Sans ce fichier, xdg-user-dirs-update remettrait les noms anglais.
    write_if_changed(config / "user-dirs.locale", "fr_FR\n", uid, gid)
    set_mime_default(config / "mimeapps.list", uid, gid)
    applications = BROWSER_HOME / ".local/share/applications"
    write_if_changed(
        applications / HANDLER,
        "[Desktop Entry]\nType=Application\nName=Explorateur\nNoDisplay=true\n"
        f"Exec=/usr/bin/python3 {SELF} open %f\nMimeType=inode/directory;\n",
        uid,
        gid,
    )
    # La session est XFCE : xdg-open passe par exo-open et le gestionnaire de
    # fichiers XFCE (Thunar, qui ne peut pas tourner sous ce compte).
    write_if_changed(config / "xfce4/helpers.rc", "FileManager=winlux-browser-folder\n", uid, gid)
    write_if_changed(
        BROWSER_HOME / ".local/share/xfce4/helpers/winlux-browser-folder.desktop",
        "[Desktop Entry]\nType=X-XFCE-Helper\nX-XFCE-Category=FileManager\nName=Explorateur\n"
        "Icon=system-file-manager\nX-XFCE-Binaries=python3;\n"
        f"X-XFCE-Commands=/usr/bin/python3 {SELF} open;\n"
        f'X-XFCE-CommandsWithParameter=/usr/bin/python3 {SELF} open "%s";\n',
        uid,
        gid,
    )
    for parent in (BROWSER_HOME / ".local/share/xfce4", BROWSER_HOME / ".local/share", BROWSER_HOME / ".local"):
        os.chown(parent, uid, gid)
    # Boîte aux lettres : le navigateur dépose, seul root lit.
    SPOOL.mkdir(parents=True, exist_ok=True)
    os.chown(SPOOL, 0, gid)
    os.chmod(SPOOL, 0o1730)
    return 0


# ---------- Côté navigateur (winlux-web) ----------

def request(path):
    """Dépose une demande d'ouverture pour la session de l'utilisateur."""
    path = Path(path)
    name = SPOOL / f".{uuid.uuid4().hex}"
    try:
        with open(name, "x", encoding="utf-8") as handle:
            handle.write(str(path))
        os.replace(name, SPOOL / name.name[1:])
    except OSError as error:
        print(f"Demande refusée : {error}", file=sys.stderr)
        return 1
    return 0


def uri_path(uri):
    if uri.startswith("/"):
        return uri
    parsed = urlparse(uri)
    if parsed.scheme != "file":
        return None
    return unquote(parsed.path)


def filemanager():
    from gi.repository import Gio, GLib

    xml = """<node><interface name="org.freedesktop.FileManager1">
<method name="ShowFolders"><arg type="as" direction="in"/><arg type="s" direction="in"/></method>
<method name="ShowItems"><arg type="as" direction="in"/><arg type="s" direction="in"/></method>
<method name="ShowItemProperties"><arg type="as" direction="in"/><arg type="s" direction="in"/></method>
</interface></node>"""
    loop = GLib.MainLoop()
    idle = {"source": 0}

    def quit_later():
        if idle["source"]:
            GLib.source_remove(idle["source"])
        idle["source"] = GLib.timeout_add_seconds(60, loop.quit)

    def call(_connection, _sender, _path, _interface, method, parameters, invocation):
        uris, _startup = parameters.unpack()
        for uri in uris[:16]:
            path = uri_path(uri)
            if path:
                request(path)
        invocation.return_value(None)
        quit_later()

    info = Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]

    def acquired(connection, _name):
        connection.register_object("/org/freedesktop/FileManager1", info, call, None, None)

    Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.FileManager1", Gio.BusNameOwnerFlags.NONE, acquired, None, lambda *_: loop.quit())
    quit_later()
    loop.run()
    return 0


# ---------- Côté session (root) ----------

def windows_path(path, home):
    try:
        rest = path.relative_to(home)
    except ValueError:
        return None
    return "C:\\" + str(rest).replace("/", "\\") if str(rest) != "." else "C:\\"


def allowed(raw, downloads):
    """Chemin demandé → dossier à ouvrir, seulement dans Téléchargements."""
    path = Path(os.path.normpath(raw))
    for root in (BROWSER_DOWNLOADS, downloads):
        try:
            rest = path.relative_to(root)
        except ValueError:
            continue
        if ".." in rest.parts:
            return None
        target = downloads / rest
        resolved = target.resolve()
        if resolved != downloads.resolve() and downloads.resolve() not in resolved.parents:
            return None
        while target != downloads and not target.is_dir():
            target = target.parent
        return target
    return None


def open_explorer(folder, home):
    location = windows_path(folder, home) or str(folder)
    subprocess.Popen(
        ["/usr/bin/winlux-shell-app", "explorer", location],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )


def browser_running(uid):
    for status in Path("/proc").glob("[0-9]*/status"):
        try:
            for line in status.read_text().splitlines():
                if line.startswith("Uid:"):
                    if int(line.split()[1]) == uid:
                        return True
                    break
        except (OSError, ValueError, IndexError):
            continue
    return False


def serve(watch_pid):
    """Tourne tant que le navigateur vit (un seul serveur à la fois)."""
    if os.getuid() != 0:
        return 0
    import fcntl

    try:
        web_uid = pwd.getpwnam(BROWSER_USER).pw_uid
    except KeyError:
        return 0
    home = Path(os.environ.get("HOME") or "/root")
    downloads = user_downloads(home)
    SPOOL.mkdir(parents=True, exist_ok=True)
    lock = open(SPOOL.with_name(SPOOL.name + ".lock"), "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        return 0
    started = time.monotonic()
    while True:
        for entry in sorted(SPOOL.iterdir()):
            if entry.name.startswith("."):
                continue
            try:
                raw = entry.read_text(encoding="utf-8")[:4096]
            except OSError:
                raw = ""
            entry.unlink(missing_ok=True)
            folder = allowed(raw.strip(), downloads)
            if folder:
                open_explorer(folder, home)
        launcher_alive = watch_pid and Path(f"/proc/{watch_pid}").exists()
        if not launcher_alive and time.monotonic() - started > 30 and not browser_running(web_uid):
            return 0
        time.sleep(0.5)


def main(argv):
    command = argv[1] if len(argv) > 1 else ""
    if command == "prepare":
        return prepare()
    if command == "serve":
        return serve(int(argv[2]) if len(argv) > 2 and argv[2].isdigit() else 0)
    if command == "filemanager":
        return filemanager()
    if command == "open":
        # Sans argument : le dossier Téléchargements lui-même.
        path = argv[2] if len(argv) > 2 and argv[2] else str(BROWSER_DOWNLOADS)
        return request(uri_path(path) or path)
    print("usage: winlux-browser-downloads prepare|serve [PID]|filemanager|open CHEMIN", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
