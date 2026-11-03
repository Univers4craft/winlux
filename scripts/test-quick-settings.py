#!/usr/bin/python3
"""Réglages rapides Win+A : tuiles selon le matériel, curseurs, actions, langues.

À lancer sous Xvfb (xvfb-run -a /usr/bin/python3 -B scripts/test-quick-settings.py).
Le matériel est simulé par WINLUX_SYS_ROOT (faux /sys et /proc) et les commandes
(nmcli, bluetoothctl, winlux-brightness, winlux-ambiance, ip) par de faux programmes
qui notent leurs arguments.
"""
import os
from pathlib import Path
import re
import runpy
import stat
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-quick-"))
bin_dir = work / "bin"
bin_dir.mkdir()
log = work / "commands.log"
fakes = {
    "nmcli": 'case "$*" in "-t -f WIFI radio") echo enabled;; *"connection show --active"*) echo "802-11-wireless:Maison\\:5G";; esac',
    "bluetoothctl": 'case "$1" in show) echo "Controller 00:11 (public)"; echo "	Powered: yes";; esac',
    # Luminosité : winlux-brightness (test-brightness.py teste l'outil lui-même).
    "winlux-brightness": 'case "$1" in get) if [ -d "$WINLUX_SYS_ROOT/sys/class/backlight/intel_backlight" ]; then '
                         'echo \'{"ok": true, "method": "backlight", "percent": 40}\'; else echo \'{"ok": true, "method": "none", "percent": null}\'; fi;; esac',
    "winlux-ambiance": 'case "$1" in status) printf "supported=1\\nnightlight=off\\n";; esac',
    "ip": 'echo "default via 10.0.2.2 dev eth0"',
}
for name, body in fakes.items():
    path = bin_dir / name
    path.write_text(f'#!/bin/sh\necho "{name} $*" >> "{log}"\n{body}\nexit 0\n')
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


def sys_root(name, laptop):
    root = work / name
    (root / "proc").mkdir(parents=True)
    (root / "sys/class/net/eth0").mkdir(parents=True)
    (root / "proc/cpuinfo").write_text("processor : 0\nflags : fpu sse2" + ("" if laptop else " hypervisor") + "\n")
    if laptop:
        for directory in ("sys/class/ieee80211/phy0", "sys/class/net/wlan0/wireless", "sys/class/bluetooth/hci0",
                          "sys/class/backlight/intel_backlight", "sys/class/power_supply/BAT0"):
            (root / directory).mkdir(parents=True)
        (root / "sys/class/power_supply/BAT0/capacity").write_text("76\n")
        (root / "sys/class/power_supply/BAT0/status").write_text("Discharging\n")
    return root


class FakeModes:
    applied = []

    @staticmethod
    def detect():
        return ("ppd", ["saver", "balanced", "performance"], "balanced")

    @classmethod
    def apply(cls, mode, source):
        cls.applied.append((mode, source))
        return True


def load(root, lang="fr"):
    os.environ.update(WINLUX_SYS_ROOT=str(root), WINLUX_LANG=lang, XDG_CONFIG_HOME=str(work / ("config-" + root.name)),
                      PATH=f"{bin_dir}:{os.environ['PATH']}")
    return runpy.run_path(str(HERE / "winlux-popup.py"), run_name="quick_test")


BUILT = []


def build(module, modes=None):
    app = module["Popup"]("quick")
    # Deux panneaux dans le même test : un identifiant chacun (D-Bus).
    BUILT.append(app)
    app.set_application_id(f"org.winlux.PopupTest.Quick{len(BUILT)}")
    app.register(None)
    app.build()
    if modes is not None: app.modes_module = modes
    app.apply_quick(module["quick_state"](app.hardware, app.modes_module))
    return app


def tile_keys(app):
    return [key for key, _ in app.tile_layout]


def pump(module, seconds=0.6):
    """Laisse tourner GTK le temps que les commandes des tuiles se terminent."""
    loop = module["GLib"].MainLoop()
    module["GLib"].timeout_add(int(seconds * 1000), loop.quit)
    loop.run()


# 1. Machine virtuelle sans Wi-Fi ni batterie : « Ethernet virtuel », rien de grisé.
module = load(sys_root("vm", laptop=False))
vm = build(module)
assert not vm.hardware["wifi"] and not vm.hardware["battery"] and vm.in_vm
assert tile_keys(vm) == ["ethernet", "night", "accessibility"], tile_keys(vm)
assert vm.tiles["ethernet"][2].get_text() == "Ethernet virtuel"
assert vm.tiles["ethernet"][0].get_active(), "Ethernet connecté (route par défaut) = tuile allumée"
assert not vm.light_row.get_visible(), "Aucun contrôle de luminosité : pas de curseur inerte"
vm.refresh_quick()
assert not vm.battery_button.get_visible(), "Pas de batterie : pas d'indicateur de batterie"
assert all(button.get_sensitive() for button, _, _ in vm.tiles.values()), "Aucune tuile grisée"

