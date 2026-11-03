#!/usr/bin/python3
"""Pilotes du matériel réel : ce qui est reconnu, ce qui manque, ce qui s'installe.

Comme Windows Update pour les pilotes : tout vient des dépôts Ubuntu signés,
après l'installation du système (jamais dans l'ISO, qui doit rester
amorçable en machine virtuelle et sous la limite de taille des versions).

    winlux-drivers status            état CLÉ=valeur (sysfs seulement, rapide)
    winlux-drivers refresh           relit les dépôts et note ce qui est proposé (root)
    winlux-drivers install ÉLÉMENT   installe un élément proposé (root)
    winlux-drivers remove nvidia     revient au pilote libre nouveau (root)
    winlux-drivers auto              premier démarrage : pilotes recommandés (root)
    winlux-drivers boot              à chaque démarrage : microcode et noyau à jour (root)
    winlux-drivers firmware-check    micrologiciels à mettre à jour (fwupd)
    winlux-drivers firmware-update   applique les mises à jour de micrologiciel (root)

Éléments : modules, nvidia, mesa, vaapi, microcode, amd-pstate, fwupd,
fingerprint, scanner, printer-drivers.

WINLUX_DRIVERS_ROOT déplace /sys, /proc, /etc, /boot, /usr et /var (tests) ;
WINLUX_KERNEL_RELEASE remplace uname -r.
"""
from fnmatch import fnmatchcase
from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(os.environ.get("WINLUX_DRIVERS_ROOT", "/"))
STATE = ROOT / "var/lib/winlux/drivers"
CANDIDATES = STATE / "candidates"
FIRMWARE = STATE / "firmware-updates"
CONFIG = ROOT / "etc/winlux/drivers.conf"
GRUB = ROOT / "boot/grub/grub.cfg"
NVIDIA_MODPROBE = ROOT / "etc/modprobe.d/winlux-nvidia.conf"
MICROCODE_CPIO = "/boot/winlux-microcode.cpio"
LIBDIRS = ("usr/lib/x86_64-linux-gnu", "usr/lib64", "usr/lib")
VENDORS = {"0x8086": "intel", "0x1002": "amd", "0x10de": "nvidia"}
# Classes PCI qui doivent avoir un pilote : stockage, réseau, affichage,
# multimédia (son, webcam), sans fil. Les ponts et contrôleurs système non.
CHECKED_CLASSES = ("0x01", "0x02", "0x03", "0x04", "0x0d")
# Lecteurs d'empreintes USB les plus courants (Synaptics, Validity, Elan,
# Goodix, FocalTech, AuthenTec, Upek).
FINGERPRINT_VENDORS = ("138a", "06cb", "04f3", "27c6", "2808", "08ff", "147e", "1c7a")
PRINTER_DRIVERS = ["printer-driver-gutenprint", "printer-driver-hpcups", "printer-driver-brlaser",
                   "printer-driver-splix"]
VAAPI = {"intel": ("iHD_drv_video.so", ["intel-media-va-driver-non-free"], ["intel-media-va-driver"]),
         "amd": ("radeonsi_drv_video.so", ["mesa-va-drivers"], []),
         "nvidia": ("nvidia_drv_video.so", ["nvidia-vaapi-driver"], [])}
ITEMS = ("modules", "nvidia", "mesa", "vaapi", "microcode", "amd-pstate", "fwupd", "fingerprint",
         "scanner", "printer-drivers")


# ── Lecture du système ─────────────────────────────────────────────────────

def path(relative):
    return ROOT / relative.lstrip("/")


def read(target, default=""):
    try:
        return Path(target).read_text(errors="replace").strip()
    except OSError:
        return default


def keyvals(text):
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def run(args, timeout=30):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout if result.returncode == 0 else None


def have(command):
    return shutil.which(command) is not None


def kernel_release():
    return os.environ.get("WINLUX_KERNEL_RELEASE") or os.uname().release


def installed(package):
    out = run(["dpkg-query", "-W", "-f=${Status}", package], timeout=10)
    return bool(out) and out.endswith("installed") and "not-installed" not in out


def installed_version(pattern):
    """Version du premier paquet installé dont le nom correspond au motif."""
    out = run(["dpkg-query", "-W", "-f=${Package} ${Status}\\n", pattern], timeout=10) or ""
    for line in out.splitlines():
        name, _, status = line.partition(" ")
        if status.endswith(" installed"):
            return name
    return ""


def config():
    values = {"AUTO": "1"}
    values.update(keyvals(read(CONFIG)))
    return values


def live():
    """Session live (clé USB, ISO) : seul un système installé garde des pilotes."""
    return not path("etc/winlux/install-config.json").is_file()


def virtual_machine():
    return read(path("run/winlux/hw/virtual-machine"))


def installable():
    """Installer des pilotes n'a de sens que sur un système installé, avec APT."""
    return not live() and have("apt-get") and have("dpkg-query")


def pci_devices():
    base = path("sys/bus/pci/devices")
    try:
        entries = sorted(base.iterdir())
    except OSError:
        return []
    devices = []
    for dev in entries:
        driver = dev / "driver"
        devices.append({
            "slot": dev.name, "class": read(dev / "class"), "vendor": read(dev / "vendor"),
            "device": read(dev / "device"), "modalias": read(dev / "modalias"),
            "driver": os.path.basename(os.readlink(driver)) if driver.is_symlink() else "",
            "boot_vga": read(dev / "boot_vga") == "1"})
    return devices


