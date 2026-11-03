#!/usr/bin/env python3
"""Diagnostic du matériel : ce que Winlux ne prend pas (encore) en charge.

    winlux-hardware-check [--json] [--save FICHIER] [--all]

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


# ── Rapport ─────────────────────────────────────────────────────────────────

def collect():
    aliases = load_aliases()
    pci = pci_devices()
    devices = pci + usb_devices() + acpi_bus_devices("i2c") + acpi_bus_devices("spi")
    log, readable = kernel_log()
    vendor = read(ROOT / "sys/class/dmi/id/sys_vendor")
    product = read(ROOT / "sys/class/dmi/id/product_name")
    return {"machine": " ".join(filter(None, (vendor, product))) or "?",
            "chassis": read(ROOT / "sys/class/dmi/id/chassis_type"),
            "kernel": kernel_release(),
            "unsupported": unsupported(devices, aliases),
            "firmware": missing_firmware(log) if readable else None,
            "checks": [{"ok": ok, "text": text} for ok, text in checks(pci)],
            "devices": devices}


def render(report, show_all=False):
    lines = [t("title"), "=" * len(t("title")),
             f"{t('machine')} : {report['machine']}" if LANG == "fr" else f"{t('machine')}: {report['machine']}",
             f"{t('kernel')} : {report['kernel']}" if LANG == "fr" else f"{t('kernel')}: {report['kernel']}", ""]
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
    args = parser.parse_args()
    report = collect()
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
