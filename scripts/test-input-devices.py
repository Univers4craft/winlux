#!/usr/bin/python3
"""Tests hors ligne du clavier tactile (winlux-osk) et des réglages avancés de
la souris (winlux-mouse) : détection d'un clavier physique, icône de la barre,
touches envoyées, réglages enregistrés et appliqués par xinput, thème de
pointeurs recoloré, fichier lu par les programmes Windows. Avec DISPLAY, le
clavier est aussi construit en GTK dans les quatre langues, et l'écran de
connexion y tape un mot de passe."""
import json
import os
from pathlib import Path
import runpy
import struct
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp())
checks = 0


def check(condition, message):
    global checks
    if not condition:
        raise AssertionError(message)
    checks += 1


# ── Clavier tactile ─────────────────────────────────────────────────────────

KEYBOARD = """I: Bus=0011 Vendor=0001 Product=0001 Version=ab41
N: Name="AT Translated Set 2 keyboard"
H: Handlers=sysrq kbd event0 leds
B: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe

"""
NOT_KEYBOARDS = """I: Bus=0019 Vendor=0000 Product=0001 Version=0000
N: Name="Power Button"
H: Handlers=kbd event1
B: KEY=10000000000000 0

I: Bus=0003 Vendor=0627 Product=0001 Version=0001
N: Name="QEMU QEMU USB Tablet"
H: Handlers=mouse0 event2
B: KEY=70000 0 0 0 0

I: Bus=0003 Vendor=046d Product=c52b Version=0111
N: Name="Logitech USB Receiver Consumer Control"
H: Handlers=kbd event3
B: KEY=1 3f000303ff 0 0 483ffff17aff32d bfd4444600000000 1 130ff38b17c007 ffe77bfad9415fff febeffdfffefffff fffffffffffffffe

"""
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
os.environ["HOME"] = str(tmp / "home")
os.environ.pop("XDG_CONFIG_HOME", None)
os.environ["WINLUX_LANG"] = "fr"
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "etc/winlux").mkdir(parents=True)
devices = tmp / "proc/bus/input/devices"
osk = runpy.run_path(str(HERE / "winlux-osk.py"), run_name="osk_test")

devices.write_text(NOT_KEYBOARDS)
check(osk["keyboards"]() == [], "bouton d'alimentation, tablette ou touches multimédia ne sont pas des claviers")
check(osk["setting"]() == "auto" and osk["icon_wanted"](), "sans clavier physique, l'icône apparaît (réglage auto)")
devices.write_text(NOT_KEYBOARDS + KEYBOARD)
check(osk["keyboards"]() == ["AT Translated Set 2 keyboard"], osk["keyboards"]())
check(not osk["icon_wanted"](), "clavier branché : pas d'icône en mode auto")
osk["save_setting"]("always")
check((tmp / "home/.config/winlux/osk.conf").read_text() == "OSK_ICON=always\n", "réglage enregistré dans le compte")
check(osk["icon_wanted"](), "réglage Toujours : icône même avec un clavier")
osk["save_setting"]("never")
devices.write_text(NOT_KEYBOARDS)
check(not osk["icon_wanted"](), "réglage Jamais : aucune icône")
try:
    osk["save_setting"]("parfois")
    check(False, "valeur inconnue refusée")
except SystemExit:
    check(True, "")
osk["save_setting"]("auto")
check("<img>" in osk["tray_output"]() and "toggle</click>" in osk["tray_output"](), osk["tray_output"]())
devices.write_text(KEYBOARD)
check(osk["tray_output"]() == "<txt></txt><tool></tool>", "greffon vide quand l'icône n'est pas voulue")
check(osk["ids_with_icon"]([3, 1, 2, 8, 10, 4, 5, 6, 9, 7]) == [3, 1, 2, 8, 10, 12, 4, 5, 6, 9, 7], "icône avant le réseau")
check(osk["ids_with_icon"]([3, 1, 12, 2, 4, 7]) == [3, 1, 2, 12, 4, 7], "icône déplacée, jamais en double")
# Disposition : celle du système (XKB_LAYOUT), belge → AZERTY, britannique → QWERTY.
for layout, expected in (("de", "de"), ("es", "es"), ("be", "fr"), ("gb", "us"), ("it", "fr")):
    (tmp / "etc/winlux/input.conf").write_text(f"MOUSE_SPEED=50\nXKB_LAYOUT={layout}\n")
    check(osk["layout_name"]() == expected, (layout, osk["layout_name"]()))