def virtual_display(vendor):
    return vendor in ("0x1234", "0x1af4", "0x1b36", "0x15ad", "0x80ee", "0x1414")


def gpus(devices=None):
    found = []
    for dev in devices if devices is not None else pci_devices():
        if not dev["class"].startswith("0x03") or virtual_display(dev["vendor"]):
            continue
        found.append(dict(dev, kind=VENDORS.get(dev["vendor"], "other")))
    return found


def gpu_name(dev):
    if have("lspci"):
        out = run(["lspci", "-s", dev["slot"]], timeout=5) or ""
        name = out.split(": ", 1)[-1].strip()
        if name:
            return re.sub(r"\s*\(rev [0-9a-f]+\)$", "", name)
    names = {"intel": "Intel", "amd": "AMD", "nvidia": "NVIDIA"}
    return f"{names.get(dev['kind'], 'GPU')} {dev['device']}"


def unbound(devices):
    return [d for d in devices if not d["driver"] and d["class"].startswith(CHECKED_CLASSES)
            and not virtual_display(d["vendor"])]


def lib_present(name):
    return any(path(f"{d}/{name}").exists() for d in LIBDIRS)


def vaapi_present(driver):
    return any(path(f"{d}/dri/{driver}").exists() for d in LIBDIRS)


def cpu_vendor():
    info = read(path("proc/cpuinfo"))
    if "GenuineIntel" in info:
        return "intel"
    if "AuthenticAMD" in info:
        return "amd"
    return ""


def cpu_field(name):
    match = re.search(rf"^{name}\s*:\s*(.+)$", read(path("proc/cpuinfo")), re.M)
    return match.group(1).strip() if match else ""


def amd_family():
    try:
        return int(cpu_field("cpu family"))
    except ValueError:
        return 0


def hwmon():
    """Températures et ventilateurs exposés par le noyau (coretemp, k10temp…)."""
    temps, fans = {}, []
    base = path("sys/class/hwmon")
    try:
        entries = sorted(base.iterdir())
    except OSError:
        entries = []
    for mon in entries:
        name = read(mon / "name")
        for sensor in sorted(mon.glob("temp*_input")):
            try:
                value = int(read(sensor, "0")) // 1000
            except ValueError:
                continue
            label = read(mon / sensor.name.replace("_input", "_label")) or name
            if name in ("coretemp", "k10temp", "zenpower") and value > 0:
                if label.startswith(("Package", "Tctl", "Tdie")) or "cpu" not in temps:
                    temps["cpu"] = value
            elif name in ("amdgpu", "nouveau", "i915", "xe") and value > 0:
                temps.setdefault("gpu", value)
            elif name == "nvme" and value > 0:
                temps.setdefault("disk", value)
        for fan in sorted(mon.glob("fan*_input")):
            try:
                rpm = int(read(fan, "0"))
            except ValueError:
                continue
            fans.append(rpm)
    return temps, fans


def usb_vendors():
    base = path("sys/bus/usb/devices")
    try:
        return {read(dev / "idVendor") for dev in base.iterdir()}
    except OSError:
        return set()


def candidates():
    return keyvals(read(CANDIDATES))


# ── Votre PC (Paramètres > À propos) : ce que le système a détecté ─────────

# Valeurs de remplissage des fabricants : pas un vrai nom.
DMI_PLACEHOLDERS = ("to be filled", "system product name", "system manufacturer", "default string",
                    "not applicable", "not specified", "o.e.m.", "oem", "none", "unknown", "123456789")
SECURE_BOOT_VAR = "sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c"


def dmi(name):
    value = " ".join(read(path(f"sys/class/dmi/id/{name}")).split())
    return "" if not value or value.lower().startswith(DMI_PLACEHOLDERS) or value.lower() in DMI_PLACEHOLDERS else value


def pc_cpu():
    info = read(path("proc/cpuinfo"))
    threads = len(re.findall(r"^processor\s*:", info, re.M))
    cores = set()
    for block in info.split("\n\n"):
        phys = re.search(r"^physical id\s*:\s*(\S+)", block, re.M)
        core = re.search(r"^core id\s*:\s*(\S+)", block, re.M)
        if core:
            cores.add((phys.group(1) if phys else "0", core.group(1)))
    name = cpu_field("model name") or cpu_field("Model") or cpu_field("Hardware")
    return " ".join(name.split()), len(cores) or threads, threads


def pc_disks():
    """Disques physiques (ni loop, ni RAM, ni lecteur optique) avec la place
    libre des partitions montées."""
    mounts = {}
    for line in read(path("proc/mounts")).splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[0].startswith("/dev/"):
            mounts.setdefault(parts[0][5:], parts[1].replace("\\040", " "))
    disks = []
    base = path("sys/block")
    try:
        entries = sorted(base.iterdir())
    except OSError:
        entries = []
    for dev in entries:
        name = dev.name
        if name.startswith(("loop", "ram", "zram", "sr", "fd", "dm-", "md", "nbd")):
            continue
        try:
            size = int(read(dev / "size", "0")) * 512
        except ValueError:
            size = 0
        if size < 64 * 1048576:  # disques factices et lecteurs vides
            continue
        removable = read(dev / "removable") == "1" or "/usb" in os.path.realpath(dev / "device")
        if name.startswith("nvme"):
            kind = "nvme"
        elif name.startswith("mmcblk"):
            kind = "sd" if read(dev / "device/type") == "SD" else "emmc"
        elif removable:
            kind = "usb"
        else:
            kind = "hdd" if read(dev / "queue/rotational") == "1" else "ssd"
        free = total = 0
        parts = [name] + sorted(p.name for p in dev.iterdir() if p.name.startswith(name) and p.name != name) if dev.is_dir() else [name]
        for part in parts:
            mount = mounts.get(part)
            if not mount:
                continue
            try:
                stat = os.statvfs(path(mount))
            except OSError:
                continue
            free += stat.f_bavail * stat.f_frsize
            total += stat.f_blocks * stat.f_frsize
        model = " ".join((read(dev / "device/model") or read(dev / "device/name")).split())
        disks.append({"name": name, "model": model, "kind": kind, "size": size,
                      "free": free if total else "", "mounted": total})
    return disks


