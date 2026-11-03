#!/usr/bin/env python3
"""Diagnostic du matériel : ce que Winlux ne prend pas (encore) en charge.

    winlux-hardware-check [--json] [--save FICHIER] [--all]
    winlux-hardware-check --items     éléments à installer, un par ligne

« --items » et la clé « recommend » du JSON nomment des éléments de
winlux-drivers (« winlux-drivers install ÉLÉMENT ») : l'installateur et le
premier démarrage les installent dès que le réseau est là. La clé « profile »
décrit la machine (fixe, portable, convertible, tablette, VM ; graphique
hybride, tactile, stylet, capteurs, batterie).

Parcourt les appareils PCI, USB, I2C et SPI de la machine, dit lesquels n'ont
aucun pilote et pourquoi (pilote absent du noyau, présent mais pas chargé,
firmware manquant), puis vérifie ce qu'un utilisateur remarque : écran
accéléré, son, pavé tactile, batterie, luminosité, Wi-Fi, Bluetooth, capteurs
de rotation, Thunderbolt, webcam. Lancé en administrateur (sudo), il lit aussi
le journal du noyau pour les firmwares manquants. Rien n'est modifié.
Politique de support : docs/MATERIEL.md.

Variables de test : WINLUX_HW_ROOT (racine qui contient sys/, proc/, lib/),
WINLUX_KERNEL_LOG (fichier lu à la place de dmesg), WINLUX_LANG.
"""
import argparse
import fnmatch
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

ROOT = Path(os.environ.get("WINLUX_HW_ROOT", "/"))

# ── Langue (fr, en, es, de : les quatre langues de Winlux) ─────────────────