check(osk["LAYOUTS"]["de"][0].startswith("qwertz") and osk["LAYOUTS"]["fr"][0].startswith("azerty"), "QWERTZ et AZERTY")
check("ñ" in osk["LAYOUTS"]["es"][1], "Ñ espagnol")
check(osk["xdotool_args"]("é", []) == ["xdotool", "type", "--clearmodifiers", "--", "é"], "caractère accentué tapé tel quel")
check(osk["xdotool_args"]("c", ["ctrl"]) == ["xdotool", "key", "--clearmodifiers", "ctrl+c"], "Ctrl + C")
check(osk["xdotool_args"]("e", ["super"]) == ["xdotool", "key", "--clearmodifiers", "super+e"], "Windows + E")
check(osk["xdotool_args"]("Tab", ["alt"]) == ["xdotool", "key", "--clearmodifiers", "alt+Tab"], "Alt + Tab")
check(osk["xdotool_args"](",", ["ctrl"])[-1] == "ctrl+comma", "symbole nommé en keysym")
check(osk["keysym"]("€") == "EuroSign" and osk["keysym"]("ß") == "U00DF", "keysyms hors ASCII")
check(all(key in osk["ACCENTS"] for key in "aeiouc"), "accents par appui long")
out = subprocess.run([sys.executable, "-B", str(HERE / "winlux-osk.py"), "detect"], capture_output=True, text=True,
                     env=dict(os.environ))
check(out.returncode == 0 and out.stdout.startswith("keyboard=1"), out.stdout + out.stderr)
# « need » (champs de mot de passe) : avec un clavier branché, rien ne s'ouvre.
out = subprocess.run([sys.executable, "-B", str(HERE / "winlux-osk.py"), "need"], capture_output=True, text=True,
                     env=dict(os.environ), timeout=20)
check(out.returncode == 0 and not out.stdout, "need avec clavier : aucune fenêtre " + out.stdout + out.stderr)
# Écran de verrouillage, contrôle de compte et installateur proposent le clavier visuel.
for script, needle in (("winlux-lock.py", "keyboard_widget"), ("winlux-uac.py", "physical_keyboard"),
                       ("winlux-installer.py", "setup_touch_keyboard")):
    check(needle in (HERE / script).read_text(encoding="utf-8"), f"clavier tactile absent de {script}")
uac = (HERE / "winlux-uac.py").read_text(encoding="utf-8")
check('"Clavier tactile": ("Touch keyboard", "Teclado táctil", "Bildschirmtastatur")' in uac, "UAC : 4 langues")


# Icône de la barre (tablette après installation, v0.2.16) : le greffon est
# écrit avant le lancement de xfce4-panel, et une barre déjà lancée est
# relancée quand on l'ajoute à chaud (elle ne le chargeait pas).
panel_bin = tmp / "panel-bin"
panel_bin.mkdir()
store = tmp / "xfconf.json"
store.write_text(json.dumps({"/panels/panel-1/plugin-ids": [3, 1, 2, 8, 10, 4, 5]}))
restarts = tmp / "panel-restarts.log"
(panel_bin / "xfconf-query").write_text(f"""#!/usr/bin/python3
import json, sys
data = json.load(open({str(store)!r}))
args = sys.argv[1:]
path = args[args.index("-p") + 1]
values = [args[i + 1] for i, a in enumerate(args) if a == "-s"]
if "-r" in args:
    for key in [k for k in data if k == path or k.startswith(path + "/")]:
        del data[key]
elif "-a" in args:
    data[path] = [int(v) for v in values]
elif values:
    if path not in data and "-n" not in args:
        sys.exit(1)
    data[path] = values[0]
else:
    if path not in data:
        sys.exit(1)
    value = data[path]
    print("\\n".join(str(v) for v in value) if isinstance(value, list) else value)
    sys.exit(0)
json.dump(data, open({str(store)!r}, "w"))
""")
(panel_bin / "pgrep").write_text("#!/bin/sh\nexit 0\n")
(panel_bin / "xfce4-panel").write_text(f"#!/bin/sh\necho \"$*\" >>{restarts}\n")
for tool in panel_bin.iterdir():
    tool.chmod(0o755)
panel_env = dict(os.environ, PATH=f"{panel_bin}:{os.environ['PATH']}", HOME=str(tmp / "home-panel"))
panel_env.pop("WINLUX_OSK_NO_RESTART", None)
(tmp / "proc/bus/input/devices").write_text(NOT_KEYBOARDS)
out = subprocess.run([sys.executable, "-B", str(HERE / "winlux-osk.py"), "panel"], capture_output=True, text=True,
                     env=dict(panel_env, WINLUX_OSK_NO_RESTART="1"), timeout=20)