def pc_network():
    """Cartes réseau physiques (une interface avec un appareil derrière)."""
    cards = []
    base = path("sys/class/net")
    try:
        entries = sorted(base.iterdir())
    except OSError:
        entries = []
    for dev in entries:
        if dev.name == "lo" or not (dev / "device").exists():
            continue
        kind = "wifi" if (dev / "wireless").exists() or (dev / "phy80211").exists() else "ethernet"
        cards.append({"name": dev.name, "kind": kind, "up": read(dev / "operstate") == "up"})
    return cards


def pc_sound():
    names = []
    for line in read(path("proc/asound/cards")).splitlines():
        match = re.match(r"\s*\d+\s+\[[^\]]*\]:\s*(.*?)\s+-\s+(.+)$", line)
        if match:
            names.append(" ".join(match.group(2).split()))
    return names


def pc_batteries():
    found = []
    base = path("sys/class/power_supply")
    try:
        entries = sorted(base.iterdir())
    except OSError:
        entries = []
    for supply in entries:
        if read(supply / "type") != "Battery" or read(supply / "present", "1") != "1" or not (supply / "capacity").exists():
            continue
        if read(supply / "scope") == "Device":  # souris, manette : pas la batterie du PC
            continue
        health = ""
        for full, design in (("energy_full", "energy_full_design"), ("charge_full", "charge_full_design")):
            try:
                ratio = int(read(supply / full, "0")) * 100 // int(read(supply / design, "0"))
            except (ValueError, ZeroDivisionError):
                continue
            if 0 < ratio <= 150:
                health = min(ratio, 100)
                break
        found.append({"capacity": read(supply / "capacity"), "status": read(supply / "status").lower().replace(" ", "-"),
                      "health": health, "model": " ".join(" ".join((read(supply / "manufacturer"), read(supply / "model_name"))).split())})
    return found


def secure_boot():
    """« 1 », « 0 », ou vide si inconnu (BIOS hérité, variable illisible)."""
    try:
        data = path(SECURE_BOOT_VAR).read_bytes()
    except OSError:
        return ""
    return str(data[4]) if len(data) >= 5 and data[4] in (0, 1) else ""


def pc_status(devices):
    """Clés pc_* du relevé : tout ce que le système a détecté sur la machine."""
    lines = {"pc_vendor": dmi("sys_vendor") or dmi("board_vendor"),
             "pc_model": dmi("product_name") or dmi("board_name"),
             "pc_family": dmi("product_version") if dmi("product_version") and dmi("product_version") != dmi("product_name") else ""}
    model, cores, threads = pc_cpu()
    lines.update(pc_cpu_model=model, pc_cpu_cores=cores or "", pc_cpu_threads=threads or "")
    total = re.search(r"^MemTotal:\s*(\d+)\s*kB", read(path("proc/meminfo")), re.M)
    lines["pc_memory_kb"] = total.group(1) if total else ""
    # Toutes les cartes d'affichage, y compris celle d'une machine virtuelle.
    shown = [d for d in devices if d["class"].startswith("0x03")]
    lines["pc_gpus"] = "|".join(gpu_name(dict(d, kind=VENDORS.get(d["vendor"], "other"))) for d in shown[:4])
    disks = pc_disks()
    lines["pc_disk_count"] = len(disks[:8])
    for i, disk in enumerate(disks[:8]):
        for key, value in disk.items():
            lines[f"pc_disk{i}_{key}"] = value
    nets = pc_network()
    lines["pc_net_count"] = len(nets[:8])
    for i, net in enumerate(nets[:8]):
        lines[f"pc_net{i}_name"], lines[f"pc_net{i}_kind"], lines[f"pc_net{i}_up"] = net["name"], net["kind"], int(net["up"])
    lines["pc_sound"] = "|".join(pc_sound()[:4])
    batteries = pc_batteries()
    lines["pc_battery_count"] = len(batteries[:2])
    for i, bat in enumerate(batteries[:2]):
        for key, value in bat.items():
            lines[f"pc_battery{i}_{key}"] = value
    uefi = path("sys/firmware/efi").is_dir()
    lines["pc_firmware"] = "uefi" if uefi else "bios"
    lines["pc_firmware_bits"] = read(path("sys/firmware/efi/fw_platform_size")) if uefi else ""
    lines["pc_bios_vendor"] = dmi("bios_vendor")
    lines["pc_bios_version"] = dmi("bios_version")
    lines["pc_bios_date"] = dmi("bios_date")
    lines["pc_secure_boot"] = secure_boot() if uefi else ""
    lines["pc_arch"] = os.environ.get("WINLUX_MACHINE") or os.uname().machine
    lines["pc_cpu_64bit"] = int(bool(re.search(r"^flags\s*:.*\blm\b", read(path("proc/cpuinfo")), re.M)))
    return lines


def grub_text():
    return read(GRUB)


