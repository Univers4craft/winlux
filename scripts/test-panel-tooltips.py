#!/usr/bin/python3
"""Aucune infobulle « (genmon) » ni « micro-barre » dans la barre des tâches (#645).

Sans balise <tool>, genmon affiche au survol son infobulle par défaut :
« (genmon) », la commande et sa période. Le contrepoids de la barre centrée
(greffon 13, `winlux-tray balance`) n'en donnait pas : la bulle apparaissait
juste à côté de Démarrer. Et la fenêtre des greffons externes
(#XfcePanelWindowWrapper) porte aussi la classe xfce4-panel : la bordure du
haut de gtk.css y était redessinée, ce qui doublait le trait sur la largeur
du contrepoids.

    test-panel-tooltips.py           sorties genmon et gtk.css (rapide)
    test-panel-tooltips.py --barre   en plus, vraie xfce4-panel sous X
                                     (xvfb-run, xfce4-panel, xfce4-genmon-plugin, xdotool)
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
CONFIG = ROOT / "integration/native/config"
failures = []


def ok(condition, label):
    print(("OK    " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


def has_tooltip_tag(output):
    begin, end = output.find("<tool>"), output.find("</tool>")
    return begin >= 0 and end > begin


# ── 1. Chaque sortie genmon livrée porte une balise <tool> ──────────────────
temp = Path(tempfile.mkdtemp(prefix="winlux-tooltips-"))
env = dict(os.environ, WINLUX_TRAY_DIR=str(temp / "tray"), XDG_CACHE_HOME=str(temp / "cache"),
           HOME=str(temp), WINLUX_LANG="fr")
for kind in ("balance", "network", "audio", "battery", "notifications"):
    result = subprocess.run([sys.executable, "-B", str(SCRIPTS / "winlux-tray.py"), kind], env=env,
                            capture_output=True, text=True, timeout=30)
    ok(result.returncode == 0 and has_tooltip_tag(result.stdout),
       f"winlux-tray {kind} : balise <tool> présente ({result.stdout.strip()[:80]!r})")
clock = subprocess.run(["sh", str(SCRIPTS / "winlux-clock.sh")], env=env, capture_output=True, text=True, timeout=30)
ok(has_tooltip_tag(clock.stdout), "winlux-clock : balise <tool> présente")
# Clavier tactile masqué : greffon vide, mais sans infobulle par défaut.
osk = (SCRIPTS / "winlux-osk.py").read_text(encoding="utf-8")
returns = re.findall(r'return f?"(<(?:txt|img)>.*)"', osk[osk.index("def tray_output"):])
ok(returns and all("<tool>" in value for value in returns), "winlux-osk : chaque sortie genmon a sa balise <tool>")
widgets = (SCRIPTS / "winlux-widgets.py").read_text(encoding="utf-8")
body = widgets[widgets.index("def tray():"):]
body = body[:body.index("\ndef ")]
prints = re.findall(r"print\((.*?)\)\n", body, re.S)
ok(prints and all("<tool>" in value for value in prints), "winlux-widgets : chaque sortie genmon a sa balise <tool>")

# ── 2. La fenêtre des greffons externes ne redessine pas la bordure ─────────
css = (CONFIG / "gtk-3.0/gtk.css").read_text(encoding="utf-8")
block = css[css.index("/* Winlux panel */"):css.index("/* End Winlux panel */")]
ok(re.search(r"#XfcePanelWindowWrapper\s*\{[^}]*border-top:\s*none", block) is not None,
   "gtk.css : bordure retirée des greffons externes, dans le bloc resynchronisé à chaque session")


# ── 3. Vraie barre : ni bulle au survol du contrepoids, ni trait doublé ─────
def plugin_windows():
    """Fenêtres des greffons externes : {id du greffon: (x, y, largeur, hauteur)}."""
    tree = subprocess.run(["xwininfo", "-root", "-tree"], capture_output=True, text=True).stdout
    found = {}
    for line in tree.splitlines():
        match = re.match(r"\s*(0x[0-9a-f]+) .*?\s(\d+)x(\d+)\+-?\d+\+-?\d+\s+\+(-?\d+)\+(-?\d+)", line)
        if not match:
            continue
        window, width, height, x, y = match.group(1), *map(int, match.groups()[1:])
        try:
            pid = int(subprocess.run(["xprop", "-id", window, "_NET_WM_PID"], capture_output=True,
                                     text=True).stdout.rsplit("=", 1)[1])
            arguments = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
        except (ValueError, IndexError, OSError):
            continue
        if Path(arguments[0].decode(errors="replace")).name.startswith("wrapper-"):
            plugin = next((int(item) for item in arguments[1:4] if item.isdigit()), None)
            if plugin is not None and width > 1:
                found[plugin] = (x, y, width, height)
    return found


def real_panel():
    import gi
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk
    home = temp / "barre"
    config = home / ".config"
    shutil.copytree(CONFIG, config)
    (config / "gtk-3.0/settings.ini").write_text("[Settings]\ngtk-theme-name=Adwaita\n", encoding="utf-8")
    bin_dir = home / "bin"
    bin_dir.mkdir()
    tray = bin_dir / "tray"
    # Contrepoids : la vraie sortie de winlux-tray balance, avec une largeur fixe
    # de 150 px (le calcul de la largeur est couvert par test-taskbar-boot-centering).
    tray.write_text(f"#!/bin/sh\nout=$({sys.executable} -B {SCRIPTS / 'winlux-tray.py'} \"$1\")\n"
                    "test \"$1\" = balance && out=$(printf '%s' \"$out\" | sed 's#spacer-[0-9]*\\.svg#spacer-150.svg#')\n"
                    "printf '%s\\n' \"$out\"\n", encoding="utf-8")
    tray.chmod(0o755)
    (temp / "tray").mkdir(exist_ok=True)
    (temp / "tray/spacer-150.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg" width="150" height="1" '
                                              'viewBox="0 0 150 1"><rect width="150" height="1" fill="none"/></svg>')
    for path in list((config / "xfce4").rglob("*.rc")) + list((config / "xfce4").rglob("*.xml")):
        text = path.read_text(encoding="utf-8")
        text = text.replace("/usr/bin/winlux-tray", str(tray))
        text = text.replace("/usr/bin/winlux-clock", f"/bin/sh {SCRIPTS / 'winlux-clock.sh'}")
        if path.name == "xfce4-panel.xml":
            # Ordre de la barre centrée (winlux-control align-center) : 3 13 1 …
            ids = "3 13 1 2 8 4 5 6 9 7".split()
            text = re.sub(r'<property name="plugin-ids" type="array">.*?</property>',
                          '<property name="plugin-ids" type="array">'
                          + "".join(f'<value type="int" value="{i}"/>' for i in ids) + "</property>", text)
            # genmon 4.2+ lit xfconf, pas les genmon-N.rc : reporter chaque .rc
            # dans le XML comme le fait winlux-native-config à l'ouverture de session.
            for rc in sorted((config / "xfce4/panel").glob("genmon-*.rc")):
                number = rc.stem.split("-")[1]
                values = dict(line.split("=", 1) for line in rc.read_text(encoding="utf-8").splitlines() if "=" in line)
                values["Command"] = values["Command"].replace("/usr/bin/winlux-tray", str(tray)).replace(
                    "/usr/bin/winlux-clock", f"/bin/sh {SCRIPTS / 'winlux-clock.sh'}")
                props = ("".join(f'<property name="{key}" type="{kind}" value="{value}"/>' for key, kind, value in (
                    ("command", "string", values["Command"]), ("use-label", "bool", "false"), ("text", "string", ""),
                    ("update-period", "int", values.get("UpdatePeriod", "2000")), ("font", "string", values.get("Font", "")))))
                text = text.replace(f'<property name="plugin-{number}" type="string" value="genmon"/>',
                                    f'<property name="plugin-{number}" type="string" value="genmon">{props}</property>')
        path.write_text(text, encoding="utf-8")
    panel_env = dict(env, HOME=str(home), XDG_CONFIG_HOME=str(config), XDG_CACHE_HOME=str(home / ".cache"),
                     XDG_DATA_HOME=str(home / ".local/share"))
    process = subprocess.Popen(["dbus-run-session", "--", "sh", "-c",
                                "openbox & sleep 1; xfce4-panel --disable-wm-check & wait"],
                               env=panel_env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               start_new_session=True)
    try:
        spacer = None
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline and spacer is None:
            time.sleep(1)
            spacer = plugin_windows().get(13)
            if spacer and spacer[2] < 100:
                spacer = None
        ok(spacer is not None, f"contrepoids (greffon 13) affiché : {spacer}")
        if spacer is None:
            return
        time.sleep(3)  # premier rafraîchissement de genmon : infobulle posée
        x, y, width, height = spacer
        root = Gdk.get_default_root_window()

        def grab(top, rows):
            pixbuf = Gdk.pixbuf_get_from_window(root, 0, top, root.get_width(), rows)
            return pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()

        def pixel(shot, px, py):
            data, stride, channels = shot
            offset = py * stride + px * channels
            return tuple(data[offset:offset + 3])

        # Bordure : la ligne juste sous le trait du haut de la barre reste du fond.
        top = y - 1
        shot = grab(top, 4)
        inside = pixel(shot, x + width // 2, 1)
        outside = pixel(shot, max(0, x - 40), 1)
        ok(inside == outside, f"pas de trait doublé sur le contrepoids ({inside} sous le trait, {outside} à côté)")
        # Survol : rien ne doit apparaître au-dessus de la barre.
        above = max(0, y - 200)
        before = grab(above, y - above)
        subprocess.run(["xdotool", "mousemove", str(x + width // 2 - 5), str(y + height // 2)])
        time.sleep(0.3)
        subprocess.run(["xdotool", "mousemove", str(x + width // 2), str(y + height // 2 + 2)])
        time.sleep(2.5)
        after = grab(above, y - above)
        ok(before[0] == after[0], "survol du contrepoids : aucune infobulle")
    finally:
        try:
            os.killpg(process.pid, 15)
        except ProcessLookupError:
            pass
        process.wait(timeout=10)


if "--barre" in sys.argv:
    missing = [tool for tool in ("xfce4-panel", "xdotool", "xwininfo", "xprop", "openbox", "dbus-run-session")
               if shutil.which(tool) is None]
    if missing or not os.environ.get("DISPLAY"):
        print("IGNORÉ vraie barre : " + (", ".join(missing) + " absent(s)" if missing else "pas d'affichage X"))
    else:
        real_panel()

shutil.rmtree(temp, ignore_errors=True)
if failures:
    print(f"\n{len(failures)} échec(s)")
    sys.exit(1)
print("\nTout est bon.")