data = json.loads(store.read_text())
check(out.returncode == 0 and data["/panels/panel-1/plugin-ids"] == [3, 1, 2, 8, 10, 12, 4, 5],
      f"greffon 12 avant le son et le réseau : {data.get('/panels/panel-1/plugin-ids')} {out.stderr}")
check(data.get("/plugins/plugin-12") == "genmon" and data.get("/plugins/plugin-12/command", "").endswith("tray"),
      "greffon genmon du clavier tactile")
check(not restarts.exists(), "barre relancée au démarrage de session (elle n'est pas encore lancée)")
store.write_text(json.dumps({"/panels/panel-1/plugin-ids": [3, 1, 2, 8, 10, 4, 5]}))
subprocess.run([sys.executable, "-B", str(HERE / "winlux-osk.py"), "panel"], env=panel_env, timeout=20)
check(restarts.exists() and restarts.read_text().strip() == "-r", "barre déjà lancée non relancée après ajout du greffon")
restarts.unlink()
subprocess.run([sys.executable, "-B", str(HERE / "winlux-osk.py"), "panel"], env=panel_env, timeout=20)
check(not restarts.exists(), "barre relancée sans changement")
for script in ("winlux-user-session.sh", "winlux-native-session.sh"):
    text = (HERE / script).read_text(encoding="utf-8")
    osk_at = text.find("winlux-osk panel")
    panel_at = text.find("/usr/bin/xfce4-panel --disable-wm-check")
    check(0 <= osk_at < panel_at, f"{script} : greffon du clavier écrit après le lancement de la barre")

# Clavier USB branché pendant la session (v0.2.16) : la disposition choisie
# est réappliquée, sinon le nouveau clavier tape en QWERTY.
import time
watch_root = tmp / "watch-root"
(watch_root / "proc/bus/input").mkdir(parents=True)
(watch_root / "etc/winlux").mkdir(parents=True)
(watch_root / "etc/winlux/input.conf").write_text("MOUSE_SPEED=50\nXKB_LAYOUT=fr\n")
(watch_root / "proc/bus/input/devices").write_text(NOT_KEYBOARDS)
watch_bin = tmp / "watch-bin"
watch_bin.mkdir()
xkb_log = tmp / "setxkbmap.log"
(watch_bin / "setxkbmap").write_text(f"#!/bin/sh\necho \"$*\" >>{xkb_log}\n")
(watch_bin / "setxkbmap").chmod(0o755)
# Faux xinput : la liste des périphériques du serveur X, lue dans un fichier.
(watch_bin / "xinput").write_text(f"#!/bin/sh\ncat {watch_bin / 'xinput-ids'} 2>/dev/null\n")
(watch_bin / "xinput").chmod(0o755)
(watch_bin / "xinput-ids").write_text("2\n3\n4\n")
watch_env = dict(os.environ, PATH=f"{watch_bin}:{os.environ['PATH']}", WINLUX_INPUT_ROOT=str(watch_root),
                 WINLUX_INPUT_INTERVAL="0.2", XDG_RUNTIME_DIR=str(tmp), DISPLAY=":98")
watcher = subprocess.Popen(["sh", str(HERE / "winlux-input-watch.sh")], env=watch_env)
try:
    time.sleep(0.8)
    check(not xkb_log.exists(), "disposition réappliquée sans rien brancher")
    second = subprocess.run(["sh", str(HERE / "winlux-input-watch.sh")], env=watch_env, timeout=5)
    check(second.returncode == 0, "deuxième surveillance sur le même affichage non refusée")
    (watch_root / "proc/bus/input/devices").write_text(NOT_KEYBOARDS + KEYBOARD)
    for _ in range(40):
        if xkb_log.exists():
            break
        time.sleep(0.1)
    check(xkb_log.exists() and xkb_log.read_text().split() == ["fr"],
          f"clavier branché à chaud : setxkbmap {xkb_log.read_text() if xkb_log.exists() else 'jamais appelé'}")
    # Le serveur X ajoute le clavier après le noyau (plus d'une seconde parfois) :
    # la disposition doit être réappliquée quand X l'ajoute, pas seulement au
    # changement de /proc (sinon le clavier arrivé dans X restait en QWERTY).
    time.sleep(1.5)
    xkb_log.unlink()
    time.sleep(1.5)
    check(not xkb_log.exists(), "disposition réappliquée sans changement")
    (watch_bin / "xinput-ids").write_text("2\n3\n4\n12\n")
    for _ in range(40):
        if xkb_log.exists():
            break
        time.sleep(0.1)
    check(xkb_log.exists() and xkb_log.read_text().split() == ["fr"],
          "clavier ajouté par X après le noyau : disposition non réappliquée")
