#!/usr/bin/env python3
"""Pilotes du matériel réel dans l'image et diagnostic winlux-hardware-check.

- stage-kernel-modules.py embarque des dossiers entiers de pilotes, respecte
  les exclusions et ramène les dépendances d'un module (modules.dep) même
  hors des dossiers listés ;
- check-image-hardware.py fait échouer la construction si un pilote ou un
  firmware essentiel manque, ou si un module n'est pas dans modules.dep ;
- winlux-hardware-check signale un appareil sans pilote, un firmware
  manquant, un pavé tactile ou une batterie absents d'un portable, en
  quatre langues ;
- les listes réelles : chaque pilote essentiel est bien embarqué par l'un des
  mécanismes de build-live-iso.sh (vérifié sur un vrai noyau quand la machine
  en a un : WINLUX_HOST_MODULES ou /lib/modules/*-generic).
Politique : docs/MATERIEL.md.
"""
import glob
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
RELEASE = "7.0.0-42-generic"
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        print(f"ÉCHEC : {label}", file=sys.stderr)
        raise SystemExit(1)
    checks += 1
    print(f"  OK     {label}")


def write(path, text=""):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def run(*args, env=None):
    return subprocess.run([sys.executable, "-B", *map(str, args)], capture_output=True, text=True,
                          env=dict(os.environ, **(env or {})), timeout=120)


def fake_kernel(base):
    """Noyau de construction factice : quelques modules et leur modules.dep."""
    modules = base / "host" / RELEASE
    files = {
        "kernel/drivers/ata/ahci.ko.zst": "kernel/drivers/ata/libahci.ko.zst",
        "kernel/drivers/ata/libahci.ko.zst": "",
        "kernel/drivers/md/raid456.ko.zst": "kernel/crypto/async_tx/async_xor.ko.zst kernel/crypto/xor.ko.zst",
        "kernel/drivers/md/dm-vdo/dm-vdo.ko.zst": "",
        "kernel/crypto/async_tx/async_xor.ko.zst": "kernel/crypto/xor.ko.zst",
        "kernel/crypto/xor.ko.zst": "",
        "kernel/crypto/ccm.ko.zst": "",
        "kernel/drivers/pinctrl/intel/pinctrl-sunrisepoint.ko.zst": "",
        "kernel/drivers/hid/uhid.ko.zst": "",
        "kernel/fs/binfmt_misc.ko.zst": "",
    }
    for name in files:
        write(modules / name, "ZSTD " + name)
    write(modules / "modules.dep", "".join(f"{name}: {deps}\n" for name, deps in files.items()))
    write(modules / "modules.builtin", "kernel/drivers/ata/ata_piix.ko\n")
    return base / "host"


