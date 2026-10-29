#!/usr/bin/python3
"""Seed native desktop defaults once; retain personal configuration afterwards."""
from pathlib import Path
import os
import shutil
import pwd
import grp
import sys

if os.geteuid() == 0:
    try:
        saver = pwd.getpwnam("winlux-saver")
    except KeyError:
        occupied = {entry.pw_uid for entry in pwd.getpwall()} | {entry.gr_gid for entry in grp.getgrall()}
        uid = 1002
        while uid in occupied: uid += 1
        with Path("/etc/group").open("a") as stream: stream.write(f"winlux-saver:x:{uid}:\n")
        with Path("/etc/passwd").open("a") as stream: stream.write(f"winlux-saver:!:{uid}:{uid}:Winlux screensaver:/home/winlux-saver:/bin/false\n")
        saver = pwd.getpwnam("winlux-saver")
    directory = Path(saver.pw_dir)
    directory.mkdir(parents=True, exist_ok=True)
    directory.chmod(0o700)
    os.chown(directory, saver.pw_uid, saver.pw_gid)

home = Path.home()
source = Path("/usr/share/winlux/native")
reset_panel = "--reset-panel" in sys.argv
for path in (source / "config").rglob("*"):
    if path.is_file():
        target = home / ".config" / path.relative_to(source / "config")
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists() or (reset_panel and path.name == "xfce4-panel.xml"):
            shutil.copyfile(path, target)
if not (home / ".xscreensaver").exists(): shutil.copyfile(source / "xscreensaver", home / ".xscreensaver")
user_dirs = home / ".config/user-dirs.dirs"
if not user_dirs.exists():
    user_dirs.write_text('XDG_DESKTOP_DIR="$HOME/Bureau"\nXDG_DOWNLOAD_DIR="$HOME/Téléchargements"\nXDG_DOCUMENTS_DIR="$HOME/Documents"\nXDG_PICTURES_DIR="$HOME/Images"\nXDG_MUSIC_DIR="$HOME/Musique"\nXDG_VIDEOS_DIR="$HOME/Vidéos"\n')
desktop = home / "Bureau"
desktop.mkdir(exist_ok=True)
# Le bureau reste vide par défaut. Les icônes (Corbeille, Explorateur,
# Documents, etc.) sont ajoutées explicitement depuis Paramètres.
# Defaults for file opening, not a rewrite of the user's selected associations.
mimeapps = home / ".config/mimeapps.list"
if not mimeapps.exists():
    mimeapps.write_text("[Default Applications]\ntext/plain=org.xfce.mousepad.desktop;\ntext/x-python=org.xfce.mousepad.desktop;\ntext/x-shellscript=org.xfce.mousepad.desktop;\ninode/directory=winlux-explorer.desktop;\napplication/x-ms-dos-executable=winlux.desktop;\napplication/x-msdownload=winlux.desktop;\napplication/vnd.microsoft.portable-executable=winlux.desktop;\nx-scheme-handler/http=winlux-browser.desktop;\nx-scheme-handler/https=winlux-browser.desktop;\n")