finally:
    watcher.terminate()
    watcher.wait(timeout=5)
for script in ("winlux-user-session.sh", "winlux-native-session.sh"):
    check("/usr/bin/winlux-input-watch" in (HERE / script).read_text(encoding="utf-8"), f"{script} : pas de surveillance des claviers branchés")
check("'winlux-input-watch.sh:winlux-input-watch'" in (HERE / "stage-desktop-components.sh").read_text(), "winlux-input-watch absent de l'image")

# ── Souris ──────────────────────────────────────────────────────────────────

home = tmp / "home"
log = tmp / "xinput.log"
fake = tmp / "bin/xinput"
fake.parent.mkdir(parents=True)
fake.write_text(f"""#!/bin/sh
echo "$*" >>{log}
case "$1" in
--list) printf '9\\n11\\n12\\n' ;;
list-props)
    case "$2" in
    9) cat <<'EOF'
Device 'Logitech USB Optical Mouse':
	libinput Natural Scrolling Enabled (300):	0
	libinput Left Handed Enabled (301):	0
	libinput Accel Speed (302):	0.000000
	libinput Accel Profiles Available (303):	1, 1, 1
	libinput Accel Profile Enabled (304):	1, 0, 0
EOF
    ;;
    11) cat <<'EOF'
Device 'ELAN Touchpad':
	libinput Tapping Enabled (310):	1
	libinput Left Handed Enabled (301):	0
	libinput Accel Speed (302):	0.000000
EOF
    ;;
    12) echo "Device 'Power Button':" ;;
    esac ;;
esac
""")
fake.chmod(0o755)
os.environ["WINLUX_MOUSE_XINPUT"] = str(fake)
os.environ["WINLUX_CONTROL_ROOT"] = str(tmp)
os.environ["PATH"] = f"{tmp / 'bin'}:{os.environ['PATH']}"
(tmp / "etc/winlux/input.conf").write_text("MOUSE_SPEED=75\n")
mouse = runpy.run_path(str(HERE / "winlux-mouse.py"), run_name="mouse_test")
values = mouse["load"]()
check(values["speed"] == "75", "vitesse reprise du réglage Lente/Normale/Rapide existant")
status = dict(line.split("=", 1) for line in mouse["status"]().splitlines())
check(status["mouse_present"] == "1" and status["cap_precision"] == "1" and status["cap_natural"] == "1", status)
check(status["cap_left"] == "1", "gaucher : souris et pavé")
check([d for d, _ in mouse["mice"]()] == ["9"], "le pavé tactile n'est pas compté comme souris")


def mouse_cli(*args):
    return subprocess.run([sys.executable, "-B", str(HERE / "winlux-mouse.py"), *args], capture_output=True, text=True,
                          env=dict(os.environ, DISPLAY=""))


log.write_text("")
check(mouse_cli("set", "speed", "100").returncode == 0, "vitesse acceptée")
check("set-prop 9 libinput Accel Speed 1.00" in log.read_text(), log.read_text())
check("set-prop 11 libinput Accel Speed" not in log.read_text(), "la vitesse de la souris ne touche pas le pavé")
mouse_cli("set", "precision", "0")
check("set-prop 9 libinput Accel Profile Enabled 0 1 0" in log.read_text(), "précision désactivée : profil plat")
mouse_cli("set", "natural", "1")
check("set-prop 9 libinput Natural Scrolling Enabled 1" in log.read_text(), "défilement inversé")
mouse_cli("set", "left", "1")
check("set-prop 9 libinput Left Handed Enabled 1" in log.read_text() and
      "set-prop 11 libinput Left Handed Enabled 1" in log.read_text(), "gaucher sur tous les pointeurs")
for key, value in (("lines", "8"), ("double", "350"), ("trail", "5")):
    check(mouse_cli("set", key, value).returncode == 0, (key, value))
for key, value in (("speed", "101"), ("lines", "0"), ("double", "100"), ("trail", "1"), ("color", "rose"), ("inconnu", "1")):
    check(mouse_cli("set", key, value).returncode == 2, f"valeur refusée {key}={value}")