def status():
    devices = pci_devices()
    cards = gpus(devices)
    release = kernel_release()
    cand = candidates()
    lines = {"drivers_installable": int(installable()), "drivers_live": int(live()),
             "drivers_virtual": virtual_machine(), "drivers_auto": config().get("AUTO", "1"),
             "kernel": release, "candidates_known": int(CANDIDATES.is_file())}
    # Les pilotes graphiques sont dans l'image depuis #314 : seul le paquet
    # Ubuntu installé dit que tous les pilotes du noyau sont là.
    modules_full = modules_installed(release)
    lines["modules_full"] = int(modules_full)
    lines["modules_candidate"] = cand.get("modules", "")
    lines["gpu_count"] = len(cards)
    kinds = []
    for i, dev in enumerate(cards[:4]):
        kinds.append(dev["kind"])
        lines[f"gpu{i}_name"] = gpu_name(dev)
        lines[f"gpu{i}_vendor"] = dev["kind"]
        lines[f"gpu{i}_pci"] = dev["slot"]
        lines[f"gpu{i}_driver"] = dev["driver"] or "none"
        lines[f"gpu{i}_boot"] = int(dev["boot_vga"])
    lines["gpu_hybrid"] = int(len(cards) > 1)
    if "nvidia" in kinds:
        lines["nvidia_installed"] = installed_version("nvidia-utils-*")
        lines["nvidia_active"] = int(path("sys/module/nvidia").is_dir())
        lines["nvidia_candidate"] = cand.get("nvidia", "")
        lines["nvidia_method"] = cand.get("nvidia_method", "")
        # Pilote installé pour un autre noyau (mise à jour de Winlux) : à refaire.
        lines["nvidia_reinstall"] = int(bool(lines["nvidia_installed"]) and not nvidia_module_present(release))
    mesa = run(["dpkg-query", "-W", "-f=${Version}", "libgl1-mesa-dri"], timeout=10) or ""
    lines["mesa_version"] = mesa.split("-")[0].split(":")[-1]
    lines["vulkan"] = int(any(path(p).is_dir() and any(path(p).glob("*.json"))
                              for p in ("usr/share/vulkan/icd.d", "etc/vulkan/icd.d")))
    missing = []
    for kind in dict.fromkeys(kinds):
        if kind in VAAPI and not vaapi_present(VAAPI[kind][0]):
            missing.append(kind)
    lines["vaapi_missing"] = ",".join(missing)
    lines["vaapi_ok"] = ",".join(k for k in dict.fromkeys(kinds) if k in VAAPI and k not in missing)
    lines["nvenc"] = int(lib_present("libnvidia-encode.so.1"))
    lost = unbound(devices)
    lines["unbound_count"] = len(lost)
    lines["unbound"] = ",".join(f"{d['slot']}:{d['class'][:6]}:{d['vendor'][2:]}:{d['device'][2:]}" for d in lost[:8])
    # Processeur
    vendor = cpu_vendor()
    lines["cpu_vendor"] = vendor
    lines["microcode_revision"] = cpu_field("microcode")
    if vendor:
        lines["microcode_package"] = int(installed("intel-microcode" if vendor == "intel" else "amd64-microcode"))
        lines["microcode_early"] = int(MICROCODE_CPIO in grub_text() and path(MICROCODE_CPIO).is_file())
    cpu = path("sys/devices/system/cpu")
    lines["freq_driver"] = read(cpu / "cpu0/cpufreq/scaling_driver")
    lines["freq_governor"] = read(cpu / "cpu0/cpufreq/scaling_governor")
    lines["freq_status"] = read(cpu / "intel_pstate/status") or read(cpu / "amd_pstate/status")
    # amd_pstate : Zen 2 et plus (famille 0x17+), proposé si le noyau est resté sur acpi-cpufreq.
    lines["amd_pstate_possible"] = int(vendor == "amd" and amd_family() >= 0x17
                                       and lines["freq_driver"] == "acpi-cpufreq")
    temps, fans = hwmon()
    for key, value in temps.items():
        lines[f"temp_{key}"] = value
    lines["fan_count"] = len(fans)
    if fans:
        lines["fans"] = ",".join(str(f) for f in fans[:8])
    # Firmware et périphériques
    lines["fwupd"] = int(have("fwupdtool"))
    fw = keyvals(read(FIRMWARE))
    lines["firmware_updates"] = fw.get("count", "")
    lines["firmware_checked"] = fw.get("checked", "")
    lines["firmware_names"] = fw.get("names", "")
    hw = path("run/winlux/hw")
    lines["wifi"] = int(read(hw / "has-wifi") == "1" or any(path("sys/class/net").glob("*/wireless")))
    lines["wifi_card"] = int(any(d["class"].startswith("0x0280") for d in devices))
    lines["bluetooth"] = int(any(path("sys/class/bluetooth").glob("hci*")))
    lines["touchpad"] = int(read(hw / "has-touchpad") == "1")
    lines["touchscreen"] = int(read(hw / "has-touchscreen") == "1")
    cams = [d for d in path("sys/class/video4linux").glob("video*")
            if "metadata" not in read(d / "name").lower()] if path("sys/class/video4linux").is_dir() else []
    lines["webcam_count"] = len(cams)
    cards_snd = [c for c in path("proc/asound").glob("card[0-9]*")] if path("proc/asound").is_dir() else []
    lines["audio_cards"] = len(cards_snd)
    lines["audio_card_pci"] = int(any(d["class"].startswith("0x0403") for d in devices))
    lines["fingerprint_device"] = int(bool(usb_vendors() & set(FINGERPRINT_VENDORS)))
    lines["fprintd"] = int(have("fprintd-enroll") or installed("fprintd"))
    lines["scanner_tools"] = int(have("scanimage"))
    lines["printer_drivers"] = int(installed("printer-driver-gutenprint"))
    lines["cups"] = int(have("lpstat"))
    lines["reboot_needed"] = int(path("var/lib/winlux/drivers/reboot-needed").is_file())
    lines["busy"] = int(path("run/winlux/drivers.lock").is_dir())
    lines["last"] = read(STATE / "last")
    lines.update(pc_status(devices))
    return "".join(f"{k}={v}\n" for k, v in lines.items())


