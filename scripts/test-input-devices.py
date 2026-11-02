#!/usr/bin/python3
"""Tests hors ligne du clavier tactile (winlux-osk) et des réglages avancés de
la souris (winlux-mouse) : détection d'un clavier physique, icône de la barre,
touches envoyées, réglages enregistrés et appliqués par xinput, thème de
pointeurs recoloré, fichier lu par les programmes Windows. Avec DISPLAY, le
clavier est aussi construit en GTK dans les quatre langues, et l'écran de
connexion y tape un mot de passe."""
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
check(osk["tray_output"]() == "<txt></txt>", "greffon vide quand l'icône n'est pas voulue")
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

print(f"OK: clavier tactile et souris avancée ({checks} vérifications)")
