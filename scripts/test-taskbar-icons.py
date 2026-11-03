#!/usr/bin/python3
"""Icônes de la barre des tâches native centrées dans leur bouton.

À lancer dans un affichage X dédié (xvfb-run, sans gestionnaire de fenêtres).
Démarre une vraie xfce4-panel avec la configuration livrée (gtk.css et
xfce4-panel.xml), Openbox et trois fenêtres à icône carrée symétrique, puis mesure
chaque bouton : l'icône doit avoir la même marge à gauche et à droite (à 2 px
près). Vérifié barre centrée et barre alignée à gauche (séparateur 3 non
extensible, comme `winlux-control align-left`).

Avant le correctif, le bouton carré (hauteur de la barre) était plus étroit
que l'icône plus les marges du thème : GTK collait le contenu à gauche et
l'icône débordait à droite (8 à 11 px à gauche, 0 à droite).

    test-taskbar-icons.py            (les deux alignements)
    test-taskbar-icons.py window     (interne : une fenêtre d'essai)
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
shipped = directory.parent / "integration/native/config"
TOLERANCE = 2
# Boutons repeints en magenta uni, sans arrondi, pour voir leurs bords exacts.
MARK = ("\n.xfce4-panel button, .xfce4-panel button:checked, .xfce4-panel button:hover { background: #ff00ff; border-color: #ff00ff; "
        "border-radius: 0; box-shadow: none; }\n")


def test_window():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GdkPixbuf, Gtk
    # Carré plein centré avec une marge transparente, comme une vraie icône.
    icon = GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB, True, 8, 64, 64)
    icon.fill(0x00000000)
    icon.new_subpixbuf(8, 8, 48, 48).fill(0x00C800FF)
    window = Gtk.Window(title="Essai barre")
    window.set_icon(icon)
    window.set_default_size(200, 120)
    window.move(50, 50)
    window.connect("destroy", Gtk.main_quit)
    window.show_all()
    Gtk.main()


def magenta(pixel):
    return abs(pixel[0] - 255) + pixel[1] + abs(pixel[2] - 255) < 60


def measure(pixbuf):
    """Boutons (magenta) de la bande basse et marges de leur contenu."""
    width, height = pixbuf.get_width(), pixbuf.get_height()
    data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()

    def pixel(x, y):
        offset = y * stride + x * channels
        return tuple(data[offset:offset + 3])
    rows = [y for y in range(height) if any(magenta(pixel(x, y)) for x in range(0, width, 2))]
    if not rows:
        return []
    top, bottom = rows[0], rows[-1]
    buttons, x = [], 0
    while x < width:
        if magenta(pixel(x, top + 1)):
            start = x
            while x < width and magenta(pixel(x, top + 1)):
                x += 1
            buttons.append((start, x - 1))
        else:
            x += 1
    found = []
    for left, right in buttons:
        content = [xx for yy in range(top, bottom + 1) for xx in range(left, right + 1)
                   if not magenta(pixel(xx, yy))]
        if right - left < 20 or right - left > 60 or not content:
            continue  # zone vide ou horloge (texte)
        green = any(pixel(xx, yy) == (0, 200, 0) for yy in range(top, bottom + 1) for xx in range(left, right + 1))
        found.append((left, right, min(content) - left, right - max(content), green))
    return found


def run(alignment, temp, problems):
    import gi
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk
    home = temp / alignment
    config = home / ".config"
    shutil.copytree(shipped, config)
    (config / "gtk-3.0/settings.ini").write_text(
        "[Settings]\ngtk-theme-name=Adwaita\ngtk-enable-animations=false\n", encoding="utf-8")
    with open(config / "gtk-3.0/gtk.css", "a", encoding="utf-8") as css:
        css.write(MARK)
    start = home / "winlux-start.desktop"
    start.write_text((directory.parent / "integration/native/applications/winlux-start.desktop").read_text(encoding="utf-8")
                     .replace("/usr/share/winlux/native/start.svg", str(directory.parent / "integration/native/start.svg")),
                     encoding="utf-8")
    for path in list((config / "xfce4").rglob("*.rc")) + list((config / "xfce4").rglob("*.xml")):
        text = path.read_text(encoding="utf-8")
        text = text.replace("/usr/bin/winlux-clock", f"/bin/sh {directory}/winlux-clock.sh")
        text = text.replace("/usr/share/applications/winlux-start.desktop", str(start))
        text = re.sub(r"/usr/bin/winlux-tray \w+", "true", text)
        if alignment == "gauche":
            text = text.replace('<property name="plugin-3" type="string" value="separator"><property name="expand" type="bool" value="true"/>',
                                '<property name="plugin-3" type="string" value="separator"><property name="expand" type="bool" value="false"/>')
        path.write_text(text, encoding="utf-8")
    env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(config),
               XDG_CACHE_HOME=str(home / ".cache"), XDG_DATA_HOME=str(home / ".local/share"))
    script = ("openbox & sleep 1; xfce4-panel --disable-wm-check & sleep 2; "
              + " ".join(f"{sys.executable} -B {Path(__file__).resolve()} window & sleep 0.5;" for _ in range(3))
              + " wait")
    process = subprocess.Popen(["dbus-run-session", "--", "sh", "-c", script], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        root = Gdk.get_default_root_window()
        width, height = root.get_width(), root.get_height()
        buttons = []
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            time.sleep(1)
            pixbuf = Gdk.pixbuf_get_from_window(root, 0, height - 48, width, 48)
            if pixbuf is None:
                continue
            buttons = measure(pixbuf)
            if sum(1 for button in buttons if button[4]) >= 3:
                time.sleep(1)
                buttons = measure(Gdk.pixbuf_get_from_window(root, 0, height - 48, width, 48))
                break
        windows = [button for button in buttons if button[4]]
        if len(windows) < 3:
            problems.append(f"barre {alignment} : {len(windows)} bouton(s) de fenêtre trouvé(s) sur 3")
        for left, right, margin_left, margin_right, green in buttons:
            what = "fenêtre" if green else "bouton"
            print(f"barre {alignment} · {what} x={left}-{right} : marge gauche {margin_left}, droite {margin_right}")
            if abs(margin_left - margin_right) > TOLERANCE:
                problems.append(f"barre {alignment} · {what} x={left}-{right} : icône décalée "
                                f"(marge gauche {margin_left} px, droite {margin_right} px)")
    finally:
        try:
            os.killpg(process.pid, 15)
        except ProcessLookupError:
            pass
        process.wait(timeout=10)


def check_balance(problems):
    """Barre centrée : le groupe Démarrer + fenêtres au milieu de l'écran (contrepoids 13)."""
    import ast
    source = (directory / "winlux-tray.py").read_text(encoding="utf-8")
    node = next(item for item in ast.parse(source).body if isinstance(item, ast.FunctionDef) and item.name == "balance_width")
    scope = {}
    exec(compile(ast.Module(body=[node], type_ignores=[]), "winlux-tray.py", "exec"), scope)
    balance_width = scope["balance_width"]
    # Mesure du portable de damien : 1920 px, bloc de droite (systray 10,
    # genmon 12/4/5/6/9, horloge 7) à partir de x=1641, groupe 1+2 de 237 px.
    right_ids = {10, 12, 4, 5, 6, 9, 7}
    children = [(10, 1641, 30), (12, 1671, 32), (4, 1703, 32), (5, 1735, 32), (6, 1767, 32), (9, 1799, 32), (7, 1831, 89),
                (None, 760, 48), (1, 703, 48)]
    width = balance_width(1920, children, right_ids)
    if width != 279:
        problems.append(f"contrepoids : {width} px au lieu de 279")
    # Bloc de gauche (bouton Widgets 11, 60 px) : le contrepoids le compense.
    if balance_width(1920, children + [(11, 0, 60)], right_ids, {11}) != 219:
        problems.append("contrepoids : le bouton Widgets à gauche n'est pas déduit")
    if balance_width(1920, [(1, 703, 48)], right_ids) != 0:
        problems.append("contrepoids : non nul sans bloc de droite")
    # Disposition [11?][sép. 3 extensible][13 = W][1][2][sép. 8 extensible][R] :
    # l'espace libre se partage à parts égales entre les deux séparateurs.
    for screen, left, group, right in ((1920, 0, 237, 279), (1366, 0, 140, 200), (3840, 60, 400, 500)):
        weight = balance_width(screen, [(10, screen - right, right), (11, 0, left)], {10}, {11})
        free = screen - left - weight - group - right
        center = left + free / 2 + weight + group / 2
        if abs(center - screen / 2) > 1:
            problems.append(f"barre centrée {screen} px : groupe centré à {center:.0f} au lieu de {screen / 2:.0f}")
    # winlux-control align-center place le 13 juste après le séparateur 3.
    with tempfile.TemporaryDirectory() as temp:
        temp = Path(temp)
        (temp / "bin").mkdir()
        log = temp / "xfconf.log"
        (temp / "bin/xfconf-query").write_text(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$XFCONF_LOG\"\n"
            "case \" $* \" in *' -s '*) exit 0 ;; *plugin-ids*) printf '3\\n1\\n2\\n8\\n10\\n4\\n5\\n6\\n9\\n7\\n' ;; esac\nexit 0\n")
        (temp / "bin/xfconf-query").chmod(0o755)
        env = dict(os.environ, PATH=f"{temp / 'bin'}:{os.environ['PATH']}", XFCONF_LOG=str(log),
                   WINLUX_CONTROL_ROOT=str(temp / "root"), WINLUX_CONTROL_NO_STATUS="1", WINLUX_CONTROL_TEST_LOG=str(temp / "commands"))
        subprocess.run(["sh", str(directory / "winlux-control.sh"), "align-center"], env=env, capture_output=True)
        order = [line for line in log.read_text().splitlines() if "plugin-ids -a" in line]
        ids = re.findall(r"-s (\d+)", order[-1]) if order else []
        if ids[:3] != ["3", "13", "1"]:
            problems.append(f"barre centrée : ordre {ids}, le contrepoids 13 doit suivre le séparateur 3")


def main():
    problems = []
    check_balance(problems)
    if problems:
        for problem in problems:
            print("ÉCHEC : " + problem, file=sys.stderr)
        return 1
    print("Contrepoids de la barre centrée : groupe au milieu de l'écran.")
    missing = [tool for tool in ("xfce4-panel", "openbox", "dbus-run-session") if not shutil.which(tool)]
    if missing:
        print("Barre réelle non vérifiée : " + ", ".join(missing) + " absent(s)")
        return 0
    problems = []
    # xfconfd peut encore écrire dans le dossier pendant son nettoyage : une
    # erreur de suppression n'est pas un échec du test.
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp:
        for alignment in ("centrée", "gauche"):
            run(alignment, Path(temp), problems)
    for problem in problems:
        print("ÉCHEC : " + problem, file=sys.stderr)
    if not problems:
        print("Icônes de la barre des tâches centrées dans leur bouton.")
    return 1 if problems else 0


if __name__ == "__main__":
    if sys.argv[1:] == ["window"]:
        test_window()
    else:
        sys.exit(main())