# ── Dépôts : ce qui est proposé pour ce matériel ───────────────────────────

def apt_show(package):
    return run(["apt-cache", "show", "--no-all-versions", package], timeout=60) or ""


def apt_available(package):
    return "Package:" in apt_show(package)


def simulate(packages):
    """apt-get peut-il installer ces paquets ensemble (sans rien changer) ?"""
    result = run(["apt-get", "install", "-s", "-q", "--no-install-recommends"] + packages, timeout=120)
    return result is not None


def nvidia_choice(release, devices=None):
    """Branche NVIDIA la plus récente qui reconnaît la carte. Le module signé
    précompilé pour ce noyau est préféré ; quand il ne suit plus la version
    des bibliothèques des dépôts (noyau Winlux figé), le module est compilé
    pour ce noyau (DKMS). Renvoie (branche, méthode) ou ("", "")."""
    cards = [d for d in gpus(devices) if d["kind"] == "nvidia" and d["modalias"]]
    if not cards:
        return "", ""
    names = (run(["apt-cache", "pkgnames", "nvidia-driver-"], timeout=60) or "").split()
    branches = []
    for name in names:
        match = re.fullmatch(r"nvidia-driver-([0-9]+)(-open)?", name)
        if match:
            branches.append((int(match.group(1)), bool(match.group(2)), name))
    # Plus récente d'abord ; à version égale, le module ouvert (Turing et plus).
    for version, is_open, name in sorted(branches, reverse=True):
        info = apt_show(name)
        aliases = re.search(r"^Modaliases:\s*nvidia\((.*)\)\s*$", info, re.M)
        if not aliases:
            continue
        patterns = [p.strip() for p in aliases.group(1).split(",")]
        if not any(fnmatchcase(card["modalias"], pattern) for card in cards for pattern in patterns):
            continue
        branch = f"{version}{'-open' if is_open else ''}"
        for method in ("signed", "dkms"):
            if simulate(nvidia_packages(branch, release, method)):
                return branch, method
    return "", ""


def apt_update():
    lists = path("var/lib/apt/lists")
    fresh = any(lists.glob("*_Packages")) and time.time() - max(
        (p.stat().st_mtime for p in lists.glob("*_Packages")), default=0) < 6 * 3600
    if not fresh:
        subprocess.run(["apt-get", "update", "-q"], check=False, timeout=900)


def refresh():
    apt_update()
    release = kernel_release()
    values = {"checked": int(time.time())}
    if apt_available(f"linux-modules-extra-{release}") or apt_available(f"linux-modules-{release}"):
        values["modules"] = release
    values["nvidia"], values["nvidia_method"] = nvidia_choice(release)
    STATE.mkdir(parents=True, exist_ok=True)
    tmp = CANDIDATES.with_suffix(".tmp")
    tmp.write_text("".join(f"{k}={v}\n" for k, v in values.items()))
    tmp.replace(CANDIDATES)
    return 0


# ── Installation ───────────────────────────────────────────────────────────

def say(text):
    print(text, flush=True)
    STATE.mkdir(parents=True, exist_ok=True)
    (STATE / "last").write_text(text.replace("\n", " ") + "\n")


class GrubGuard:
    """Un paquet de noyau Ubuntu relance update-grub, qui écraserait le menu
    Winlux (installation, Windows à côté, secours) : il est remis tel quel."""

    def __enter__(self):
        self.saved = GRUB.read_bytes() if GRUB.is_file() else None
        return self

    def __exit__(self, *_):
        if self.saved is not None and (not GRUB.is_file() or GRUB.read_bytes() != self.saved):
            GRUB.write_bytes(self.saved)
        return False


def apt_install(packages, overwrite=False):
    packages = [p for p in packages if p]
    if not packages:
        return 0
    args = ["apt-get", "install", "-y", "-q", "--no-install-recommends"]
    if overwrite:
        # Les pilotes complets d'Ubuntu reprennent modules.builtin, déjà
        # fourni (identique) par winlux-kernel.
        args += ["-o", "Dpkg::Options::=--force-overwrite"]
    env = dict(os.environ, DEBIAN_FRONTEND="noninteractive")
    with GrubGuard():
        result = subprocess.run(args + packages, env=env, check=False, timeout=3600)
    return result.returncode


def mark_reboot():
    STATE.mkdir(parents=True, exist_ok=True)
    (STATE / "reboot-needed").write_text(time.strftime("%Y-%m-%d %H:%M\n"))


def nvidia_packages(branch, release, method="signed"):
    version = branch.split("-")[0]
    common = [f"nvidia-utils-{version}", f"libnvidia-gl-{version}", f"libnvidia-decode-{version}",
              f"libnvidia-encode-{version}", f"xserver-xorg-video-nvidia-{version}",
              f"libnvidia-cfg1-{version}", "nvidia-vaapi-driver"]
    if method == "dkms":
        return [f"nvidia-dkms-{branch}", f"nvidia-kernel-common-{version}", f"linux-headers-{release}"] + common
    return [f"linux-modules-nvidia-{branch}-{release}", f"nvidia-headless-no-dkms-{branch}"] + common


