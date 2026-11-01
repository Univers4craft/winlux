#!/usr/bin/python3
"""Bureau > clic droit > Coller : colle sur le Bureau les fichiers copiés ou
coupés dans l'Explorateur Winlux, Thunar ou une autre application.

Le presse-papiers X11 (« x-special/gnome-copied-files », puis
« text/uri-list ») est le même que celui des programmes Windows grâce au pont
`winlux --clipboard-bridge`. Couper = déplacer, comme sous Windows. La copie
passe par Thunar (fenêtre de progression, conflits de noms) et retombe sur une
copie locale si Thunar ne répond pas.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    "Rien à coller": ("Nothing to paste", "Nada que pegar", "Nichts einzufügen"),
    "Le presse-papiers ne contient aucun fichier. Copiez ou coupez d’abord un fichier dans l’Explorateur.": (
        "The clipboard holds no file. Copy or cut a file in File Explorer first.",
        "El portapapeles no contiene ningún archivo. Copie o corte primero un archivo en el Explorador.",
        "Die Zwischenablage enthält keine Datei. Kopieren oder schneiden Sie zuerst eine Datei im Explorer aus."),
    "Collage incomplet": ("Paste incomplete", "Pegado incompleto", "Einfügen unvollständig"),
    "Certains fichiers n’ont pas pu être collés sur le Bureau.": (
        "Some files couldn't be pasted on the Desktop.",
        "Algunos archivos no se pudieron pegar en el Escritorio.",
        "Einige Dateien konnten nicht auf dem Desktop eingefügt werden."),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def parse_clipboard(text):
    """(chemins, couper) d'un contenu gnome-copied-files ou uri-list."""
    lines = [line.strip() for line in text.replace("\r", "\n").split("\n")]
    lines = [line for line in lines if line and not line.startswith("#")]
    cut = False
    if lines and lines[0] in ("cut", "copy"):
        cut = lines.pop(0) == "cut"
    paths = []
    for line in lines:
        if line.startswith("file://"):
            parsed = urlparse(line)
            if parsed.netloc not in ("", "localhost"):
                continue
            paths.append(Path(unquote(parsed.path)))
        elif line.startswith("/"):
            paths.append(Path(line))
    return paths, cut


def read_clipboard():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, Gtk
    clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
    for target in ("x-special/gnome-copied-files", "text/uri-list"):
        data = clipboard.wait_for_contents(Gdk.Atom.intern(target, False))
        if data is None or data.get_length() <= 0:
            continue
        raw = data.get_data()
        text = raw.decode("utf-8", "replace") if isinstance(raw, (bytes, bytearray)) else str(raw)
        paths, cut = parse_clipboard(text)
        if paths:
            return paths, cut
    return [], False


def desktop_dir():
    try:
        from gi.repository import GLib
        special = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_DESKTOP)
        if special and special != str(Path.home()):
            return Path(special)
    except Exception:
        pass
    return Path.home() / "Bureau"


def unique_target(directory, name):
    target = directory / name
    if not target.exists() and not target.is_symlink():
        return target
    stem, suffix = (name, "") if name.startswith(".") or "." not in name else name.rsplit(".", 1)
    suffix = "." + suffix if suffix else ""
    n = 2
    while True:
        candidate = directory / f"{stem} ({n}){suffix}"
        if not candidate.exists() and not candidate.is_symlink():
            return candidate
        n += 1


def local_paste(paths, destination, cut):
    """Copie ou déplace sans Thunar ; un nom déjà pris devient « nom (2) »."""
    failed = 0
    for source in paths:
        try:
            if cut and source.parent.resolve() == destination.resolve():
                continue
            target = unique_target(destination, source.name)
            if cut:
                shutil.move(str(source), str(target))
            elif source.is_dir() and not source.is_symlink():
                if destination.resolve().is_relative_to(source.resolve()):
                    raise OSError("copie d’un dossier dans lui-même")
                shutil.copytree(source, target, symlinks=True)
            else:
                shutil.copy2(source, target, follow_symlinks=False)
        except (OSError, shutil.Error, ValueError):
            failed += 1
    return failed


def thunar_paste(paths, destination, cut):
    """Délègue à Thunar (progression, remplacement) ; faux s'il ne répond pas."""
    method = "MoveInto" if cut else "CopyInto"
    sources = "[" + ",".join("'" + str(p).replace("\\", "\\\\").replace("'", "\\'") + "'" for p in paths) + "]"
    target = str(destination)
    try:
        result = subprocess.run(
            ["gdbus", "call", "--session", "--dest", "org.xfce.FileManager",
             "--object-path", "/org/xfce/FileManager",
             "--method", f"org.xfce.FileManager.{method}",
             target, sources, target, os.environ.get("DISPLAY", ""), ""],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
        return result.returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def notify(title, body):
    try:
        subprocess.Popen(["notify-send", "-i", "edit-paste", title, body],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except OSError:
        pass


def main(argv):
    destination = Path(argv[1]) if len(argv) > 1 else desktop_dir()
    destination.mkdir(parents=True, exist_ok=True)
    paths, cut = read_clipboard()
    paths = [p for p in paths if p.exists() or p.is_symlink()]
    if not paths:
        notify(_t("Rien à coller"), _t("Le presse-papiers ne contient aucun fichier. Copiez ou coupez d’abord un fichier dans l’Explorateur."))
        return 1
    if cut:
        # Couper puis coller au même endroit ne change rien.
        paths = [p for p in paths if p.parent.resolve() != destination.resolve()]
        if not paths:
            return 0
    if os.environ.get("WINLUX_PASTE_LOCAL") != "1" and thunar_paste(paths, destination, cut):
        return 0
    if local_paste(paths, destination, cut):
        notify(_t("Collage incomplet"), _t("Certains fichiers n’ont pas pu être collés sur le Bureau."))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