def _lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in (ROOT / "etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip().strip('"')
        except OSError:
            pass
    code = (code or os.environ.get("LC_ALL") or os.environ.get("LC_MESSAGES") or os.environ.get("LANG") or "fr")[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "en"


LANG = _lang()
COL = {"fr": 0, "en": 1, "es": 2, "de": 3}
T = {
    "title": ("Diagnostic du matériel Winlux", "Winlux hardware check", "Diagnóstico de hardware de Winlux", "Winlux-Hardwarediagnose"),
    "machine": ("Machine", "Machine", "Equipo", "Gerät"),
    "kernel": ("Noyau", "Kernel", "Núcleo", "Kernel"),
    "devices": ("Appareils sans pilote", "Devices without a driver", "Dispositivos sin controlador", "Geräte ohne Treiber"),
    "none_missing": ("Tous les appareils ont un pilote.", "Every device has a driver.", "Todos los dispositivos tienen controlador.", "Alle Geräte haben einen Treiber."),
    "not_loaded": ("pilote {0} présent mais pas chargé (redémarrez ; s'il reste inactif, signalez-le)",
                   "driver {0} is present but not loaded (restart; if it stays inactive, report it)",
                   "el controlador {0} está presente pero no cargado (reinicie; si sigue inactivo, infórmelo)",
                   "Treiber {0} vorhanden, aber nicht geladen (neu starten; bleibt er inaktiv, bitte melden)"),
    "no_driver_full": ("aucun pilote dans Winlux : Paramètres > Matériel et pilotes > « Installer les pilotes complets » peut en apporter un",
                       "no driver in Winlux: Settings > Hardware and drivers > \"Install complete drivers\" may bring one",
                       "ningún controlador en Winlux: Configuración > Hardware y controladores > «Instalar controladores completos» puede traer uno",
                       "kein Treiber in Winlux: Einstellungen > Hardware und Treiber > „Vollständige Treiber installieren“ kann einen bringen"),
    "no_driver": ("aucun pilote dans le noyau Linux : non pris en charge", "no driver in the Linux kernel: not supported",
                  "ningún controlador en el núcleo Linux: no compatible", "kein Treiber im Linux-Kernel: nicht unterstützt"),
    "firmware": ("Firmwares manquants", "Missing firmware", "Firmware que falta", "Fehlende Firmware"),
    "firmware_none": ("Aucun firmware manquant dans le journal du noyau.", "No missing firmware in the kernel log.",
                      "No falta ningún firmware en el registro del núcleo.", "Keine fehlende Firmware im Kernel-Protokoll."),
    "firmware_unread": ("Journal du noyau illisible sans droits : relancez avec « sudo winlux-hardware-check » pour les firmwares.",
                        "Kernel log unreadable without rights: run \"sudo winlux-hardware-check\" to check firmware.",
                        "Registro del núcleo ilegible sin permisos: ejecute «sudo winlux-hardware-check» para el firmware.",
                        "Kernel-Protokoll ohne Rechte nicht lesbar: „sudo winlux-hardware-check“ prüft die Firmware."),
    "checks": ("Fonctions", "Features", "Funciones", "Funktionen"),
    "gpu_ok": ("Affichage accéléré : {0}", "Accelerated display: {0}", "Pantalla acelerada: {0}", "Beschleunigte Anzeige: {0}"),
    "gpu_basic": ("Affichage sans accélération (pilote générique {0}) : vidéos et jeux lents", "Display without acceleration (generic driver {0}): slow video and games",
                  "Pantalla sin aceleración (controlador genérico {0}): vídeo y juegos lentos", "Anzeige ohne Beschleunigung (generischer Treiber {0}): Videos und Spiele langsam"),
    "sound_ok": ("Son : {0}", "Sound: {0}", "Sonido: {0}", "Ton: {0}"),
    "sound_none": ("Aucune carte son : pas de haut-parleurs ni de micro", "No sound card: no speakers or microphone",
                   "Ninguna tarjeta de sonido: sin altavoces ni micrófono", "Keine Soundkarte: keine Lautsprecher, kein Mikrofon"),
    "touchpad_ok": ("Pavé tactile : {0}", "Touchpad: {0}", "Panel táctil: {0}", "Touchpad: {0}"),
    "touchpad_none": ("Aucun pavé tactile détecté sur ce portable", "No touchpad found on this laptop",
                      "No se detecta panel táctil en este portátil", "Kein Touchpad auf diesem Laptop erkannt"),
    "touch_ok": ("Écran tactile ou stylet : {0}", "Touchscreen or pen: {0}", "Pantalla táctil o lápiz: {0}", "Touchscreen oder Stift: {0}"),
    "battery_ok": ("Batterie : {0} %", "Battery: {0}%", "Batería: {0} %", "Akku: {0} %"),
    "battery_none": ("Batterie non détectée : niveau de charge invisible", "Battery not detected: no charge level",
                     "Batería no detectada: nivel de carga invisible", "Akku nicht erkannt: kein Ladestand"),
    "backlight_ok": ("Luminosité de l'écran : {0}", "Screen brightness: {0}", "Brillo de la pantalla: {0}", "Bildschirmhelligkeit: {0}"),
    "backlight_none": ("Luminosité de l'écran non réglable", "Screen brightness cannot be changed",
                       "No se puede ajustar el brillo de la pantalla", "Bildschirmhelligkeit nicht regelbar"),
    "wifi_ok": ("Wi-Fi : {0}", "Wi-Fi: {0}", "Wi-Fi: {0}", "WLAN: {0}"),
    "wifi_blocked": ("Wi-Fi coupé par l'interrupteur ou la touche du portable", "Wi-Fi switched off by the laptop switch or key",
                     "Wi-Fi desactivado por el interruptor o la tecla del portátil", "WLAN durch Schalter oder Taste des Laptops aus"),
    "ethernet_ok": ("Ethernet : {0}", "Ethernet: {0}", "Ethernet: {0}", "Ethernet: {0}"),
    "bt_ok": ("Bluetooth : {0}", "Bluetooth: {0}", "Bluetooth: {0}", "Bluetooth: {0}"),
    "sensor_ok": ("Capteur de rotation : {0}", "Rotation sensor: {0}", "Sensor de rotación: {0}", "Lagesensor: {0}"),
    "sensor_none": ("Aucun capteur de rotation sur cette tablette : l'écran ne pivote pas seul", "No rotation sensor on this tablet: the screen will not rotate by itself",
                    "Ningún sensor de rotación en esta tableta: la pantalla no gira sola", "Kein Lagesensor auf diesem Tablet: der Bildschirm dreht sich nicht selbst"),
    "tb_unauth": ("Périphérique Thunderbolt non autorisé : {0}", "Thunderbolt device not authorized: {0}",
                  "Dispositivo Thunderbolt no autorizado: {0}", "Thunderbolt-Gerät nicht autorisiert: {0}"),
    "camera_ok": ("Webcam : {0}", "Webcam: {0}", "Cámara web: {0}", "Webcam: {0}"),
    "camera_mipi": ("Webcam MIPI Intel (IPU) : pas encore prise en charge par Winlux", "Intel MIPI webcam (IPU): not supported by Winlux yet",
                    "Cámara MIPI de Intel (IPU): aún no compatible con Winlux", "Intel-MIPI-Webcam (IPU): von Winlux noch nicht unterstützt"),
    "disks": ("Disques : {0}", "Disks: {0}", "Discos: {0}", "Datenträger: {0}"),
    "raid": ("Contrôleur en mode RAID (Intel RST) : {0}", "Controller in RAID mode (Intel RST): {0}",
             "Controlador en modo RAID (Intel RST): {0}", "Controller im RAID-Modus (Intel RST): {0}"),
    "summary_ok": ("Rien de bloquant n'a été trouvé.", "Nothing blocking was found.", "No se encontró nada bloqueante.", "Nichts Blockierendes gefunden."),
    "summary_bad": ("{0} problème(s) : joignez ce rapport à un signalement (Paramètres > Signaler un problème).",
                    "{0} problem(s): attach this report to a bug report (Settings > Report a problem).",
                    "{0} problema(s): adjunte este informe a un aviso (Configuración > Informar de un problema).",
                    "{0} Problem(e): diesen Bericht einer Meldung anhängen (Einstellungen > Problem melden)."),
    "profile": ("Type de machine", "Device type", "Tipo de equipo", "Gerätetyp"),
    "kind_desktop": ("PC fixe", "Desktop PC", "PC de sobremesa", "Desktop-PC"),
    "kind_laptop": ("Portable", "Laptop", "Portátil", "Laptop"),
    "kind_convertible": ("Portable convertible", "Convertible laptop", "Portátil convertible", "Convertible"),
    "kind_tablet": ("Tablette", "Tablet", "Tableta", "Tablet"),
    "kind_vm": ("Machine virtuelle", "Virtual machine", "Máquina virtual", "Virtuelle Maschine"),
    "hybrid": ("graphique hybride", "hybrid graphics", "gráficos híbridos", "Hybridgrafik"),
    "recommend": ("À installer pour cette machine", "To install for this device", "Para instalar en este equipo", "Für dieses Gerät zu installieren"),
    "recommend_none": ("Rien à ajouter.", "Nothing to add.", "Nada que añadir.", "Nichts hinzuzufügen."),
    "item_modules": ("Pilotes complets du noyau (un appareil n'a pas de pilote dans l'image)", "Complete kernel drivers (a device has no driver in the image)",
                     "Controladores completos del núcleo (un dispositivo no tiene controlador en la imagen)", "Vollständige Kernel-Treiber (ein Gerät hat keinen Treiber im Abbild)"),
    "item_nvidia": ("Pilote NVIDIA propriétaire (jeux, CUDA, NVENC)", "Proprietary NVIDIA driver (games, CUDA, NVENC)",
                    "Controlador propietario de NVIDIA (juegos, CUDA, NVENC)", "Proprietärer NVIDIA-Treiber (Spiele, CUDA, NVENC)"),
    "item_vaapi": ("Vidéo matérielle (VA-API)", "Hardware video (VA-API)", "Vídeo por hardware (VA-API)", "Hardware-Video (VA-API)"),
    "item_microcode": ("Microcode du processeur", "Processor microcode", "Microcódigo del procesador", "Prozessor-Microcode"),
    "item_fwupd": ("Mises à jour du micrologiciel (fwupd)", "Firmware updates (fwupd)", "Actualizaciones de firmware (fwupd)", "Firmware-Aktualisierungen (fwupd)"),
    "item_fingerprint": ("Lecteur d'empreintes digitales", "Fingerprint reader", "Lector de huellas", "Fingerabdruckleser"),
    "item_rgb": ("Éclairage RGB (OpenRGB)", "RGB lighting (OpenRGB)", "Iluminación RGB (OpenRGB)", "RGB-Beleuchtung (OpenRGB)"),
    "item_printer-drivers": ("Pilotes d'imprimantes (imprimante USB branchée)", "Printer drivers (USB printer plugged in)",
                             "Controladores de impresora (impresora USB conectada)", "Druckertreiber (USB-Drucker angeschlossen)"),
    "saved": ("Rapport enregistré : {0}", "Report saved: {0}", "Informe guardado: {0}", "Bericht gespeichert: {0}"),
}


def t(key, *args):
    return T[key][COL[LANG]].format(*args)


# ── Lecture de /sys ─────────────────────────────────────────────────────────

def read(path, default=""):
    try:
        return Path(path).read_text(errors="replace").strip()
    except OSError:
        return default


def driver_of(device):
    link = Path(device) / "driver"
    try:
        return os.path.basename(os.readlink(link))
    except OSError:
        return ""


def run(args):
    try:
        return subprocess.run(args, capture_output=True, text=True, timeout=20).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def kernel_release():
    return os.environ.get("WINLUX_KERNEL_RELEASE") or platform.release()


def modules_dir():
    return ROOT / "lib/modules" / kernel_release()


def load_aliases():
    """[(motif, module)] de modules.alias ; correspondance façon modprobe."""
    aliases = []
    for line in read(modules_dir() / "modules.alias").splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[0] == "alias":
            aliases.append((parts[1], parts[2]))
    return aliases


def module_for(modalias, aliases):
    if not modalias:
        return ""
    for pattern, module in aliases:
        if fnmatch.fnmatchcase(modalias, pattern):
            return module
    return ""


def full_modules_installed():
    status = read(ROOT / "var/lib/dpkg/status")
    release = re.escape(kernel_release())
    return bool(re.search(rf"^Package: linux-modules(-extra)?-{release}\nStatus: install ok installed", status, re.M))


# PCI : classes dont un appareil sans pilote se remarque (stockage, réseau,
# affichage, son, communication, entrée, USB/Thunderbolt/I2C/SMBus, sans fil,
# traitement du signal : capteurs et contrôleurs I2C des pavés tactiles).
PCI_IMPORTANT = ("01", "02", "03", "04", "07", "09", "0c", "0d", "11")
PCI_IGNORED = ("0c05",)  # SMBus : souvent sans pilote utile sur les PC fixes


def pci_names():
    names = {}
    for line in run(["lspci", "-mm", "-D"]).splitlines():
        fields = re.findall(r'"([^"]*)"', line)
        slot = line.split(" ", 1)[0]
        if len(fields) >= 3:
            names[slot] = f"{fields[1]} {fields[2]}".strip()
    return names


def pci_devices():
    devices = []
    base = ROOT / "sys/bus/pci/devices"
    if not base.is_dir():
        return devices
    names = pci_names()
    for device in sorted(base.iterdir()):
        cls = read(device / "class").removeprefix("0x")
        vendor = read(device / "vendor").removeprefix("0x")
        product = read(device / "device").removeprefix("0x")
        devices.append({"bus": "pci", "slot": device.name, "class": cls, "id": f"{vendor}:{product}",
                        "name": names.get(device.name, ""), "driver": driver_of(device),
                        "modalias": read(device / "modalias")})
    return devices


def usb_names():
    names = {}
    for line in run(["lsusb"]).splitlines():
        m = re.match(r"Bus (\d+) Device (\d+): ID ([0-9a-f]{4}:[0-9a-f]{4}) ?(.*)", line)
        if m:
            names[(int(m.group(1)), int(m.group(2)))] = m.group(4).strip()
    return names


def usb_devices():
    devices = []
    base = ROOT / "sys/bus/usb/devices"
    if not base.is_dir():
        return devices
    names = usb_names()
    for device in sorted(base.iterdir()):
        if ":" in device.name or not (device / "idVendor").exists():
            continue
        interfaces = [i for i in sorted(base.iterdir()) if i.name.startswith(device.name + ":")]
        if not interfaces or read(device / "bDeviceClass") == "09":
            continue  # concentrateurs, appareils sans interface
        drivers = [driver_of(i) for i in interfaces]
        unbound = [i for i, d in zip(interfaces, drivers) if not d]
        ident = f"{read(device / 'idVendor')}:{read(device / 'idProduct')}"
        try:
            key = (int(read(device / "busnum", "0")), int(read(device / "devnum", "0")))
        except ValueError:
            key = (0, 0)
        name = names.get(key) or " ".join(filter(None, (read(device / "manufacturer"), read(device / "product"))))
        devices.append({"bus": "usb", "slot": device.name, "class": read(interfaces[0] / "bInterfaceClass"), "id": ident,
                        "name": name, "driver": ",".join(sorted(set(filter(None, drivers)))),
                        "modalias": read(unbound[0] / "modalias") if unbound else "",
                        "partial": bool(unbound) and len(unbound) < len(interfaces)})
    return devices


def acpi_bus_devices(bus):
    """Clients I2C/SPI décrits par l'ACPI (pavés et écrans tactiles, capteurs)."""
    devices = []
    base = ROOT / "sys/bus" / bus / "devices"
    if not base.is_dir():
        return devices
    for device in sorted(base.iterdir()):
        if not re.match(rf"^{bus}-[A-Z0-9]{{4,8}}:\d+$", device.name):
            continue
        devices.append({"bus": bus, "slot": device.name, "class": "", "id": device.name.split("-", 1)[1].split(":")[0],
                        "name": read(device / "name"), "driver": driver_of(device), "modalias": read(device / "modalias")})
    return devices


def unsupported(devices, aliases):
    full = full_modules_installed()
    problems = []
    for dev in devices:
        if dev["driver"] and not dev.get("partial"):
            continue
        if dev["bus"] == "pci" and (not dev["class"][:2] in PCI_IMPORTANT or dev["class"][:4] in PCI_IGNORED):
            continue
        if dev.get("partial"):
            continue  # une interface secondaire sans pilote (ex. DFU) ne gêne pas
        module = module_for(dev["modalias"], aliases)
        if module:
            reason = t("not_loaded", module)
        else:
            reason = t("no_driver") if full else t("no_driver_full")
        problems.append(dict(dev, module=module, reason=reason))
    return problems


# ── Firmwares (journal du noyau) ───────────────────────────────────────────

def kernel_log():
    path = os.environ.get("WINLUX_KERNEL_LOG")
    if path:
        return read(path), True
    result = None
    try:
        result = subprocess.run(["dmesg"], capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.SubprocessError):
        pass
    if result is None or result.returncode != 0 or not result.stdout:
        return "", False
    return result.stdout, True


def missing_firmware(log):
    found = []
    for pattern in (r"Direct firmware load for (\S+) failed", r"firmware: failed to load (\S+)",
                    r"failed to load firmware (\S+)", r"Failed to load (?:firmware )?\"?([\w./-]+\.(?:bin|fw|ucode|sbin|ri|pnvm))"):
        for name in re.findall(pattern, log):
            name = name.rstrip(",.:\"")
            if name not in found:
                found.append(name)
    return found


# ── Fonctions remarquées par l'utilisateur ─────────────────────────────────

LAPTOP_CHASSIS = {"8", "9", "10", "11", "14", "30", "31", "32"}
TABLET_CHASSIS = {"30", "31", "32"}


def input_devices():
    blocks = read(ROOT / "proc/bus/input/devices").split("\n\n")
    devices = []
    for block in blocks:
        name = re.search(r'^N: Name="(.*)"', block, re.M)
        if name:
            devices.append((name.group(1), block))
    return devices


def checks(pci):
    chassis = read(ROOT / "sys/class/dmi/id/chassis_type")
    laptop = chassis in LAPTOP_CHASSIS
    tablet = chassis in TABLET_CHASSIS
    out = []  # (ok, texte)

    gpus = [d for d in pci if d["class"].startswith("03")]
    accel = [d for d in gpus if d["driver"] in ("i915", "xe", "amdgpu", "radeon", "nouveau", "nvidia", "gma500_gfx",
                                               "virtio-pci", "qxl", "vmwgfx", "bochs-drm", "bochs", "hyperv_drm", "vboxvideo")]
    if accel:
        out.append((True, t("gpu_ok", ", ".join(f"{d['name'] or d['id']} ({d['driver']})" for d in accel))))
    elif gpus or (ROOT / "sys/class/drm").is_dir():
        generic = next((driver_of(c / "device") for c in sorted((ROOT / "sys/class/drm").glob("card[0-9]"))), "") or "simpledrm"
        out.append((False, t("gpu_basic", generic)))

    cards = [line.split("]:", 1)[1].strip() for line in read(ROOT / "proc/asound/cards").splitlines() if "]:" in line]
    cards = [c for c in cards if "Dummy" not in c and "Loopback" not in c]
    out.append((True, t("sound_ok", "; ".join(cards))) if cards else (False, t("sound_none")))

    inputs = input_devices()
    pads = [n for n, b in inputs if re.search(r"touchpad|trackpad|clickpad|glidepoint", n, re.I)]
    touch = [n for n, b in inputs if re.search(r"touchscreen|wacom|pen|stylus|digitizer", n, re.I) and n not in pads]
    if pads:
        out.append((True, t("touchpad_ok", ", ".join(pads))))
    elif laptop and not tablet:
        out.append((False, t("touchpad_none")))
    if touch:
        out.append((True, t("touch_ok", ", ".join(touch))))

    supplies = ROOT / "sys/class/power_supply"
    batteries = [p for p in sorted(supplies.iterdir()) if read(p / "type") == "Battery" and read(p / "scope") != "Device"] if supplies.is_dir() else []
    if batteries:
        out.append((True, t("battery_ok", read(batteries[0] / "capacity", "?"))))
    elif laptop:
        out.append((False, t("battery_none")))

    lights = sorted(p.name for p in (ROOT / "sys/class/backlight").iterdir()) if (ROOT / "sys/class/backlight").is_dir() else []
    if lights:
        out.append((True, t("backlight_ok", ", ".join(lights))))
    elif laptop:
        out.append((False, t("backlight_none")))

    net = ROOT / "sys/class/net"
    if net.is_dir():
        wireless = [n.name for n in sorted(net.iterdir()) if (n / "wireless").exists() or (n / "phy80211").exists()]
        wired = [n.name for n in sorted(net.iterdir()) if (n / "device").exists() and n.name not in wireless]
        if wireless:
            out.append((True, t("wifi_ok", ", ".join(wireless))))
        if wired:
            out.append((True, t("ethernet_ok", ", ".join(wired))))
    rfkill = ROOT / "sys/class/rfkill"
    if rfkill.is_dir() and any(read(r / "type") == "wlan" and read(r / "hard") == "1" for r in rfkill.iterdir()):
        out.append((False, t("wifi_blocked")))
    bt = sorted(p.name for p in (ROOT / "sys/class/bluetooth").iterdir() if ":" not in p.name) if (ROOT / "sys/class/bluetooth").is_dir() else []
    if bt:
        out.append((True, t("bt_ok", ", ".join(bt))))

    iio = ROOT / "sys/bus/iio/devices"
    accels = [read(d / "name") or d.name for d in sorted(iio.iterdir()) if any(d.glob("in_accel_*"))] if iio.is_dir() else []
    if accels:
        out.append((True, t("sensor_ok", ", ".join(accels))))
    elif tablet:
        out.append((False, t("sensor_none")))

    tb = ROOT / "sys/bus/thunderbolt/devices"
    if tb.is_dir():
        for device in sorted(tb.iterdir()):
            if read(device / "authorized") == "0":
                out.append((False, t("tb_unauth", read(device / "device_name") or device.name)))

    v4l = ROOT / "sys/class/video4linux"
    cams = sorted({read(v / "name") for v in v4l.iterdir() if read(v / "index", "0") == "0"}) if v4l.is_dir() else []
    if cams:
        out.append((True, t("camera_ok", ", ".join(c for c in cams if c))))
    if any(d["id"].startswith("8086:") and d["class"].startswith("0480") and not d["driver"] for d in pci):
        out.append((False, t("camera_mipi")))

    block = ROOT / "sys/block"
    disks = [b.name for b in sorted(block.iterdir()) if not re.match(r"(loop|ram|zram|sr|dm-|nbd)", b.name)] if block.is_dir() else []
    if disks:
        out.append((True, t("disks", ", ".join(disks))))
    for d in pci:
        if d["class"].startswith("0104"):
            out.append((bool(d["driver"]), t("raid", d["driver"] or "—")))
    return out


# ── Profil de la machine et éléments recommandés ───────────────────────────

VM_VENDORS = ("QEMU", "VMware", "innotek", "VirtualBox", "Microsoft Corporation", "Xen", "Parallels", "Bochs")
# Mêmes listes que winlux-drivers (lecteurs d'empreintes, éclairage RGB).
FINGERPRINT_VENDORS = ("138a", "06cb", "04f3", "27c6", "2808", "08ff", "147e", "1c7a")
RGB_VENDORS = ("1038", "1462", "0b05", "1b1c", "1532", "2516", "048d")
VAAPI_FILES = {"8086": "iHD_drv_video.so", "1002": "radeonsi_drv_video.so", "10de": "nvidia_drv_video.so"}


def installed(package):
    status = read(ROOT / "var/lib/dpkg/status")
    return bool(re.search(rf"^Package: {re.escape(package)}\nStatus: install ok installed", status, re.M))


def have_file(*candidates):
    return any((ROOT / c.lstrip("/")).exists() for c in candidates)


def profile(pci, usb):
    chassis = read(ROOT / "sys/class/dmi/id/chassis_type")
    vendor = read(ROOT / "sys/class/dmi/id/sys_vendor")
    product = read(ROOT / "sys/class/dmi/id/product_name")
    virtual = any(vendor.startswith(v) for v in VM_VENDORS) and ("Virtual" in product or vendor != "Microsoft Corporation")
    kind = ("vm" if virtual else "tablet" if chassis in ("30", "32") else "convertible" if chassis == "31"
            else "laptop" if chassis in LAPTOP_CHASSIS else "desktop")
    gpus = sorted({d["id"].split(":")[0] for d in pci if d["class"].startswith("03")})
    names = [n for n, _ in input_devices()]
    iio = ROOT / "sys/bus/iio/devices"
    supplies = ROOT / "sys/class/power_supply"
    return {"kind": kind, "chassis": chassis, "vendor": vendor, "model": product,
            "gpus": gpus, "hybrid": len(gpus) > 1,
            "nvidia": "10de" in gpus,
            "touchscreen": any(re.search(r"touchscreen|touch$", n, re.I) for n in names),
            "pen": any(re.search(r"pen|stylus|wacom", n, re.I) for n in names),
            "touchpad": any(re.search(r"touchpad|trackpad|clickpad", n, re.I) for n in names),
            "accelerometer": iio.is_dir() and any(any(d.glob("in_accel_*")) for d in iio.iterdir()),
            "battery": supplies.is_dir() and any(read(b / "type") == "Battery" and read(b / "scope") != "Device" for b in supplies.iterdir()),
            "usb_vendors": sorted({d["id"].split(":")[0] for d in usb})}


def cpu_vendor():
    text = read(ROOT / "proc/cpuinfo")
    return "intel" if "GenuineIntel" in text else "amd" if "AuthenticAMD" in text else ""


def recommend(prof, pci, usb, problems):
    """Éléments de winlux-drivers utiles à cette machine. En cas de doute, on
    ajoute (règle de damien) : un élément inutile ne coûte que de la place."""
    items = []
    if prof["kind"] == "vm":
        return items
    vendor = cpu_vendor()
    if vendor and not installed("intel-microcode" if vendor == "intel" else "amd64-microcode"):
        items.append("microcode")
    if prof["nvidia"] and not any(d["driver"] == "nvidia" for d in pci):
        items.append("nvidia")
    for gpu in prof["gpus"]:
        if gpu in VAAPI_FILES and not have_file(f"/usr/lib/x86_64-linux-gnu/dri/{VAAPI_FILES[gpu]}"):
            items.append("vaapi")
            break
    if any(not p["module"] for p in problems) and not full_modules_installed():
        items.append("modules")
    if not have_file("/usr/bin/fwupdtool", "/usr/libexec/fwupd/fwupdtool"):
        items.append("fwupd")
    vendors = set(prof["usb_vendors"])
    if vendors & set(FINGERPRINT_VENDORS) and not installed("fprintd"):
        items.append("fingerprint")
    if vendors & set(RGB_VENDORS) and not installed("openrgb"):
        items.append("rgb")
    if any(d["class"] == "07" for d in usb) and not installed("printer-driver-gutenprint"):
        items.append("printer-drivers")
    return items


# ── Rapport ─────────────────────────────────────────────────────────────────

def collect():
    aliases = load_aliases()
    pci = pci_devices()
    devices = pci + usb_devices() + acpi_bus_devices("i2c") + acpi_bus_devices("spi")
    log, readable = kernel_log()
    vendor = read(ROOT / "sys/class/dmi/id/sys_vendor")
    product = read(ROOT / "sys/class/dmi/id/product_name")
    usb = [d for d in devices if d["bus"] == "usb"]
    rows = checks(pci)
    problems = unsupported(devices, aliases)
    prof = profile(pci, usb)
    return {"machine": " ".join(filter(None, (vendor, product))) or "?",
            "profile": prof,
            "recommend": recommend(prof, pci, usb, problems),
            "chassis": read(ROOT / "sys/class/dmi/id/chassis_type"),
            "kernel": kernel_release(),
            "unsupported": problems,
            "firmware": missing_firmware(log) if readable else None,
            "checks": [{"ok": ok, "text": text} for ok, text in rows],
            "devices": devices}


def render(report, show_all=False):
    lines = [t("title"), "=" * len(t("title")),
             f"{t('machine')} : {report['machine']}" if LANG == "fr" else f"{t('machine')}: {report['machine']}",
             f"{t('kernel')} : {report['kernel']}" if LANG == "fr" else f"{t('kernel')}: {report['kernel']}"]
    prof = report["profile"]
    kind = t("kind_" + prof["kind"]) + (f" · {t('hybrid')}" if prof["hybrid"] else "")
    lines += [f"{t('profile')} : {kind}" if LANG == "fr" else f"{t('profile')}: {kind}", ""]
    lines.append(t("devices"))
    if report["unsupported"]:
        for dev in report["unsupported"]:
            label = dev["name"] or dev["id"]
            lines.append(f"  ✗ [{dev['bus'].upper()} {dev['id']}] {label}")
            lines.append(f"      {dev['reason']}")
    else:
        lines.append("  ✓ " + t("none_missing"))
    lines += ["", t("firmware")]
    if report["firmware"] is None:
        lines.append("  ? " + t("firmware_unread"))
    elif report["firmware"]:
        lines += [f"  ✗ {name}" for name in report["firmware"]]
    else:
        lines.append("  ✓ " + t("firmware_none"))
    lines += ["", t("checks")]
    lines += [f"  {'✓' if c['ok'] else '✗'} {c['text']}" for c in report["checks"]]
    lines += ["", t("recommend")]
    lines += [f"  + {t('item_' + item)}  (winlux-drivers install {item})" for item in report["recommend"]] or ["  ✓ " + t("recommend_none")]
    if show_all:
        lines.append("")
        for dev in report["devices"]:
            lines.append(f"  {dev['bus']:4} {dev['slot']:16} {dev['id']:10} {dev['driver'] or '—':16} {dev['name']}")
    problems = len(report["unsupported"]) + len(report["firmware"] or []) + sum(1 for c in report["checks"] if not c["ok"])
    lines += ["", t("summary_bad", problems) if problems else t("summary_ok")]
    return "\n".join(lines) + "\n", problems


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--json", action="store_true", help="rapport JSON")
    parser.add_argument("--save", metavar="FICHIER", help="enregistre aussi le rapport texte")
    parser.add_argument("--all", action="store_true", help="liste aussi tous les appareils")
    parser.add_argument("--items", action="store_true", help="éléments winlux-drivers à installer, un par ligne")
    args = parser.parse_args()
    report = collect()
    if args.items:
        print("\n".join(report["recommend"]))
        return 0
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=1))
        return 0
    text, problems = render(report, args.all)
    sys.stdout.write(text)
    if args.save:
        Path(args.save).write_text(text, encoding="utf-8")
        print(t("saved", args.save))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
