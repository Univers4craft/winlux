#!/usr/bin/python3
"""Luminosité (winlux-brightness) : un seul chemin pour le curseur des réglages
rapides, Paramètres > Écran et les touches du clavier.

Écran de portable simulé (/sys/class/backlight factice, deux pilotes dont un
inerte), écran de bureau en DDC/CI (ddcutil factice), écran sans rien de
pilotable (rampes gamma de winlux-ambiance, simulées), et machine sans aucun
contrôle (curseur caché). Le compte n'écrit pas dans /sys : le relais root
est appelé. Puis la page Écran de Paramètres, sous Xvfb si DISPLAY est là.
"""
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-brightness.py"
failures = []


def ok(condition, message):
    print(("ok   " if condition else "ÉCHEC ") + message)
    if not condition: failures.append(message)


work = Path(tempfile.mkdtemp(prefix="winlux-brightness-"))
bin_dir = work / "bin"
bin_dir.mkdir()
(bin_dir / "winlux-root").write_text(f"""#!/bin/sh
echo "$@" >>{work}/relay.log
# Le vrai relais tourne en root, qui écrit malgré le 0444 : simulé ici.
find "$WINLUX_SYS_ROOT/sys/class/backlight" -name brightness -exec chmod 644 {{}} + 2>/dev/null
WINLUX_BRIGHTNESS_AS=root WINLUX_CONTROL_RELAYED=1 exec {sys.executable} {TOOL} "$3" "$4"
""")
(bin_dir / "ddcutil").write_text(f"""#!/bin/sh
echo "$@" >>{work}/ddcutil.log
case "$*" in
  "detect --brief") printf 'Display 1\\n   I2C bus:  /dev/i2c-4\\n   Monitor:  DEL:DELL P2419H\\n'; exit 0 ;;
  *"getvcp 10") printf 'VCP 10 C 70 100\\n'; exit 0 ;;
  *"setvcp 10"*) exit 0 ;;
esac
exit 1
""")
for tool in ("winlux-root", "ddcutil"): (bin_dir / tool).chmod(0o755)
socket = work / "root.sock"
socket.write_text("")
base = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}", WINLUX_ROOT_SOCKET=str(socket),
            WINLUX_BRIGHTNESS_RUN=str(work / "run"), WINLUX_BRIGHTNESS_NO_OSD="1",
            WINLUX_BRIGHTNESS_DAEMON="0", HOME=str(work / "home"), XDG_CONFIG_HOME=str(work / "home/.config"),
            WINLUX_AMBIANCE_FAKE=str(work / "gamma.txt"), WINLUX_LANG="fr")
base.pop("WINLUX_CONTROL_RELAYED", None)


def machine(name, lights=()):
    root = work / name
    for device, kind, maximum, value in lights:
        folder = root / "sys/class/backlight" / device
        folder.mkdir(parents=True)
        (folder / "type").write_text(kind + "\n")
        (folder / "max_brightness").write_text(f"{maximum}\n")
        (folder / "brightness").write_text(f"{value}\n")
    (root / "sys/class/backlight").mkdir(parents=True, exist_ok=True)
    return root


def run(root, *args, who="user", display=False, **extra):
    env = dict(base, WINLUX_SYS_ROOT=str(root), WINLUX_BRIGHTNESS_AS=who, WINLUX_BRIGHTNESS_RUN=str(root / "run"), **extra)
    if not display: env.pop("DISPLAY", None)
    result = subprocess.run([sys.executable, str(TOOL), *args], capture_output=True, text=True, env=env, timeout=60)
    try: return json.loads(result.stdout.strip().splitlines()[-1])
    except (IndexError, ValueError): return {"ok": False, "raw": result.stdout + result.stderr}


def raw(root, device):
    return int((root / "sys/class/backlight" / device / "brightness").read_text())


# ── Portable : acpi_video0 (firmware) et intel_backlight (raw) ──
laptop = machine("portable", [("acpi_video0", "firmware", 15, 15), ("intel_backlight", "raw", 1000, 1000)])
state = run(laptop, "get")
ok(state.get("method") == "backlight" and state.get("device") == "acpi_video0" and state.get("percent") == 100,
   f"portable : rétroéclairage firmware choisi en premier, comme le démon d'alimentation ({state})")
set60 = run(laptop, "set", "60", who="root")
ok(set60.get("ok") and raw(laptop, "acpi_video0") == 9 and raw(laptop, "intel_backlight") == 600,
   f"portable : 60 % écrit dans les deux pilotes, le bon répond toujours ({set60})")