conf = (home / ".config/winlux/mouse.conf").read_text()
check("SPEED=100" in conf and "PRECISION=0" in conf and "LINES=8" in conf and "DOUBLE=350" in conf, conf)
ini = (home / ".local/share/winlux/mouse.ini").read_text()
check("double_click=350\n" in ini and "wheel_lines=8\n" in ini and "swap_buttons=1\n" in ini, ini)
check("trails=5\n" in ini and "speed=20\n" in ini, ini)
# Double-clic aussi pour Openbox (barre de titre).
rc = home / ".config/openbox/rc.xml"
rc.parent.mkdir(parents=True, exist_ok=True)
rc.write_text("<openbox_config><mouse><doubleClickTime>500</doubleClickTime></mouse></openbox_config>")
mouse_cli("set", "double", "400")
check("<doubleClickTime>400</doubleClickTime>" in rc.read_text(), rc.read_text())

# Xcursor : lecture, écriture et recoloration (corps noir, contour blanc).
black, white = 0xFF000000, 0xFFFFFFFF
image = (24, 2, 2, 0, 0, 0, [black, white, 0, black])
data = mouse["write_xcursor"]([image])
check(mouse["read_xcursor"](data) == [image], "aller-retour Xcursor")
recolored = mouse["recolor"](image[6], (255, 255, 255), (0, 0, 0), True)
check(recolored == [white, black, 0, white], [hex(p) for p in recolored])
lime = mouse["recolor"]([black], (160, 255, 0), (0, 0, 0), True)[0]
check(lime == 0xFFA0FF00, hex(lime))
source = home / ".local/share/icons/Adwaita/cursors"
source.mkdir(parents=True)
(source / "left_ptr").write_bytes(data)
(source / "default").symlink_to("left_ptr")
(home / ".icons/default").mkdir(parents=True)
(home / ".icons/default/index.theme").write_text("[Icon Theme]\nInherits=Adwaita\n")
name = mouse["build_theme"]("white")
built = home / ".local/share/icons" / name / "cursors"
check(name == "Winlux-Pointeur-white" and os.readlink(built / "default") == "left_ptr", name)
check(mouse["read_xcursor"]((built / "left_ptr").read_bytes())[0][6] == [white, black, 0, white], "pointeur blanc")
a11y = home / ".local/share/winlux/accessibility.ini"
a11y.write_bytes(b"# Param\xe8tres\ncontrast=2\npointer_size=3\n")
check(mouse_cli("set", "color", "black").returncode == 0, "couleur noire")
check(a11y.read_bytes() == b"# Param\xe8tres\ncontrast=2\npointer_size=3\npointer_color=1\n", a11y.read_bytes())
check("Inherits=Winlux-Pointeur-black" in (home / ".icons/default/index.theme").read_text(), "thème X par défaut")
check(mouse["base_theme"]() == "Adwaita", "la couleur « système » retrouve le thème d'origine")
mouse_cli("set", "color", "default")
check("Inherits=Adwaita" in (home / ".icons/default/index.theme").read_text(), "retour au thème du système")
check("pointer_color=0" in a11y.read_text(encoding="latin-1"), "pointeur des programmes Windows par défaut")


# ── Interface (avec un affichage) ───────────────────────────────────────────

