#!/usr/bin/python3
"""Téléchargements du navigateur : dossier de l'utilisateur et « Ouvrir le dossier »."""
import importlib.util
import os
import tempfile
from pathlib import Path

project = Path(__file__).resolve().parent.parent
work = Path(tempfile.mkdtemp())
os.environ["WINLUX_BROWSER_HOME"] = str(work / "web")
os.environ["WINLUX_BROWSER_SPOOL"] = str(work / "spool")
os.environ.pop("XDG_CONFIG_HOME", None)
spec = importlib.util.spec_from_file_location(
    "downloads", project / "integration/native/browser/winlux-browser-downloads.py"
)
downloads = importlib.util.module_from_spec(spec)
spec.loader.exec_module(downloads)

home = work / "root"
(home / ".config").mkdir(parents=True)
# Sans user-dirs.dirs : ~/Téléchargements.
assert downloads.user_downloads(home) == home / "Téléchargements"
(home / ".config/user-dirs.dirs").write_text(
    '# xdg\nXDG_DESKTOP_DIR="$HOME/Bureau"\nXDG_DOWNLOAD_DIR="$HOME/Mes téléchargements"\n'
)
user = downloads.user_downloads(home)
assert user == home / "Mes téléchargements", user
# XDG_DOWNLOAD_DIR=$HOME désactive le dossier : jamais tout le dossier personnel.
(home / ".config/user-dirs.dirs").write_text('XDG_DOWNLOAD_DIR="$HOME"\n')
assert downloads.user_downloads(home) == home / "Téléchargements"
(home / ".config/user-dirs.dirs").write_text('XDG_DOWNLOAD_DIR="$HOME/Mes téléchargements"\n')

# Chemins Windows vus par l'Explorateur (C:\ = dossier personnel).
assert downloads.windows_path(user, home) == "C:\\Mes téléchargements"
assert downloads.windows_path(user / "a/b", home) == "C:\\Mes téléchargements\\a\\b"
assert downloads.windows_path(Path("/etc"), home) is None

# Anciens téléchargements du compte navigateur : déplacés sans rien écraser.
user.mkdir()
(user / "photo.png").write_text("ancien")
old = work / "web/Downloads"
old.mkdir(parents=True)
(old / "photo.png").write_text("nouveau")
(old / "setup.exe").write_text("exe")
downloads.move_contents(old, user)
assert not old.exists()
assert (user / "photo.png").read_text() == "ancien"
assert (user / "photo (2).png").read_text() == "nouveau"
assert (user / "setup.exe").exists()

# Demandes du navigateur : seulement dans Téléchargements, fichier → son dossier.
(user / "sous").mkdir()
web_dl = downloads.BROWSER_DOWNLOADS
assert downloads.allowed(str(web_dl / "setup.exe"), user) == user
assert downloads.allowed(str(web_dl), user) == user
assert downloads.allowed(str(web_dl / "sous"), user) == user / "sous"
assert downloads.allowed(str(user / "sous/x.pdf"), user) == user / "sous"
assert downloads.allowed(str(web_dl / "../../etc"), user) is None
assert downloads.allowed("/etc/shadow", user) is None
assert downloads.allowed(str(home / "Documents"), user) is None
(user / "fuite").symlink_to("/etc")
assert downloads.allowed(str(web_dl / "fuite"), user) is None
assert downloads.allowed("", user) is None

# Côté navigateur : la demande arrive dans la boîte aux lettres.
spool = Path(os.environ["WINLUX_BROWSER_SPOOL"])
spool.mkdir()
assert downloads.request(str(web_dl / "setup.exe")) == 0
files = [p for p in spool.iterdir()]
assert len(files) == 1 and not files[0].name.startswith(".")
assert files[0].read_text() == str(web_dl / "setup.exe")
assert downloads.uri_path("file:///home/winlux-web/T%C3%A9l%C3%A9chargements/a%20b.zip") == "/home/winlux-web/Téléchargements/a b.zip"
assert downloads.uri_path("https://example.invalid/") is None

# mimeapps.list du navigateur : seul inode/directory change.
mime = work / "web/.config/mimeapps.list"
mime.parent.mkdir(parents=True, exist_ok=True)
mime.write_text("[Default Applications]\ntext/html=firefox.desktop\ninode/directory=thunar.desktop\n")
downloads.set_mime_default(mime, os.getuid(), os.getgid())
lines = mime.read_text().splitlines()
assert "text/html=firefox.desktop" in lines
assert "inode/directory=winlux-browser-folder.desktop;" in lines
assert "inode/directory=thunar.desktop" not in lines
print("OK: téléchargements du navigateur dans le dossier de l'utilisateur, ouverture du dossier limitée à Téléchargements")