print("Dossiers entiers de pilotes (stage-kernel-modules.py)")
with tempfile.TemporaryDirectory() as temporary:
    base = Path(temporary)
    host = fake_kernel(base)
    stage = base / "stage"
    image = stage / "lib/modules" / RELEASE
    # Déjà embarqué décompressé par une autre boucle de build-live-iso.sh.
    write(image / "kernel/drivers/ata/libahci.ko", "RAW")
    listing = base / "dirs.list"
    write(listing, "# commentaire\ndrivers/ata\ndrivers/md\n!drivers/md/dm-vdo\ndrivers/pinctrl\nfs/binfmt_misc.ko\n")
    result = run(SCRIPTS / "stage-kernel-modules.py", stage, RELEASE, listing, "--modules", host)
    ok(result.returncode == 0, "copie sans erreur : " + result.stdout.strip())
    ok((image / "kernel/drivers/ata/ahci.ko.zst").read_text() == "ZSTD kernel/drivers/ata/ahci.ko.zst",
       "module copié tel qu'Ubuntu le livre (.ko.zst)")
    ok((image / "kernel/drivers/md/raid456.ko.zst").is_file() and (image / "kernel/drivers/pinctrl/intel/pinctrl-sunrisepoint.ko.zst").is_file(),
       "sous-dossiers parcourus (RAID, broches Intel Skylake)")
    ok(not (image / "kernel/drivers/md/dm-vdo/dm-vdo.ko.zst").exists(), "« ! » exclut un sous-dossier")
    ok((image / "kernel/crypto/xor.ko.zst").is_file() and (image / "kernel/crypto/async_tx/async_xor.ko.zst").is_file(),
       "dépendances ramenées hors des dossiers listés (raid456 → xor)")
    ok(not (image / "kernel/crypto/ccm.ko.zst").exists(), "rien d'autre hors des dossiers listés")
    ok((image / "kernel/fs/binfmt_misc.ko.zst").is_file(), "un fichier seul peut être listé (binfmt_misc)")
    ok(not (image / "kernel/drivers/ata/libahci.ko.zst").exists() and (image / "kernel/drivers/ata/libahci.ko").read_text() == "RAW",
       "module déjà présent décompressé : pas de doublon")
    ok(run(SCRIPTS / "stage-kernel-modules.py", stage, RELEASE, listing, "--modules", host).returncode == 0,
       "deuxième passage sans erreur")

    print("Contrôle de l'image (check-image-hardware.py)")
    essentials = base / "essentials.list"
    write(essentials, "module ahci\nmodule ata_piix\nmodule pinctrl-sunrisepoint\nmodule nom_disparu\nfirmware i915/*_guc*\nfirmware intel/sof*/*\n")
    write(stage / "usr/lib/firmware/i915/tgl_guc_70.bin.zst", "fw")
    write(stage / "usr/lib/firmware/intel/sof-ipc4/tgl/sof-tgl.ri", "fw")
    (stage / "lib/firmware").symlink_to("../usr/lib/firmware")
    dep = "".join(f"{p.relative_to(image)}:\n" for p in sorted(image.rglob("*.ko*")))
    write(image / "modules.dep", dep)

    def check():
        return run(SCRIPTS / "check-image-hardware.py", stage, RELEASE, "--essentials", essentials, "--modules", host)

    result = check()
    ok(result.returncode == 0, "image complète acceptée")
    ok("nom_disparu" in result.stderr, "module inconnu du noyau signalé sans échec")
    sunrise = image / "kernel/drivers/pinctrl/intel/pinctrl-sunrisepoint.ko.zst"
    sunrise.rename(sunrise.with_suffix(".bak"))
    result = check()
    ok(result.returncode == 1 and "module pinctrl-sunrisepoint" in result.stderr, "pilote essentiel disparu : construction refusée")
    sunrise.with_suffix(".bak").rename(sunrise)
    write(image / "modules.dep", dep.replace("kernel/drivers/ata/ahci.ko.zst:\n", ""))
    result = check()
    ok(result.returncode == 1 and "modules.dep" in result.stderr and "ahci" in result.stderr, "module absent de modules.dep refusé")
    write(image / "modules.dep", dep)
    (stage / "usr/lib/firmware/i915/tgl_guc_70.bin.zst").unlink()
    result = check()
    ok(result.returncode == 1 and "firmware i915/*_guc*" in result.stderr, "firmware essentiel disparu : construction refusée")