def module_packages(release):
    """Paquets des pilotes complets du noyau. Jusqu'au noyau 6.x, Ubuntu les
    coupait en linux-modules + linux-modules-extra ; depuis le noyau 7.0
    (Ubuntu 26.04), linux-modules contient tout et -extra n'existe plus :
    le demander faisait échouer apt-get à chaque essai."""
    return [p for p in (f"linux-modules-{release}", f"linux-modules-extra-{release}") if apt_available(p)]


def modules_installed(release):
    return installed(f"linux-modules-{release}") or installed(f"linux-modules-extra-{release}")


def install_modules(release):
    packages = module_packages(release)
    if not packages:
        say("Aucun paquet de pilotes complets pour ce noyau dans les dépôts.")
        return 1
    code = apt_install(packages, overwrite=True)
    if code == 0:
        subprocess.run(["depmod", "-a", release], check=False)
        STATE.mkdir(parents=True, exist_ok=True)
        (STATE / "modules.installed").write_text(release + "\n")
        mark_reboot()
        say("Pilotes complets du noyau installés. Redémarrez pour les utiliser.")
    return code


def install_nvidia(release):
    cand = candidates()
    branch, method = cand.get("nvidia", ""), cand.get("nvidia_method", "signed")
    if not branch:
        branch, method = nvidia_choice(release)
    if not branch:
        say("Aucun pilote NVIDIA des dépôts Ubuntu ne prend en charge cette carte avec ce noyau.")
        return 1
    packages = nvidia_packages(branch, release, method)
    # Le module signé dépend d'un paquet linux-image : la variante complète
    # des pilotes du noyau doit pouvoir écraser les fichiers de winlux-kernel.
    code = apt_install(packages, overwrite=True)
    if code == 0:
        STATE.mkdir(parents=True, exist_ok=True)
        (STATE / "nvidia.packages").write_text("\n".join(packages) + "\n")
        # nouveau ne doit plus prendre la carte ; KMS pour PRIME et Xorg.
        NVIDIA_MODPROBE.parent.mkdir(parents=True, exist_ok=True)
        NVIDIA_MODPROBE.write_text("# Écrit par winlux-drivers (pilote NVIDIA propriétaire)\n"
                                   "blacklist nouveau\noptions nouveau modeset=0\noptions nvidia-drm modeset=1\n")
        subprocess.run(["depmod", "-a", release], check=False)
        mark_reboot()
        say(f"Pilote NVIDIA {branch} installé. Redémarrez pour l’utiliser.")
    return code


def nvidia_module_present(release):
    base = path(f"lib/modules/{release}")
    return any(base.glob("**/nvidia.ko*")) if base.is_dir() else False


def remove_nvidia():
    packages = read(STATE / "nvidia.packages").split()
    if not packages:
        out = run(["dpkg-query", "-W", "-f=${Package}\\n", "*nvidia*"], timeout=10) or ""
        packages = [p for p in out.split() if installed(p)]
    if not packages:
        say("Aucun pilote NVIDIA propriétaire n’est installé.")
        return 0
    env = dict(os.environ, DEBIAN_FRONTEND="noninteractive")
    with GrubGuard():
        code = subprocess.run(["apt-get", "purge", "-y", "-q"] + packages, env=env, check=False, timeout=1800).returncode
    if code == 0:
        (STATE / "nvidia.packages").unlink(missing_ok=True)
        NVIDIA_MODPROBE.unlink(missing_ok=True)
        for conf in ("20-winlux-nvidia.conf", "21-winlux-prime.conf"):
            path(f"usr/share/X11/xorg.conf.d/{conf}").unlink(missing_ok=True)
        mark_reboot()
        say("Pilote NVIDIA retiré : le pilote libre sera utilisé au prochain démarrage.")
    return code


def install_vaapi():
    kinds = list(dict.fromkeys(dev["kind"] for dev in gpus()))
    packages = []
    for kind in kinds:
        if kind not in VAAPI:
            continue
        _, preferred, fallback = VAAPI[kind]
        chosen = [p for p in preferred if apt_available(p)] or [p for p in fallback if apt_available(p)]
        packages += chosen
    if not packages:
        say("Aucun pilote de décodage vidéo matériel n’est proposé pour cette carte.")
        return 1
    code = apt_install(packages + ["vainfo"])
    if code == 0:
        say("Décodage et encodage vidéo matériels installés (VA-API).")
    return code


def cpio_newc(entries):
    """Archive cpio « newc » non compressée (seul format du microcode précoce)."""
    out = bytearray()
    for index, (name, data, mode) in enumerate(entries + [("TRAILER!!!", b"", 0)], start=1):
        encoded = name.encode() + b"\0"
        header = "070701" + "".join(f"{v:08X}" for v in (
            index if name != "TRAILER!!!" else 0, mode, 0, 0, 1, 0, len(data), 0, 0, 0, 0, len(encoded), 0))
        out += header.encode() + encoded
        out += b"\0" * (-len(out) % 4)
        out += data
        out += b"\0" * (-len(out) % 4)
    return bytes(out)