ok("BRIGHTNESS=60" in (laptop / "etc/winlux/power.conf").read_text(),
   "portable : valeur gardée pour le démon d'alimentation (redémarrage, profil)")
(laptop / "etc/winlux/power.conf").write_text("PROFILE=saver\nBRIGHTNESS=50\n")
run(laptop, "set", "35", who="root")
ok("BRIGHTNESS=50" in (laptop / "etc/winlux/power.conf").read_text(),
   "économie d'énergie : préférence corrigée du facteur 70 % (35 % affiché = 50 % voulu)")
up = run(laptop, "set", "+10", who="root")
down = run(laptop, "set", "-90", who="root")
ok(up.get("percent") == 45 and down.get("percent") == 5, f"pas relatifs, jamais sous 5 % ({up}, {down})")
bad = run(laptop, "set", "beaucoup", who="root")
ok(not bad.get("ok") and bad.get("error") == "value", "valeur absurde refusée")
# Compte ordinaire : /sys/class/backlight n'est pas inscriptible → relais root.
for device in ("acpi_video0", "intel_backlight"):
    (laptop / "sys/class/backlight" / device / "brightness").chmod(0o444)
relayed = run(laptop, "set", "80")
log = (work / "relay.log").read_text() if (work / "relay.log").exists() else ""
ok("exec winlux-brightness set 80" in log, f"compte ordinaire : écriture par le relais root ({log.strip()})")
for device in ("acpi_video0", "intel_backlight"):
    (laptop / "sys/class/backlight" / device / "brightness").chmod(0o644)
# Le faux relais écrit en « root » dans les fichiers factices.
ok(relayed.get("ok") and raw(laptop, "intel_backlight") == 800, f"relais : la valeur est appliquée ({relayed})")
# Touche : le démon d'alimentation n'est pas là, le pas se fait ici.
key = run(laptop, "key", "down", who="root")
ok(key.get("ok") and key.get("percent") == 70, f"touche luminosité − : 80 → 70 % ({key})")
key = run(laptop, "key", "up", who="root", WINLUX_BRIGHTNESS_DAEMON="1")
ok(key.get("ok") and key.get("percent") == 80, f"touche + avec un démon qui ne fait rien : le pas est fait quand même ({key})")

# ── PC de bureau, écran DDC/CI ──
desktop = machine("bureau")
detected = run(desktop, "detect-ddc", who="root")
ok(detected.get("ok") and detected.get("displays") == [{"bus": 4, "max": 100}], f"DDC/CI : écran détecté sur i2c-4 ({detected})")
denied = run(desktop, "detect-ddc")
ok(not denied.get("ok"), "détection DDC/CI réservée à root")
state = run(desktop, "get")
ok(state.get("method") == "ddc" and state.get("percent") == 100, f"DDC/CI : curseur proposé ({state})")
(work / "relay.log").unlink()
ddc = run(desktop, "set", "40")
ok(ddc.get("ok") and "exec winlux-brightness set 40" in (work / "relay.log").read_text()
   and "--bus 4 setvcp 10 40" in (work / "ddcutil.log").read_text(), f"DDC/CI : réglé par le relais root ({ddc})")
ok(run(desktop, "get").get("percent") == 40, "DDC/CI : la valeur reste affichée pour le compte")

# ── Écran sans rétroéclairage ni DDC/CI : luminosité logicielle (gamma) ──
plain = machine("vm")
if os.environ.get("DISPLAY"):
    soft = run(plain, "get", display=True)
    ok(soft.get("method") == "soft" and soft.get("percent") == 100, f"sans rétroéclairage : luminosité logicielle ({soft})")
    dimmed = run(plain, "set", "3", display=True)
    gamma = [float(v) for v in (work / "gamma.txt").read_text().split()]
    ok(dimmed.get("percent") == 10 and max(gamma) <= 0.101, f"logicielle : jamais sous 10 %, rampes assombries ({dimmed}, {gamma})")
    key = run(plain, "key", "up", display=True)
    ok(key.get("percent") == 15, f"touche + sur écran sans rétroéclairage, pas de 5 % en bas ({key})")
    none = run(plain, "get", display=True, WINLUX_AMBIANCE_FAKE_SUPPORTED="0")
    ok(none.get("method") == "none", f"pilote sans rampe gamma : aucun contrôle ({none})")