print("Diagnostic winlux-hardware-check")
with tempfile.TemporaryDirectory() as temporary:
    hw = Path(temporary) / "hw"
    sysfs = hw / "sys"
    modules = hw / "lib/modules" / RELEASE
    write(modules / "modules.alias", "alias pci:v00008086d0000A0E8sv*sd*bc*sc*i* intel_lpss_pci\n")
    drivers = sysfs / "bus/pci/drivers"

    def pci(slot, cls, vendor, device, driver=None, modalias=""):
        d = sysfs / "bus/pci/devices" / slot
        write(d / "class", cls); write(d / "vendor", vendor); write(d / "device", device); write(d / "modalias", modalias)
        if driver:
            (drivers / driver).mkdir(parents=True, exist_ok=True)
            (d / "driver").symlink_to(drivers / driver)

    pci("0000:00:02.0", "0x030000", "0x8086", "0x9a49", "i915")
    pci("0000:00:15.0", "0x0c8000", "0x8086", "0xa0e8", None, "pci:v00008086d0000A0E8sv00001028sd00000A20bc0Csc80i00")
    pci("0000:02:00.0", "0x020000", "0x1969", "0xe0b1", None, "pci:v00001969d0000E0B1sv*")
    pci("0000:00:00.0", "0x060000", "0x8086", "0x9a14")  # pont hôte : jamais signalé
    write(sysfs / "bus/i2c/devices/i2c-ELAN0001:00/name", "ELAN0001:00")
    write(sysfs / "class/dmi/id/chassis_type", "10")
    write(sysfs / "class/dmi/id/sys_vendor", "Dell Inc.")
    write(sysfs / "class/dmi/id/product_name", "Latitude 5420")
    write(hw / "proc/asound/cards", " 0 [sofhdadsp      ]: sof-hda-dsp - sof-hda-dsp\n")
    write(hw / "proc/bus/input/devices", 'I: Bus=0011\nN: Name="AT Translated Set 2 keyboard"\n\n')
    (sysfs / "class/backlight/intel_backlight").mkdir(parents=True)
    log = Path(temporary) / "dmesg.txt"
    write(log, "[ 3.1] iwlwifi 0000:00:14.3: Direct firmware load for iwlwifi-so-a0-gf-a0-89.ucode failed with error -2\n")
    env = {"WINLUX_HW_ROOT": str(hw), "WINLUX_KERNEL_LOG": str(log), "WINLUX_KERNEL_RELEASE": RELEASE,
           "WINLUX_LANG": "fr", "PATH": "/nonexistent"}
    result = run(SCRIPTS / "winlux-hardware-check.py", env=env)
    out = result.stdout
    ok(result.returncode == 1, "problèmes trouvés : code de sortie 1")
    ok("Dell Inc. Latitude 5420" in out, "machine nommée (DMI)")
    ok("[PCI 8086:a0e8]" in out and "intel_lpss_pci présent mais pas chargé" in out, "pilote présent mais pas chargé (modules.alias)")
    ok("[PCI 1969:e0b1]" in out and "Installer les pilotes complets" in out, "aucun pilote : renvoi vers les pilotes complets")
    ok("8086:9a14" not in out, "pont hôte sans pilote non signalé")
    ok("[I2C ELAN0001]" in out, "pavé tactile I2C (ACPI) sans pilote signalé")
    ok("iwlwifi-so-a0-gf-a0-89.ucode" in out, "firmware manquant lu dans le journal du noyau")
    ok("Aucun pavé tactile détecté sur ce portable" in out and "Batterie non détectée" in out, "portable : pavé tactile et batterie absents signalés")
    ok("Affichage accéléré" in out and "(i915)" in out and "Son : sof-hda-dsp" in out and "intel_backlight" in out,
       "carte graphique, son et luminosité reconnus")
    report = json.loads(run(SCRIPTS / "winlux-hardware-check.py", "--json", env=env).stdout)
    ok(len(report["unsupported"]) == 3 and report["firmware"] == ["iwlwifi-so-a0-gf-a0-89.ucode"], "rapport JSON")
    saved = Path(temporary) / "rapport.txt"
    run(SCRIPTS / "winlux-hardware-check.py", "--save", saved, env=env)
    ok(saved.read_text().startswith("Diagnostic du matériel Winlux"), "--save écrit le rapport")
    for lang, title in (("en", "Winlux hardware check"), ("es", "Diagnóstico de hardware de Winlux"), ("de", "Winlux-Hardwarediagnose")):
        ok(run(SCRIPTS / "winlux-hardware-check.py", env=dict(env, WINLUX_LANG=lang)).stdout.startswith(title), f"rapport en {lang}")
    tool = runpy.run_path(str(SCRIPTS / "winlux-hardware-check.py"), run_name="winlux_hardware_check")
    ok(all(len(row) == 4 and all(row) for row in tool["T"].values()), "chaque message existe en fr, en, es et de")
    del env["WINLUX_KERNEL_LOG"]

