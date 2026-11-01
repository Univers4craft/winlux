#!/usr/bin/python3
"""Launch explicit per-user XDG startup applications once per session."""
from pathlib import Path
import shutil
from gi.repository import Gio, GLib

for path in sorted((Path.home()/".config/autostart").glob("*.desktop")):
    try:
        key=GLib.KeyFile();key.load_from_file(str(path),GLib.KeyFileFlags.NONE)
        def field(name):
            try:return key.get_string("Desktop Entry",name)
            except GLib.Error:return ""
        if field("Hidden").lower()=="true" or field("X-GNOME-Autostart-enabled").lower()=="false":continue
        if field("TryExec") and not shutil.which(field("TryExec")):continue
        if field("OnlyShowIn") and "XFCE" not in field("OnlyShowIn").split(";"):continue
        if "XFCE" in field("NotShowIn").split(";"):continue
        app=Gio.DesktopAppInfo.new_from_filename(str(path))
        if app:app.launch([],None)
    except (GLib.Error,OSError) as error:print(f"{path.name}: {error}")

# Arrêt anormal de la session précédente : « Signaler un problème » propose
# un rapport (une seule fois par arrêt, réglage Paramètres > Signalement).
if shutil.which("winlux-report"):
    try:Gio.Subprocess.new(["winlux-report","--system"],Gio.SubprocessFlags.NONE)
    except GLib.Error as error:print(f"winlux-report: {error}")