def microcode_blob(vendor):
    if vendor == "intel":
        folder, name = path("lib/firmware/intel-ucode"), "GenuineIntel.bin"
    else:
        folder, name = path("lib/firmware/amd-ucode"), "AuthenticAMD.bin"
    parts = sorted(p for p in folder.glob("*") if p.is_file() and not p.name.endswith((".asc", ".sig")))
    if vendor == "amd":
        parts = [p for p in parts if p.name.startswith("microcode_amd")]
    blob = b"".join(p.read_bytes() for p in parts)
    return name, blob


def build_microcode(vendor):
    name, blob = microcode_blob(vendor)
    if not blob:
        return False
    dirs = [("kernel", b"", 0o40755), ("kernel/x86", b"", 0o40755), ("kernel/x86/microcode", b"", 0o40755)]
    data = cpio_newc(dirs + [(f"kernel/x86/microcode/{name}", blob, 0o100644)])
    target = path(MICROCODE_CPIO)
    if target.is_file() and target.read_bytes() == data:
        return True
    target.parent.mkdir(parents=True, exist_ok=True)
    tmp = target.with_suffix(".tmp")
    tmp.write_bytes(data)
    tmp.replace(target)
    return True


def grub_edit(edit):
    """Modifie les entrées Winlux du menu GRUB ; vérifié, sinon annulé."""
    text = grub_text()
    if not text:
        return False
    changed = edit(text)
    if changed == text:
        return True
    backup = GRUB.with_name("grub.cfg.winlux-drivers")
    backup.write_text(text)
    GRUB.write_text(changed)
    if have("grub-script-check") and subprocess.run(["grub-script-check", str(GRUB)], check=False,
                                                    capture_output=True).returncode != 0:
        GRUB.write_text(text)
        return False
    return True


def with_microcode(text):
    """Ajoute le microcode en tête des initrd des entrées qui lancent le noyau."""
    lines = text.split("\n")
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        if re.match(r"\s*linux\s+/boot/vmlinuz\b", line):
            indent = re.match(r"\s*", line).group(0)
            # initrd éventuel juste après (lignes vides comprises)
            j = i + 1
            while j < len(lines) and not lines[j].strip():
                j += 1
            if j < len(lines) and re.match(r"\s*initrd\s", lines[j]):
                if MICROCODE_CPIO not in lines[j]:
                    lines[j] = re.sub(r"^(\s*initrd)\s+", rf"\1 {MICROCODE_CPIO} ", lines[j])
            else:
                out.append(f"{indent}initrd {MICROCODE_CPIO}")
        i += 1
    return "\n".join(out)


def with_kernel_option(option):
    key = option.split("=", 1)[0]

    def edit(text):
        def fix(match):
            line = match.group(0)
            if re.search(rf"\s{re.escape(key)}=\S*", line):
                return re.sub(rf"\s{re.escape(key)}=\S*", f" {option}", line)
            return line + " " + option
        return re.sub(r"^\s*linux\s+/boot/vmlinuz\b[^\n]*", fix, text, flags=re.M)
    return edit


def install_microcode():
    vendor = cpu_vendor()
    if not vendor:
        say("Processeur ni Intel ni AMD : aucun microcode à installer.")
        return 1
    package = "intel-microcode" if vendor == "intel" else "amd64-microcode"
    if not installed(package):
        code = apt_install([package])
        if code:
            return code
    if not build_microcode(vendor) or not grub_edit(with_microcode):
        say("Microcode installé, mais le menu de démarrage n’a pas pu être mis à jour.")
        return 1
    mark_reboot()
    say("Microcode du processeur installé : il sera chargé dès le prochain démarrage.")
    return 0


def install_amd_pstate():
    if not (cpu_vendor() == "amd" and amd_family() >= 0x17):
        say("Ce processeur n’utilise pas amd_pstate.")
        return 1
    if not grub_edit(with_kernel_option("amd_pstate=active")):
        say("Le menu de démarrage n’a pas pu être mis à jour.")
        return 1
    mark_reboot()
    say("Pilote de fréquence amd_pstate activé au prochain démarrage.")
    return 0


def simple(packages, message):
    code = apt_install(packages)
    if code == 0:
        say(message)
    return code


def install(item):
    if item not in ITEMS:
        print(f"élément inconnu : {item}", file=sys.stderr)
        return 2
    if not installable():
        say("Les pilotes s’installent sur un système installé, pas depuis la session live.")
        return 1
    release = kernel_release()
    if item not in ("microcode", "amd-pstate"):
        apt_update()
    if item == "modules":
        return install_modules(release)
    if item == "nvidia":
        return install_nvidia(release)
    if item == "mesa":
        return simple(["libgl1-mesa-dri", "libglx-mesa0", "mesa-vulkan-drivers", "mesa-va-drivers", "libgbm1"],
                      "Pilotes Mesa à jour (OpenGL, Vulkan, vidéo).")
    if item == "vaapi":
        return install_vaapi()
    if item == "microcode":
        apt_update()
        return install_microcode()
    if item == "amd-pstate":
        return install_amd_pstate()
    if item == "fwupd":
        return simple(["fwupd"], "Mises à jour du micrologiciel disponibles (fwupd).")
    if item == "fingerprint":
        return simple(["fprintd", "libpam-fprintd"], "Lecteur d’empreintes pris en charge.")
    if item == "scanner":
        return simple(["sane-utils", "sane-airscan"], "Scanners USB et réseau pris en charge.")
    return simple(PRINTER_DRIVERS, "Pilotes d’imprimantes supplémentaires installés.")


def default_route():
    return any(line.split()[1:2] == ["00000000"] for line in read(path("proc/net/route")).splitlines()[1:])