print("Les deux portables de damien : profil et éléments à installer")


def machine(base, vendor, product, pci_list, usb_list, inputs):
    hw = base / "hw"
    sysfs = hw / "sys"
    drivers = sysfs / "bus/pci/drivers"
    for slot, cls, ident, driver in pci_list:
        d = sysfs / "bus/pci/devices" / slot
        v, p = ident.split(":")
        write(d / "class", cls); write(d / "vendor", "0x" + v); write(d / "device", "0x" + p)
        if driver:
            (drivers / driver).mkdir(parents=True, exist_ok=True)
            (d / "driver").symlink_to(drivers / driver)
    for number, (ident, cls, driver) in enumerate(usb_list, 1):
        d = sysfs / f"bus/usb/devices/1-{number}"
        v, p = ident.split(":")
        write(d / "idVendor", v); write(d / "idProduct", p)
        i = sysfs / f"bus/usb/devices/1-{number}:1.0"
        write(i / "bInterfaceClass", cls)
        if driver:
            (sysfs / "bus/usb/drivers" / driver).mkdir(parents=True, exist_ok=True)
            (i / "driver").symlink_to(sysfs / "bus/usb/drivers" / driver)
    write(sysfs / "class/dmi/id/chassis_type", "10")
    write(sysfs / "class/dmi/id/sys_vendor", vendor)
    write(sysfs / "class/dmi/id/product_name", product)
    write(sysfs / "class/power_supply/BAT1/type", "Battery")
    write(sysfs / "class/power_supply/BAT1/capacity", "80")
    write(hw / "proc/cpuinfo", "vendor_id\t: GenuineIntel\n")
    write(hw / "proc/bus/input/devices", "".join(f'N: Name="{n}"\n\n' for n in inputs))
    write(hw / "proc/asound/cards", " 0 [PCH            ]: HDA-Intel - HDA Intel PCH\n")
    write(hw / "var/lib/dpkg/status", "Package: hello\nStatus: install ok installed\n")
    return json.loads(run(SCRIPTS / "winlux-hardware-check.py", "--json",
                          env={"WINLUX_HW_ROOT": str(hw), "WINLUX_KERNEL_RELEASE": RELEASE, "WINLUX_KERNEL_LOG": "/dev/null",
                               "WINLUX_LANG": "fr", "PATH": "/nonexistent"}).stdout)


with tempfile.TemporaryDirectory() as temporary:
    # MSI GE73 Raider RGB 8RF : Core i7-8750H (UHD 630) + GTX 1070 (Optimus),
    # Killer E2500 (alx), Killer 1550 (iwlwifi), clavier RGB SteelSeries.
    msi = machine(Path(temporary), "Micro-Star International Co., Ltd.", "GE73 Raider RGB 8RF",
                  [("0000:00:02.0", "0x030000", "8086:3e9b", "i915"), ("0000:01:00.0", "0x030000", "10de:1ba1", "nouveau"),
                   ("0000:3d:00.0", "0x020000", "1969:e0b1", "alx"), ("0000:00:14.3", "0x028000", "8086:a370", "iwlwifi"),
                   ("0000:00:1f.3", "0x040300", "8086:a348", "snd_hda_intel")],
                  [("1038:1122", "03", "usbhid"), ("8087:0aaa", "e0", "btusb")],
                  ["SynPS/2 Synaptics TouchPad", "AT Translated Set 2 keyboard"])
    prof = msi["profile"]
    ok(prof["kind"] == "laptop" and prof["hybrid"] and prof["nvidia"] and prof["touchpad"] and prof["battery"],
       "MSI GE73 8RF : portable, graphique hybride Intel + NVIDIA, pavé tactile, batterie")
    ok({"microcode", "nvidia", "vaapi", "rgb", "fwupd"} <= set(msi["recommend"]),
       "MSI GE73 8RF : microcode, pilote NVIDIA, VA-API, OpenRGB (clavier SteelSeries), fwupd")
    ok(not msi["unsupported"], "MSI GE73 8RF : Ethernet Killer, Wi-Fi Killer et son reconnus")

