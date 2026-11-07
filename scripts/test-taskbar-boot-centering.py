#!/usr/bin/python3
"""Barre centrée : le bouton Démarrer ne glisse plus au lancement de la barre.

À lancer dans un affichage X dédié (xvfb-run, 1920x1080). Démarre une vraie
xfce4-panel avec la configuration livrée, dans l'ordre de la barre centrée
(`winlux-control align-center`) et le vrai `winlux-tray`, puis relève la
position du bouton Démarrer toutes les 20 ms pendant le lancement, deux fois
de suite avec le même compte (premier démarrage, puis démarrage suivant).

Avant le correctif (#602), le contrepoids (greffon 13) était mesuré pendant que
les icônes de droite s'inséraient encore, dans des carrés de 16 px empilés :
il restait trop court jusqu'à sa mise à jour suivante, et Démarrer passait
environ 2,2 s à 110 px à gauche du centre avant d'y revenir.

Vérifie à chaque lancement : position finale au milieu de l'écran, et moins
de MAX_OFF secondes passées loin de cette position.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

directory = Path(__file__).resolve().parent
project = directory.parent
shipped = project / "integration/native/config"
CENTERED = (3, 13, 1, 15, 14, 2, 8, 10, 4, 5, 6, 9, 7)
MAX_OFF = 1.2
TOLERANCE = 8
# Boutons de la barre en magenta uni : sans fenêtre ouverte, le premier depuis
# la gauche est Démarrer.
MARK = ("\n.xfce4-panel button, .xfce4-panel button:checked, .xfce4-panel button:hover { background: #ff00ff; "
        "border-color: #ff00ff; border-radius: 0; box-shadow: none; }\n")


def genmon_in_xfconf(config):
    """xfce4-genmon-plugin 4.2+ (Ubuntu 26.04) lit sa commande dans xfconf, plus
    dans genmon-N.rc : même report que winlux-native-config."""
    import xml.etree.ElementTree as ET
    path = config / "xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml"
    tree = ET.parse(path)
    plugins = next(item for item in tree.getroot() if item.get("name") == "plugins")
    for plugin in plugins:
        name = plugin.get("name") or ""
        rc = config / "xfce4/panel" / f"genmon-{name[7:]}.rc"
        if plugin.get("value") != "genmon" or any(child.get("name") == "command" for child in plugin) or not rc.is_file():
            continue
        values = dict(line.split("=", 1) for line in rc.read_text(encoding="utf-8").splitlines() if "=" in line)
        for key, kind, value in (("command", "string", values["Command"]),
                                 ("use-label", "bool", "true" if values.get("UseLabel") == "1" else "false"),
                                 ("text", "string", values.get("Text", "")),
                                 ("update-period", "int", values.get("UpdatePeriod", "3000")),
                                 ("font", "string", values.get("Font", "DejaVu Sans 10"))):
            ET.SubElement(plugin, "property", {"name": key, "type": kind, "value": value})
    tree.write(path, encoding="UTF-8", xml_declaration=True)


def prepare(home):
    config = home / ".config"
    shutil.copytree(shipped, config)
    (config / "gtk-3.0/settings.ini").write_text("[Settings]\ngtk-theme-name=Adwaita\ngtk-enable-animations=false\n",
                                                 encoding="utf-8")
    with open(config / "gtk-3.0/gtk.css", "a", encoding="utf-8") as css:
        css.write(MARK)
    start = home / "winlux-start.desktop"
    start.write_text((project / "integration/native/applications/winlux-start.desktop").read_text(encoding="utf-8")
                     .replace("/usr/share/winlux/native/start.svg", str(project / "integration/native/start.svg")),
                     encoding="utf-8")
    ids = "".join(f'<value type="int" value="{number}"/>' for number in CENTERED)
    for path in list((config / "xfce4").rglob("*.rc")) + list((config / "xfce4").rglob("*.xml")):
        text = path.read_text(encoding="utf-8")
        text = text.replace("/usr/bin/winlux-clock", f"/bin/sh {directory}/winlux-clock.sh")
        text = text.replace("/usr/share/applications/winlux-start.desktop", str(start))
        text = text.replace("/usr/bin/winlux-tray", f"{sys.executable} -B {directory}/winlux-tray.py")
        text = re.sub(r'(<property name="plugin-ids" type="array">).*?(</property>)', lambda m: m[1] + ids + m[2], text)
        path.write_text(text, encoding="utf-8")
    genmon_in_xfconf(config)


def start_x(Gdk, root, width, height):
    pixbuf = Gdk.pixbuf_get_from_window(root, 0, height - 30, width, 1)
    if pixbuf is None:
        return None
    data, channels = pixbuf.get_pixels(), pixbuf.get_n_channels()
    for x in range(width):
        red, green, blue = data[x * channels:x * channels + 3]
        if abs(red - 255) + green + abs(blue - 255) < 60:
            return x
    return None


def boot(home, label, problems):
    import gi
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk
    env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"), XDG_CACHE_HOME=str(home / ".cache"),
               XDG_DATA_HOME=str(home / ".local/share"), WINLUX_TRAY_DIR=str(home / "tray"), WINLUX_LANG="fr")
    process = subprocess.Popen(["dbus-run-session", "--", "sh", "-c",
                                "openbox & sleep 1; xfce4-panel --disable-wm-check & echo go; wait"],
                               env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, start_new_session=True, text=True)
    samples = []
    try:
        process.stdout.readline()
        root = Gdk.get_default_root_window()
        width, height = root.get_width(), root.get_height()
        began = time.monotonic()
        while time.monotonic() - began < 8:
            samples.append((time.monotonic() - began, start_x(Gdk, root, width, height)))
            time.sleep(0.02)
    finally:
        try:
            os.killpg(process.pid, 15)
        except ProcessLookupError:
            pass
        process.wait(timeout=10)
    shown = [(moment, x) for moment, x in samples if x is not None]
    if not shown:
        problems.append(f"{label} : bouton Démarrer introuvable")
        return
    final = shown[-1][1]
    changes = [(moment, x) for index, (moment, x) in enumerate(shown) if index == 0 or x != shown[index - 1][1]]
    print(f"{label} : " + ", ".join(f"{moment:.2f} s x={x}" for moment, x in changes))
    # Groupe Démarrer + Rechercher + Vue des tâches (3 boutons, tasklist vide)
    # au milieu de l'écran : son bord gauche est entre 800 et 960.
    if not width // 2 - 160 < final < width // 2:
        problems.append(f"{label} : bouton Démarrer à x={final}, pas au milieu de l'écran")
    off = sum(next_moment - moment for (moment, x), (next_moment, _) in zip(shown, shown[1:])
              if abs(x - final) > TOLERANCE)
    print(f"{label} : {off:.2f} s loin de sa place")
    if off > MAX_OFF:
        problems.append(f"{label} : Démarrer passe {off:.2f} s loin de sa place (x={final}) au lancement de la barre")


def main():
    missing = [tool for tool in ("xfce4-panel", "openbox", "dbus-run-session", "xwininfo", "xprop", "xfconf-query")
               if not shutil.which(tool)]
    if missing:
        print("Barre réelle non vérifiée : " + ", ".join(missing) + " absent(s)")
        return 0
    problems = []
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp:
        home = Path(temp) / "compte"
        prepare(home)
        boot(home, "premier démarrage", problems)
        boot(home, "démarrage suivant", problems)
    for problem in problems:
        print("ÉCHEC : " + problem, file=sys.stderr)
    if not problems:
        print("Bouton Démarrer immobile au lancement de la barre centrée.")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