def wait_network(attempts=60):
    for _ in range(attempts):
        if default_route():
            return True
        time.sleep(5)
    return False


def auto():
    """Premier démarrage d'un système installé : comme Windows Update, les
    pilotes recommandés pour ce matériel s'installent tout seuls."""
    done = STATE / "auto.done"
    if done.is_file() or not installable() or virtual_machine() or config().get("AUTO") == "0":
        return 0
    if not wait_network():
        return 1
    refresh()
    cand = candidates()
    release = kernel_release()
    failed = 0
    if cand.get("modules") and not modules_installed(release):
        failed |= install_modules(release) != 0
    if cpu_vendor():
        failed |= install_microcode() != 0
    if cand.get("nvidia") and not installed_version("nvidia-utils-*"):
        failed |= install_nvidia(release) != 0
    if any(dev["kind"] in VAAPI and not vaapi_present(VAAPI[dev["kind"]][0]) for dev in gpus()
           if dev["kind"] != "nvidia" or cand.get("nvidia")):
        failed |= install_vaapi() != 0
    if not have("fwupdtool"):
        failed |= simple(["fwupd"], "fwupd installé.") != 0
    if usb_vendors() & set(FINGERPRINT_VENDORS) and not installed("fprintd"):
        failed |= simple(["fprintd", "libpam-fprintd"], "Lecteur d’empreintes pris en charge.") != 0
    if not failed:
        done.write_text(time.strftime("%Y-%m-%d %H:%M\n"))
    return int(failed)


def boot():
    """Chaque démarrage : microcode régénéré après une mise à jour du paquet,
    nouveau écarté quand le pilote NVIDIA est installé."""
    vendor = cpu_vendor()
    if vendor and installed("intel-microcode" if vendor == "intel" else "amd64-microcode"):
        if build_microcode(vendor) and MICROCODE_CPIO not in grub_text():
            grub_edit(with_microcode)
    # Noyau changé par une mise à jour de Winlux : le pilote NVIDIA (et les
    # pilotes complets) sont refaits pour lui, en arrière-plan, réseau venu.
    release = kernel_release()
    redo = (STATE / "nvidia.packages").is_file() and not nvidia_module_present(release)
    redo_modules = (STATE / "modules.installed").is_file() and not modules_installed(release)
    if (redo or redo_modules) and installable() and config().get("AUTO") != "0" and os.fork() == 0:
        try:
            os.setsid()
            if wait_network():
                CANDIDATES.unlink(missing_ok=True)
                refresh()
                if redo_modules:
                    install_modules(release)
                if redo:
                    install_nvidia(release)
        finally:
            os._exit(0)
    flag = STATE / "reboot-needed"
    marker = path("run/winlux/drivers-booted")
    if flag.is_file() and not marker.exists():
        try:
            booted = float(read(path("proc/uptime"), "0").split()[0])
        except (ValueError, IndexError):
            booted = 0
        if time.time() - booted > flag.stat().st_mtime:
            flag.unlink(missing_ok=True)
        marker.parent.mkdir(parents=True, exist_ok=True)
        marker.write_text("1\n")
    return 0


def firmware_check():
    if not have("fwupdtool"):
        print("fwupd absent", file=sys.stderr)
        return 1
    run(["fwupdtool", "refresh", "--force"], timeout=300)
    out = run(["fwupdtool", "get-updates", "--no-unreported-check"], timeout=300) or ""
    names = re.findall(r"^[├└│ ─]*([^\n:]+?)\s*:\s*$", out, re.M)
    count = len(re.findall(r"New version|Nouvelle version|Version:", out)) if out else 0
    STATE.mkdir(parents=True, exist_ok=True)
    FIRMWARE.write_text(f"count={count}\nchecked={int(time.time())}\nnames={'|'.join(n.strip() for n in names[:6])}\n")
    say(f"{count} mise(s) à jour de micrologiciel disponible(s).")
    return 0


def firmware_update():
    if not have("fwupdtool"):
        return 1
    code = subprocess.run(["fwupdtool", "update", "--assume-yes", "--no-reboot-check"],
                          check=False, timeout=3600).returncode
    if code == 0:
        mark_reboot()
        say("Micrologiciels mis à jour. Redémarrez pour terminer.")
    firmware_check()
    return code


def main(argv):
    command = argv[1] if len(argv) > 1 else "status"
    if command == "status":
        sys.stdout.write(status())
        return 0
    if command == "set-auto" and len(argv) == 3 and argv[2] in ("0", "1"):
        values = config()
        values["AUTO"] = argv[2]
        CONFIG.parent.mkdir(parents=True, exist_ok=True)
        CONFIG.write_text("".join(f"{k}={v}\n" for k, v in values.items()))
        return 0
    simple_commands = {"refresh": refresh, "auto": auto, "boot": boot,
                       "firmware-check": firmware_check, "firmware-update": firmware_update}
    if command in simple_commands or command in ("install", "remove"):
        lock = path("run/winlux/drivers.lock")
        try:
            lock.parent.mkdir(parents=True, exist_ok=True)
            lock.mkdir()
        except FileExistsError:
            print("une installation de pilotes est déjà en cours", file=sys.stderr)
            return 1
        try:
            if command == "install" and len(argv) == 3:
                return install(argv[2])
            if command == "remove" and argv[2:] == ["nvidia"]:
                return remove_nvidia()
            if command in simple_commands:
                return simple_commands[command]()
        finally:
            lock.rmdir()
    print(__doc__.strip().split("\n\n")[1], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