none = run(plain, "get")
ok(none.get("method") == "none" and none.get("percent") is None, f"aucun contrôle : method=none, curseur caché ({none})")
nothing = run(plain, "set", "50")
ok(not nothing.get("ok") and nothing.get("error") == "none", "aucun contrôle : réglage refusé proprement")

# ── Relais root : seules les commandes prévues passent ──
root_tool = runpy.run_path(str(HERE / "winlux-root.py"), run_name="brightness_root_test")
level = root_tool["exec_level"]
for args, expected in ((["winlux-brightness", "set", "60"], "user"), (["winlux-brightness", "detect-ddc"], "user"),
                       (["winlux-brightness", "set", "+10"], None), (["winlux-brightness", "set", "60", "x"], None),
                       (["winlux-brightness", "get", "/etc/shadow"], None), (["winlux-brightness"], None)):
    ok(level(args) == expected, f"relais root : {' '.join(args)} → {expected or 'refusé'}")

# ── Branchements : un seul chemin partout ──
popup = (HERE / "winlux-popup.py").read_text()
ok("brightnessctl" not in popup.split("def brightness_tool")[1] and '"set",str(round(self.light.get_value()))' in popup,
   "réglages rapides : le curseur passe par winlux-brightness (plus brightnessctl, refusé au compte)")
rc = (HERE.parent / "integration/native/config/openbox/rc.xml").read_text()
ok('key="XF86MonBrightnessUp"' in rc and "winlux-brightness key up" in rc and "winlux-brightness key down" in rc,
   "touches de luminosité : même chemin, avec l'indicateur")
control = (HERE / "winlux-control.sh").read_text()
ok("then winlux-brightness set +10 --osd" in control, "Paramètres > Alimentation : boutons sur le même chemin")
stage = (HERE / "stage-desktop-components.sh").read_text()
ok(" winlux-brightness " in stage.split("winlux-root winlux-wifi", 1)[1].split("; do", 1)[0] + " ", "winlux-brightness installé dans l'image")
ok('"ddcutil"' in (HERE / "prepare-desktop-packages.py").read_text() and "i2c_dev" in (HERE / "hardware-modules.list").read_text(),
   "image : ddcutil et i2c-dev pour les écrans de bureau")
ok("winlux-brightness detect-ddc" in (HERE / "winlux-system-services.sh").read_text(), "démarrage : écrans DDC/CI détectés")


# ── Paramètres > Écran : curseur avec contrôle, caché sans ──
def settings_page(root, who):
    os.environ.update({k: v for k, v in base.items() if k.startswith(("WINLUX_", "PATH", "HOME", "XDG_"))})
    os.environ["WINLUX_SYS_ROOT"] = str(root)
    os.environ["WINLUX_BRIGHTNESS_RUN"] = str(root / "run")
    os.environ["WINLUX_BRIGHTNESS_AS"] = who
    os.environ["WINLUX_SETTINGS_PAGE"] = "8"
    settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="settings_brightness_test")
    window = settings["Settings"]()
    Gtk = settings["Gtk"]
    while Gtk.events_pending(): Gtk.main_iteration_do(False)
    scales = []

    def walk(widget):
        if isinstance(widget, Gtk.Scale) and widget.get_name() == "winlux-brightness": scales.append(widget)
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): walk(child)
    walk(window.body)
    return window, scales, Gtk


if os.environ.get("DISPLAY"):
    for device in ("acpi_video0", "intel_backlight"):
        (laptop / "sys/class/backlight" / device / "brightness").write_text("12\n" if device == "acpi_video0" else "800\n")
    window, scales, Gtk = settings_page(laptop, "root")
    ok(len(scales) == 1 and round(scales[0].get_value()) == 80, f"Paramètres > Écran : curseur Luminosité à 80 % ({[s.get_value() for s in scales]})")
    if scales:
        scales[0].set_value(45)
        deadline = time.time() + 5
        while time.time() < deadline and raw(laptop, "intel_backlight") != 450:
            Gtk.main_iteration_do(False)
            time.sleep(0.02)
        ok(raw(laptop, "intel_backlight") == 450, "Paramètres > Écran : bouger le curseur change vraiment la luminosité")
    window.destroy()
    os.environ["WINLUX_AMBIANCE_FAKE_SUPPORTED"] = "0"
    window, scales, Gtk = settings_page(plain, "user")
    ok(not scales, "Paramètres > Écran : sans aucun contrôle, pas de curseur inerte")
    window.destroy()

if failures:
    print(f"\n{len(failures)} échec(s)")
    sys.exit(1)
print("\nLuminosité : tout est bon")