with tempfile.TemporaryDirectory() as temporary:
    # Acer Aspire 3 (A315-56, Ice Lake) : Iris Plus, Realtek RTL8821CE, pavé ELAN en I2C.
    acer = machine(Path(temporary), "Acer", "Aspire A315-56",
                   [("0000:00:02.0", "0x030000", "8086:8a56", "i915"), ("0000:01:00.0", "0x028000", "10ec:c821", "rtw88_8821ce"),
                    ("0000:00:15.0", "0x0c8000", "8086:34e8", "intel-lpss"), ("0000:00:1f.3", "0x040380", "8086:34c8", "sof-audio-pci-intel-icl")],
                   [("0bda:c821", "e0", "btusb")],
                   ["ELAN0504:00 04F3:3091 Touchpad", "AT Translated Set 2 keyboard"])
    prof = acer["profile"]
    ok(prof["kind"] == "laptop" and not prof["hybrid"] and not prof["nvidia"] and prof["touchpad"],
       "Acer Aspire 3 : portable Intel seul, pavé tactile I2C")
    ok("vaapi" in acer["recommend"] and "microcode" in acer["recommend"] and "nvidia" not in acer["recommend"] and "rgb" not in acer["recommend"],
       "Acer Aspire 3 : microcode et VA-API Intel, ni NVIDIA ni RGB")
    out = run(SCRIPTS / "winlux-hardware-check.py", "--items",
              env={"WINLUX_HW_ROOT": str(Path(temporary) / "hw"), "WINLUX_KERNEL_RELEASE": RELEASE, "WINLUX_KERNEL_LOG": "/dev/null",
                   "PATH": "/nonexistent"}).stdout.split()
    ok(out == acer["recommend"], "--items : un élément winlux-drivers par ligne (pour l'installateur)")