# 2. Portable : Wi-Fi (nom du réseau), Bluetooth, mode avion, économiseur, éclairage nocturne.
module = load(sys_root("laptop", laptop=True))
laptop = build(module, FakeModes)
assert not laptop.in_vm
assert tile_keys(laptop) == ["wifi", "bluetooth", "airplane", "saver", "night", "accessibility"], tile_keys(laptop)
assert laptop.tiles["wifi"][2].get_text() == "Maison:5G", laptop.tiles["wifi"][2].get_text()
assert laptop.tiles["wifi"][0].get_active() and laptop.tiles["bluetooth"][0].get_active()
assert laptop.tiles["wifi"][1] is not None and laptop.tiles["airplane"][1] is None, "Flèche de détail sur Wi-Fi seulement"
assert not laptop.tiles["airplane"][0].get_active() and not laptop.tiles["saver"][0].get_active()
assert laptop.light_row.get_no_show_all() and laptop.light_row.get_visible() and round(laptop.light.get_value()) == 40
assert laptop.light_value.get_text() == "40", "valeur de la luminosité affichée, comme le volume"
laptop.refresh_quick()
assert laptop.battery_button.get_visible() and laptop.battery_text.get_text() == "76 %"

# 3. Tuiles : de vraies commandes, puis relecture de l'état.
log.write_text("")
laptop.tiles["wifi"][0].set_active(False)
pump(module)
assert "nmcli radio wifi off" in log.read_text(), log.read_text()
log.write_text("")
laptop.tiles["airplane"][0].set_active(True)
pump(module)
text = log.read_text()
assert "nmcli radio all off" in text and "bluetoothctl power off" in text, text
assert module["AIRPLANE"].exists(), "Mode avion mémorisé"
assert laptop.tiles["airplane"][0].get_active() and not laptop.tiles["wifi"][0].get_active(), "Avion : Wi-Fi éteint"
laptop.tiles["airplane"][0].set_active(False)
pump(module)
assert "nmcli radio all on" in log.read_text() and not module["AIRPLANE"].exists()
laptop.tiles["saver"][0].set_active(True)
pump(module)
assert FakeModes.applied[-1] == ("saver", "ppd"), FakeModes.applied
log.write_text("")
laptop.tiles["night"][0].set_active(True)
pump(module)
assert "winlux-ambiance toggle" in log.read_text()
log.write_text("")
laptop.light.set_value(65)
pump(module)
assert "winlux-brightness set 65" in log.read_text(), log.read_text()

# 4. Entrées : Win+A, icônes réseau / son / batterie de la barre.
tray = (HERE / "winlux-tray.py").read_text(encoding="utf-8")
assert '"/usr/bin/winlux-popup quick"' in tray
tools = (HERE / "winlux-system-tools.py").read_text(encoding="utf-8")
assert 'page == "quick" and shutil.which("winlux-popup")' in tools
rc = (HERE.parent / "integration/native/config/openbox/rc.xml").read_text(encoding="utf-8")
assert re.search(r'key="W-a".*winlux-(system-tools|popup) quick', rc)

# 5. Saisie du pointeur avec owner_events=True (sinon aucun clic dans le panneau).
source = (HERE / "winlux-popup.py").read_text(encoding="utf-8")
assert "Gdk.SeatCapabilities.POINTER,True" in source and "POINTER,False" not in source

# 6. Quatre langues pour chaque texte, et les tuiles traduites.
for key, row in module["_TR"].items():
    assert len(row) == 3 and all(row), key
for lang, expected in (("en", "Airplane mode"), ("es", "Modo avión"), ("de", "Flugzeugmodus")):
    output = subprocess.run([sys.executable, "-c", "import runpy,sys;m=runpy.run_path(sys.argv[1],run_name='t');print(m['_t']('Mode avion'))",
                             str(HERE / "winlux-popup.py")], env=dict(os.environ, WINLUX_LANG=lang), capture_output=True, text=True)
    assert output.stdout.strip() == expected, (lang, output.stdout, output.stderr)

print("OK: réglages rapides (VM, portable, tuiles, curseurs, entrées, langues)")