if os.environ.get("DISPLAY"):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk

    def labels(widget):
        out = []
        if isinstance(widget, Gtk.Button) and widget.get_label():
            out.append(widget.get_label())
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children():
                out += labels(child)
        return out

    sent = []
    keys = osk["keyboard_widget"](send=lambda key, mods: sent.append((key, list(mods))), layout="de", on_close=lambda: None)
    shown = labels(keys)
    check("z" in shown and "ü" in shown and "⊞" in shown and "Ctrl" in shown, shown)
    keys.osk_press_char("a")
    check(sent[-1] == ("a", []), sent)
    keys.osk_toggle("ctrl")
    keys.osk_press_char("c")
    check(sent[-1] == ("c", ["ctrl"]), sent)
    keys.osk_press_char("v")
    check(sent[-1] == ("v", []), "Ctrl relâché après une combinaison")
    keys.osk_toggle("super")
    keys.osk_toggle("super")
    check(sent[-1] == ("Super_L", []), "Windows seul ouvre Démarrer")
    keys.osk_toggle("shift")
    keys.osk_press_char("A")
    check(sent[-1] == ("A", []) and not keys.osk_state["shift"], "Maj : une majuscule puis relâché")
    keys.osk_press_special("BackSpace")
    check(sent[-1] == ("BackSpace", []), sent)
    for lang, space in (("fr", "Espace"), ("en", "Space"), ("es", "Espacio"), ("de", "Leertaste")):
        out = subprocess.run([sys.executable, "-B", "-c",
                              "import runpy,sys;m=runpy.run_path(sys.argv[1]);import gi;from gi.repository import Gtk;"
                              "w=m['keyboard_widget'](send=lambda *a:None);"
                              "f=lambda x:[x.get_label()] if isinstance(x,Gtk.Button) else sum((f(c) for c in x.get_children()),[]) if isinstance(x,Gtk.Container) else [];"
                              "print('|'.join(l for l in f(w) if l))", str(HERE / "winlux-osk.py")],
                             capture_output=True, text=True, env=dict(os.environ, WINLUX_LANG=lang))
        check(space in out.stdout.split("|"), (lang, out.stdout[-200:], out.stderr[-400:]))

    # Écran de connexion : le clavier tactile tape dans le champ du mot de passe.
    os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
    lock = runpy.run_path(str(HERE / "winlux-lock.py"), run_name="lock_osk_test")
    module = lock["osk_module"]()
    check(module is not None and hasattr(module, "keyboard_widget"), "winlux-lock charge le clavier tactile")

    entry, submitted = Gtk.Entry(), []
    for key in ("S", "é", "c", "x", "BackSpace", "Left", "Left", "1", "Return"):
        lock["type_into"](entry, key, lambda: submitted.append(entry.get_text()))
    check(entry.get_text() == "S1éc" and submitted == ["S1éc"], (entry.get_text(), submitted))
    lock["type_into"](entry, "Escape", lambda: None)
    check(entry.get_text() == "", "Échap vide le champ")


    # Paramètres > Écran : définition, fréquence et échelle par écran.
    pages = runpy.run_path(str(HERE / "winlux-settings-pages.py"), run_name="screens_test")
    combos = {}

    class Screens(Gtk.Window, pages["ExtraPages"]):
        def __init__(self, status):
            super().__init__()
            self.body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            self.add(self.body)
            self.status, self.sent = status, []

        def execute(self, *args):
            self.sent.append(args)

        def combo(self, parent, title, items, active, changed, subtitle=None):
            combos[title] = (items, active, changed)

    two = {"display_count": "2", "display_layout": "extend", "display_1_name": "eDP-1", "display_1_internal": "1",
           "display_1_primary": "1", "display_1_modes": "1920x1080,1280x720", "display_1_mode": "1920x1080",
           "display_1_rates": "60.02,48.00", "display_1_rate": "60.02", "display_1_rate_auto": "1",
           "display_1_scale": "1.5", "display_1_scale_auto": "1", "display_2_name": "DP-1", "display_2_primary": "0",
           "display_2_modes": "2560x1440", "display_2_mode": "2560x1440", "display_2_rates": "59.95,143.97,119.88",
           "display_2_rate": "143.97", "display_2_rate_auto": "0", "display_2_scale": "1", "display_2_scale_auto": "0"}
    screens = Screens(two)
    screens.sections_screens()
    rates = combos["Fréquence d’actualisation"]
    check([label for _key, label in rates[0]] == ["Automatique", "60 Hz", "144 Hz", "120 Hz"], rates[0])
    check(rates[1] == "143.97", "fréquence choisie sur l'écran de jeu")
    rates[2]("119.88")
    check(screens.sent[-1] == ("display-set", "rate", "DP-1", "119.88"), screens.sent)
    combos["Disposition"][2]("only:DP-1")
    check(screens.sent[-1] == ("display-set", "layout", "only", "DP-1"), screens.sent)
    combos["Écran principal"][2]("DP-1")
    check(screens.sent[-1] == ("display-set", "primary", "DP-1"), screens.sent)
    combos["Échelle"][2]("1.25")
    check(screens.sent[-1] == ("display-set", "scale", "DP-1", "1.25"), screens.sent)
    combos.clear()
    vm = {"display_count": "1", "display_1_name": "Virtual-1", "display_1_modes": "1280x800,1024x768",
          "display_1_mode": "1280x800", "display_1_rates": "59.81", "display_1_rate": "59.81"}
    Screens(vm).sections_screens()
    check("Définition de l’écran" in combos and "Fréquence d’actualisation" not in combos,
          "une seule fréquence (VM) : choix masqué")
    check("Disposition" not in combos, "un seul écran : pas de disposition")

print(f"OK: clavier tactile et souris avancée ({checks} vérifications)")