print("Rotation automatique (winlux-rotation)")
rotation = runpy.run_path(str(SCRIPTS / "winlux-rotation.py"), run_name="winlux_rotation")
orient = rotation["orientation"]
ok(orient(0, -9.8, 0) == "normal" and orient(0, 9.8, 0) == "bottom-up", "paysage droit et retourné")
ok(orient(9.8, 0, 0) == "left-up" and orient(-9.8, 0, 0) == "right-up", "portrait gauche et droit")
ok(orient(0.5, -0.5, 9.8, "left-up") == "left-up", "posée à plat : orientation gardée")
ok(orient(6, -6, 0, "normal") == "normal", "en diagonale : orientation gardée")
ok(rotation["parse_matrix"]("0, 1, 0; -1, 0, 0; 0, 0, 1") == [[0, 1, 0], [-1, 0, 0], [0, 0, 1]], "matrice de montage lue")
with tempfile.TemporaryDirectory() as temporary:
    base = Path(temporary)
    hw = base / "hw"
    accel = hw / "sys/bus/iio/devices/iio:device0"
    for axis, value in zip("xyz", ("0", "900", "40")):
        write(accel / f"in_accel_{axis}_raw", value)
    write(accel / "name", "accel_3d")
    write(hw / "sys/class/dmi/id/chassis_type", "32")
    bindir = base / "bin"
    calls = base / "calls.log"
    write(bindir / "xrandr", f"""#!/bin/sh
echo "xrandr $*" >>{calls}
case "$1" in --query) echo 'Screen 0: minimum 8 x 8, current 800 x 1280'; echo 'eDP-1 connected primary 800x1280+0+0 (normal left inverted right) 135mm x 216mm'; echo 'HDMI-1 disconnected (normal left inverted right)';; esac
""")
    write(bindir / "xinput", f"""#!/bin/sh
echo "xinput $*" >>{calls}
case "$*" in "list --name-only") printf 'Virtual core pointer\nGoodix Capacitive TouchScreen\nSYNA3602:00 Touchpad\nAT Translated Set 2 keyboard\n' ;; esac
""")
    for tool in ("xrandr", "xinput"):
        (bindir / tool).chmod(0o755)
    renv = {"WINLUX_HW_ROOT": str(hw), "WINLUX_ROTATION_ONCE": "1", "XDG_CONFIG_HOME": str(base / "config"),
            "PATH": f"{bindir}:/usr/bin:/bin"}
    result = run(SCRIPTS / "winlux-rotation.py", "watch", env=renv)
    log = calls.read_text() if calls.exists() else ""
    ok(result.returncode == 0 and "xrandr --output eDP-1 --rotate inverted" in log, "tablette retournée : écran interne inversé")
    ok("map-to-output Goodix Capacitive TouchScreen eDP-1" in log and "Touchpad eDP-1" not in log,
       "écran tactile recalé sur l'écran tourné, pas le pavé tactile")
    calls.write_text("")
    write(accel / "in_accel_mount_matrix", "0, 1, 0; -1, 0, 0; 0, 0, 1")
    run(SCRIPTS / "winlux-rotation.py", "watch", env=renv)
    ok("--rotate left" in calls.read_text(), "matrice de montage appliquée (capteur monté de travers)")
    calls.write_text("")
    run(SCRIPTS / "winlux-rotation.py", "lock", env=renv)
    run(SCRIPTS / "winlux-rotation.py", "watch", env=renv)
    ok("--rotate" not in calls.read_text(), "rotation verrouillée : écran inchangé")
    run(SCRIPTS / "winlux-rotation.py", "unlock", env=renv)
    write(hw / "sys/class/dmi/id/chassis_type", "10")
    result = run(SCRIPTS / "winlux-rotation.py", "watch", env=dict(renv, WINLUX_ROTATION_ONCE="0"))
    ok(result.returncode == 0 and "--rotate" not in calls.read_text(), "portable classique : le service s'arrête aussitôt")
    (accel / "in_accel_x_raw").unlink()
    ok(run(SCRIPTS / "winlux-rotation.py", "watch", env=dict(renv, WINLUX_ROTATION_ONCE="0")).returncode == 0,
       "sans accéléromètre : le service s'arrête aussitôt")
session = (SCRIPTS / "winlux-user-session.sh").read_text()
ok("start rotation.log /usr/bin/winlux-rotation watch" in session, "session : rotation suivie au démarrage")

print("Listes réelles et intégration")
dirs_list = (SCRIPTS / "hardware-module-dirs.list").read_text()
essentials_list = (SCRIPTS / "hardware-essentials.list").read_text()
build = (SCRIPTS / "build-live-iso.sh").read_text()
ok('stage-kernel-modules.py" "$stage" "$kernel_release" "$root/scripts/hardware-module-dirs.list"' in build
   and build.index("stage-kernel-modules.py") < build.index('depmod -b "$stage"'), "build-live-iso.sh embarque les dossiers avant depmod")
ok(build.index('python3 "$root/scripts/check-image-hardware.py" "$stage"') > build.index('python3 "$root/scripts/stage-apt-root.py"'), "contrôle sur la racine finale (après stage-apt-root.py)")
ok('winlux-hardware-check.py" "$stage/usr/bin/winlux-hardware-check"' in build, "winlux-hardware-check dans l'image")
ok('winlux-rotation.py" "$stage/usr/bin/winlux-rotation"' in build, "winlux-rotation dans l'image")
for line in ("drivers/net/wireless", "net/mac80211", "net/netfilter"):
    ok(f"!{line}" in dirs_list or line not in dirs_list, f"{line} laissé au Wi-Fi / pare-feu (#398)")
for name in ("pinctrl_sunrisepoint", "asix", "uhid", "binfmt_misc", "raid1", "wacom", "hid_sensor_accel_3d", "typec_displayport", "cdc_acm", "hv_vmbus"):
    ok(f"module {name}\n" in essentials_list, f"essentiel : {name}")
drivers_tool = (SCRIPTS / "winlux-drivers.py").read_text()
ok('"rgb")' in drivers_tool and 'simple(["openrgb"]' in drivers_tool, "winlux-drivers install rgb (OpenRGB), aussi au premier démarrage")
check_tool = (SCRIPTS / "winlux-hardware-check.py").read_text()
ok('RGB_VENDORS = ("1038"' in check_tool and drivers_tool.split("RGB_VENDORS = ", 1)[1].split(")", 1)[0] == check_tool.split("RGB_VENDORS = ", 1)[1].split(")", 1)[0],
   "même liste de fabricants RGB dans winlux-drivers et winlux-hardware-check")
ok(drivers_tool.split("FINGERPRINT_VENDORS = ", 1)[1].split(")", 1)[0] == check_tool.split("FINGERPRINT_VENDORS = ", 1)[1].split(")", 1)[0],
   "même liste de lecteurs d'empreintes")
base_packages = runpy.run_path(str(SCRIPTS / "prepare-apt-base.py"), run_name="apt_base")["PACKAGES"]
ok("intel-microcode" in base_packages and "amd64-microcode" in base_packages, "microcode Intel et AMD dans l'image (hors ligne)")
page = (SCRIPTS / "winlux-hardware-page.py").read_text()
ok("def run_check" in page and '"Lancer le diagnostic": (' in page, "Paramètres > Matériel et pilotes : bouton Lancer le diagnostic (4 langues)")
ok((ROOT / "docs/MATERIEL.md").is_file() and "hardware-essentials.list" in (ROOT / "docs/MATERIEL.md").read_text(), "docs/MATERIEL.md")

host_modules = os.environ.get("WINLUX_HOST_MODULES")
candidates = [Path(host_modules) / r for r in os.listdir(host_modules)] if host_modules and os.path.isdir(host_modules) else \
    [Path(p) for p in sorted(glob.glob("/lib/modules/*-generic"))]
kernel = next((p for p in candidates if (p / "modules.dep").is_file()), None)
if kernel is None:
    print("  —      pas de noyau Ubuntu complet sur cette machine : couverture réelle non vérifiée")
else:
    stager = runpy.run_path(str(SCRIPTS / "stage-kernel-modules.py"), run_name="stager")
    include, exclude = stager["read_list"](SCRIPTS / "hardware-module-dirs.list")
    chosen = set(stager["select"](kernel, include, exclude))
    # Les autres mécanismes de build-live-iso.sh : son entier, Wi-Fi (#398),
    # liste nommée hardware-modules.list (+ dépendances), outils de stockage.
    deps = stager["dependencies"](kernel)
    named = set(" ".join(l.split("#", 1)[0] for l in (SCRIPTS / "hardware-modules.list").read_text().splitlines()).split())
    named |= {"loop", "nbd", "isofs", "udf", "hfsplus", "ntfs3", "exfat", "squashfs", "virtio_net", "e1000", "e1000e", "r8169",
              "virtio_input", "hid_generic", "usbhid", "psmouse", "snd_hda_intel", "vfat", "usb_storage", "uas", "cifs", "nfs"}
    norm = lambda path: os.path.basename(stager["bare"](path)).replace("-", "_")
    by_name = {norm(m): m for m in deps}
    for name in list(named):
        module = by_name.get(name.replace("-", "_"))
        if module:
            chosen.add(module)
            chosen.update(deps[module])
    chosen |= {m for m in deps if m.startswith(("kernel/sound/", "kernel/drivers/soundwire/"))}
    shipped = {norm(m) for m in chosen}
    builtin = {norm(l) for l in (kernel / "modules.builtin").read_text().split()}
    missing = []
    for line in essentials_list.splitlines():
        kind, _, value = line.split("#", 1)[0].strip().partition(" ")
        if kind == "module":
            name = value.replace("-", "_")
            if name in by_name and name not in shipped and name not in builtin:
                missing.append(name)
    ok(not missing, f"noyau {kernel.name} : chaque pilote essentiel est embarqué ({', '.join(missing) or 'aucun oubli'})")

print(f"Matériel dans l'image : {checks} vérifications.")
