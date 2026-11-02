#!/usr/bin/python3
"""winlux-drivers et la page Matériel et pilotes, sans root ni vrai matériel.

Une fausse racine (sysfs, /proc, /boot/grub, /lib/firmware) décrit un
portable hybride Intel + NVIDIA ou un PC AMD ; apt-get, apt-cache et
dpkg-query sont remplacés par des faux qui notent leurs arguments. Vérifie
la détection, le choix de la branche NVIDIA d'après le modalias de la carte,
le module signé ou DKMS, le menu GRUB préservé, le microcode précoce, le
retour au pilote libre, la session live et la page des Paramètres (4 langues,
sections masquées sans le matériel) si un affichage est disponible.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
checks = 0
GRUB = """set default=0
menuentry "Winlux Desktop" {
    search --no-floppy --fs-uuid 1234 --set=root
    linux /boot/vmlinuz root=UUID=1234 rw rootwait rootfstype=ext4 init=/init quiet
    initrd /boot/winlux-initramfs.cpio.gz
}
menuentry "Winlux diagnostic" --hotkey=d {
    search --no-floppy --fs-uuid 1234 --set=root
    linux /boot/vmlinuz root=UUID=1234 rw rootwait rootfstype=ext4 init=/init
}
"""
RELEASE = "7.0.0-14-generic"


def ok(condition, label):
    global checks
    if not condition:
        print(f"ÉCHEC : {label}", file=sys.stderr)
        raise SystemExit(1)
    checks += 1
    print(f"  OK     {label}")


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def pci(root, slot, cls, vendor, device, driver=None, boot=False):
    dev = root / "sys/bus/pci/devices" / slot
    write(dev / "class", cls + "\n")
    write(dev / "vendor", vendor + "\n")
    write(dev / "device", device + "\n")
    write(dev / "modalias", f"pci:v0000{vendor[2:].upper()}d0000{device[2:].upper()}sv00001043sd00001E3Ebc{cls[2:4].upper()}sc{cls[4:6].upper()}i00\n")
    if boot:
        write(dev / "boot_vga", "1\n")
    if driver:
        target = root / "sys/bus/pci/drivers" / driver
        target.mkdir(parents=True, exist_ok=True)
        (dev / "driver").symlink_to(target)


def laptop(root, installed=True):
    """Portable Intel Alder Lake + RTX 3060, Wi-Fi Intel sans pilote."""
    pci(root, "0000:00:02.0", "0x030000", "0x8086", "0x46a6", "i915", boot=True)
    pci(root, "0000:01:00.0", "0x030200", "0x10de", "0x2560", "nouveau")
    pci(root, "0000:00:14.3", "0x028000", "0x8086", "0x51f0")
    pci(root, "0000:00:1f.3", "0x040380", "0x8086", "0x51c8", "snd_hda_intel")
    write(root / "proc/cpuinfo", "processor\t: 0\nvendor_id\t: GenuineIntel\ncpu family\t: 6\nmicrocode\t: 0x42c\n")
    cpu = root / "sys/devices/system/cpu"
    write(cpu / "cpu0/cpufreq/scaling_driver", "intel_pstate\n")
    write(cpu / "cpu0/cpufreq/scaling_governor", "powersave\n")
    write(cpu / "intel_pstate/status", "active\n")
    write(root / "sys/class/hwmon/hwmon3/name", "coretemp\n")
    write(root / "sys/class/hwmon/hwmon3/temp1_input", "47000\n")
    write(root / "sys/class/hwmon/hwmon3/temp1_label", "Package id 0\n")
    write(root / "sys/class/hwmon/hwmon4/name", "nvme\n")
    write(root / "sys/class/hwmon/hwmon4/temp1_input", "38850\n")
    write(root / "sys/bus/usb/devices/3-4/idVendor", "27c6\n")
    write(root / "boot/grub/grub.cfg", GRUB)
    write(root / "boot/vmlinuz", "noyau")
    for name in ("06-9a-03", "06-97-02"):
        write(root / "lib/firmware/intel-ucode" / name, "UCODE" + name)
    write(root / "usr/lib/x86_64-linux-gnu/dri/radeonsi_drv_video.so", "")
    if installed:
        write(root / "etc/winlux/install-config.json", "{}\n")


def fake_tools(bindir, log, state):
    """apt-get, apt-cache et dpkg-query factices ; les paquets « installés »
    sont notés dans state/installed."""
    (state / "installed").touch()
    tools = {
        "apt-cache": f"""#!/bin/sh
echo "apt-cache $*" >>{log}
case "$1" in
  pkgnames) printf 'nvidia-driver-580\\nnvidia-driver-580-open\\nnvidia-driver-470\\nnvidia-driver-595-open\\nnvidia-driver-595-server-open\\n' ;;
  show)
    p=$3
    case "$p" in
      nvidia-driver-595-open) echo "Package: $p"; echo 'Modaliases: nvidia(pci:v000010DEd00002560sv*sd*bc03sc*i*, pci:v000010DEd00002504sv*sd*bc03sc*i*)' ;;
      nvidia-driver-580|nvidia-driver-580-open) echo "Package: $p"; echo 'Modaliases: nvidia(pci:v000010DEd00002560sv*sd*bc03sc*i*)' ;;
      nvidia-driver-470) echo "Package: $p"; echo 'Modaliases: nvidia(pci:v000010DEd00000FC0sv*sd*bc03sc*i*)' ;;
      linux-modules-{RELEASE}|linux-modules-extra-{RELEASE}|intel-media-va-driver-non-free|mesa-va-drivers|nvidia-vaapi-driver) echo "Package: $p" ;;
      *) exit 0 ;;
    esac ;;
esac
""",
        "apt-get": f"""#!/bin/sh
echo "apt-get $*" >>{log}
case " $* " in
  *" -s "*)
    # Le module signé de la branche 595 ne suit plus les bibliothèques : DKMS.
    case " $* " in *linux-modules-nvidia-595-open*) exit 100 ;; esac
    exit 0 ;;
esac
case "$1" in
  install)
    for p in "$@"; do case "$p" in -*|install|Dpkg*) ;; *) echo "$p" >>{state}/installed ;; esac; done
    # Comme le script de linux-image : update-grub réécrit le menu.
    echo 'menu réécrit par update-grub' >"$WINLUX_DRIVERS_ROOT/boot/grub/grub.cfg" ;;
  purge) for p in "$@"; do grep -vx "$p" {state}/installed >{state}/i.tmp; mv {state}/i.tmp {state}/installed; done ;;
esac
exit 0
""",
        "dpkg-query": f"""#!/bin/sh
last=$(eval echo \\${{$#}})
case "$last" in
  *'*'*) pattern=$(echo "$last" | sed 's/\\*/.*/g'); grep -x "$pattern" {state}/installed | while read -r p; do echo "$p install ok installed"; done ;;
  libgl1-mesa-dri) printf '25.2.8-0ubuntu1' ;;
  *) if grep -qx "$last" {state}/installed; then printf 'install ok installed'; else exit 1; fi ;;
esac
""",
        "depmod": f"#!/bin/sh\necho \"depmod $*\" >>{log}\n",
        "grub-script-check": "#!/bin/sh\ngrep -q 'menuentry' \"$1\"\n",
    }
    for name, body in tools.items():
        write(bindir / name, body)
        (bindir / name).chmod(0o755)


def load(name, path, env=None):
    os.environ.update(env or {})
    loader = SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def drivers(root, *args):
    env = dict(os.environ, WINLUX_DRIVERS_ROOT=str(root), WINLUX_KERNEL_RELEASE=RELEASE)
    result = subprocess.run([sys.executable, "-B", str(HERE / "winlux-drivers.py"), *args],
                            capture_output=True, text=True, env=env, timeout=60)
    return result


def status(root):
    out = drivers(root, "status").stdout
    return dict(line.split("=", 1) for line in out.splitlines() if "=" in line)


def main():
    with tempfile.TemporaryDirectory() as temporary:
        base = Path(temporary)
        root, bindir, state = base / "root", base / "bin", base / "state"
        root.mkdir(); state.mkdir()
        log = base / "calls.log"
        fake_tools(bindir, log, state)
        os.environ["PATH"] = f"{bindir}:{os.environ['PATH']}"
        laptop(root)

        print("Détection (portable Intel + NVIDIA)")
        s = status(root)
        ok(s["gpu_count"] == "2" and s["gpu_hybrid"] == "1", "deux cartes, machine hybride")
        ok(s["gpu0_vendor"] == "intel" and s["gpu0_driver"] == "i915" and s["gpu0_boot"] == "1", "Intel principale avec i915")
        ok(s["gpu1_vendor"] == "nvidia" and s["gpu1_driver"] == "nouveau", "NVIDIA sur nouveau")
        ok(s["vaapi_missing"] == "intel,nvidia", "VA-API manquante pour Intel et NVIDIA")
        ok(s["unbound_count"] == "1" and "0000:00:14.3:0x0280" in s["unbound"], "carte Wi-Fi sans pilote signalée")
        ok(s["wifi_card"] == "1" and s["wifi"] == "0", "Wi-Fi présente mais non reconnue")
        ok(s["cpu_vendor"] == "intel" and s["microcode_revision"] == "0x42c", "microcode Intel lu")
        ok(s["freq_driver"] == "intel_pstate" and s["freq_status"] == "active", "pilote de fréquence intel_pstate")
        ok(s["temp_cpu"] == "47" and s["temp_disk"] == "38", "températures processeur et SSD")
        ok(s["fingerprint_device"] == "1" and s["fprintd"] == "0", "lecteur d’empreintes Goodix détecté")
        ok(s["drivers_installable"] == "1" and s["modules_full"] == "0", "système installé, pilotes complets absents")

        print("Dépôts : branche NVIDIA d'après la carte")
        result = drivers(root, "refresh")
        ok(result.returncode == 0, "refresh")
        s = status(root)
        ok(s["nvidia_candidate"] == "595-open" and s["nvidia_method"] == "dkms",
           "595-open choisie (580 aussi compatible mais plus ancienne, 470 et -server écartées), DKMS faute de module signé à jour")
        ok(s["modules_candidate"] == RELEASE, "pilotes complets proposés pour ce noyau")

        print("Installation du pilote NVIDIA")
        result = drivers(root, "install", "nvidia")
        ok(result.returncode == 0, "install nvidia")
        calls = log.read_text()
        ok("nvidia-dkms-595-open" in calls and f"linux-headers-{RELEASE}" in calls, "DKMS et en-têtes du noyau Winlux")
        ok("--force-overwrite" in calls and "--no-install-recommends" in calls, "options apt attendues")
        ok((root / "boot/grub/grub.cfg").read_text() == GRUB, "menu GRUB Winlux remis après update-grub")
        modprobe = (root / "etc/modprobe.d/winlux-nvidia.conf").read_text()
        ok("blacklist nouveau" in modprobe and "nvidia-drm modeset=1" in modprobe, "nouveau écarté, KMS NVIDIA")
        ok((root / "var/lib/winlux/drivers/reboot-needed").is_file(), "redémarrage demandé")
        s = status(root)
        ok(s["nvidia_installed"] == "nvidia-utils-595" and s["nvidia_reinstall"] == "1",
           "pilote noté installé ; module absent pour ce noyau → à refaire")
        write(root / f"lib/modules/{RELEASE}/updates/dkms/nvidia.ko.zst", "")
        ok(status(root)["nvidia_reinstall"] == "0", "module présent pour ce noyau")

        print("Retour au pilote libre")
        result = drivers(root, "remove", "nvidia")
        ok(result.returncode == 0 and "apt-get purge" in log.read_text(), "purge des paquets installés")
        ok(not (root / "etc/modprobe.d/winlux-nvidia.conf").exists(), "nouveau de nouveau autorisé")
        ok((root / "boot/grub/grub.cfg").read_text() == GRUB, "menu GRUB toujours intact")

        print("Microcode précoce")
        write(state / "installed", (state / "installed").read_text() + "intel-microcode\n")
        result = drivers(root, "install", "microcode")
        ok(result.returncode == 0, "install microcode")
        cpio = (root / "boot/winlux-microcode.cpio").read_bytes()
        ok(cpio.startswith(b"070701") and b"kernel/x86/microcode/GenuineIntel.bin" in cpio
           and b"UCODE06-97-02UCODE06-9a-03" in cpio and b"TRAILER!!!" in cpio, "cpio newc avec GenuineIntel.bin")
        grub = (root / "boot/grub/grub.cfg").read_text()
        ok("initrd /boot/winlux-microcode.cpio /boot/winlux-initramfs.cpio.gz" in grub, "microcode en tête de l’initrd existant")
        ok(grub.count("/boot/winlux-microcode.cpio") == 2, "ajouté aussi à l’entrée sans initrd")
        drivers(root, "install", "microcode")
        ok((root / "boot/grub/grub.cfg").read_text() == grub, "deuxième passage sans doublon")
        s = status(root)
        ok(s["microcode_package"] == "1" and s["microcode_early"] == "1", "microcode signalé à jour")

        print("Pilotes complets, VA-API, empreintes")
        ok(drivers(root, "install", "modules").returncode == 0, "install modules")
        calls = log.read_text()
        ok(f"linux-modules-extra-{RELEASE}" in calls and f"depmod -a {RELEASE}" in calls, "linux-modules-extra du noyau Winlux puis depmod")
        ok(drivers(root, "install", "vaapi").returncode == 0, "install vaapi")
        ok("intel-media-va-driver-non-free" in log.read_text() and "nvidia-vaapi-driver" in log.read_text(), "iHD (avec encodage) et NVDEC")
        ok(drivers(root, "install", "fingerprint").returncode == 0 and "fprintd" in log.read_text(), "fprintd")
        ok(drivers(root, "install", "inconnu").returncode == 2, "élément inconnu refusé")

        print("PC AMD Ryzen sur acpi-cpufreq")
        amd = base / "amd"
        amd.mkdir()
        pci(amd, "0000:0a:00.0", "0x030000", "0x1002", "0x73df", "amdgpu", boot=True)
        write(amd / "proc/cpuinfo", "vendor_id\t: AuthenticAMD\ncpu family\t: 25\nmicrocode\t: 0xa201204\n")
        write(amd / "sys/devices/system/cpu/cpu0/cpufreq/scaling_driver", "acpi-cpufreq\n")
        write(amd / "sys/class/hwmon/hwmon1/name", "k10temp\n")
        write(amd / "sys/class/hwmon/hwmon1/temp1_input", "55250\n")
        write(amd / "sys/class/hwmon/hwmon1/temp1_label", "Tctl\n")
        write(amd / "usr/lib/x86_64-linux-gnu/dri/radeonsi_drv_video.so", "")
        write(amd / "boot/grub/grub.cfg", GRUB)
        write(amd / "etc/winlux/install-config.json", "{}\n")
        s = status(amd)
        ok(s["gpu_count"] == "1" and s["vaapi_ok"] == "amd" and not s["vaapi_missing"], "VA-API radeonsi déjà là")
        ok(s["amd_pstate_possible"] == "1" and s["temp_cpu"] == "55", "amd_pstate proposé, Tctl lu")
        ok("nvidia_candidate" not in s, "rien de NVIDIA sans carte NVIDIA")
        ok(drivers(amd, "install", "amd-pstate").returncode == 0, "install amd-pstate")
        grub = (amd / "boot/grub/grub.cfg").read_text()
        ok(grub.count("amd_pstate=active") == 2, "option du noyau ajoutée aux deux entrées")

        print("Session live et machine virtuelle")
        live = base / "live"
        live.mkdir()
        laptop(live, installed=False)
        s = status(live)
        ok(s["drivers_installable"] == "0" and s["drivers_live"] == "1", "live : rien à installer")
        ok(drivers(live, "install", "nvidia").returncode == 1, "installation refusée en live")
        vm = base / "vm"
        vm.mkdir()
        pci(vm, "0000:00:02.0", "0x030000", "0x1234", "0x1111", "bochs-drm", boot=True)
        write(vm / "run/winlux/hw/virtual-machine", "qemu\n")
        write(vm / "etc/winlux/install-config.json", "{}\n")
        s = status(vm)
        ok(s["gpu_count"] == "0" and s["drivers_virtual"] == "qemu", "VM : carte émulée ignorée")
        ok(drivers(vm, "auto").returncode == 0 and not (vm / "var/lib/winlux/drivers/auto.done").exists(),
           "VM : pas d’installation automatique")

        settings_page(root, amd, vm)
    print(f"{checks} vérifications réussies.")


def settings_page(laptop_root, amd_root, vm_root):
    """La page et les sections rendues pour de vrai, si un affichage existe."""
    if not os.environ.get("DISPLAY"):
        print("  (pas d’affichage : page des Paramètres non rendue)")
        return
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk

    class Window(Gtk.Window):
        def __init__(self, module):
            super().__init__()
            self.body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            self.add(self.body)
            self.actions_sent = []
            self.module = module

        def section(self, title, text=None):
            self.body.pack_start(Gtk.Label(label=title), False, False, 0)
            if text: self.hint(text)

        def hint(self, text, parent=None):
            (parent or self.body).pack_start(Gtk.Label(label=text), False, False, 0)

        def card(self):
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            self.body.pack_start(box, False, False, 0)
            return box

        def row(self, parent, title, widget, subtitle=None):
            line = Gtk.Box()
            line.pack_start(Gtk.Label(label=title), False, False, 0)
            if subtitle: line.pack_start(Gtk.Label(label=subtitle), False, False, 0)
            if widget is not None: line.pack_start(widget, False, False, 0)
            parent.pack_start(line, False, False, 0)

        def switch(self, parent, title, active, changed, subtitle=None):
            self.row(parent, title, Gtk.Switch(active=active), subtitle)

        def actions(self, parent, buttons):
            for title, callback, *rest in buttons:
                button = Gtk.Button(label=title)
                button.connect("clicked", lambda *_, cb=callback: cb())
                parent.pack_start(button, False, False, 0)

        def execute(self, action):
            self.actions_sent.append(action)

        def confirm(self, text):
            return True

        def open_page(self, page):
            self.actions_sent.append(f"page-{page}")

    def texts(widget):
        out = []
        if isinstance(widget, Gtk.Label): out.append(widget.get_text())
        if isinstance(widget, Gtk.Button) and widget.get_label(): out.append(widget.get_label())
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): out += texts(child)
        return out

    def click(widget, text):
        if isinstance(widget, Gtk.Button) and widget.get_label() == text:
            widget.clicked()
            return True
        return isinstance(widget, Gtk.Container) and any(click(c, text) for c in widget.get_children())

    def render(root, lang, part):
        report = Path(tempfile.mkdtemp()) / "drivers.txt"
        report.write_text(drivers(root, "status").stdout)
        module = load(f"hardware_page_{lang}_{part}", HERE / "winlux-hardware-page.py",
                      {"WINLUX_LANG": lang, "WINLUX_DRIVERS_REPORT": str(report)})
        window = Window(module)
        getattr(module, part)(window)
        return window, texts(window.body)

    print("Page Matériel et pilotes (Xvfb)")
    expected = {"fr": ("Pilotes recommandés", "Pilote NVIDIA propriétaire", "Microcode et fréquence"),
                "en": ("Recommended drivers", "Proprietary NVIDIA driver", "Microcode and frequency"),
                "es": ("Controladores recomendados", "Controlador propietario de NVIDIA", "Microcódigo y frecuencia"),
                "de": ("Empfohlene Treiber", "Proprietärer NVIDIA-Treiber", "Microcode und Taktfrequenz")}
    for lang, (rec, nvidia, cpu) in expected.items():
        window, shown = render(laptop_root, lang, "page")
        ok(rec in shown, f"{lang} : section des pilotes recommandés")
        window, shown = render(laptop_root, lang, "gpu_section")
        ok(nvidia in shown, f"{lang} : section NVIDIA dans Graphismes")
        window, shown = render(laptop_root, lang, "cpu_section")
        ok(cpu in shown, f"{lang} : microcode et fréquence dans Processeur")
    window, shown = render(laptop_root, "fr", "page")
    ok(any("Wi-Fi" == t for t in shown) and any("Non reconnu" in t for t in shown), "Wi-Fi sans pilote marquée non reconnue")
    ok(click(window.body, "Installer") and window.actions_sent[-1].startswith("drivers-install-"), "bouton Installer → winlux-control")
    window, shown = render(amd_root, "fr", "gpu_section")
    ok("Pilote NVIDIA propriétaire" not in shown, "pas de section NVIDIA sur une carte AMD")
    window, shown = render(vm_root, "fr", "gpu_section")
    ok(not shown, "VM : section du pilote graphique masquée")
    window, shown = render(vm_root, "fr", "page")
    ok("Pilotes recommandés" not in shown and any("Machine virtuelle" in t for t in shown), "VM : rien à installer, dit clairement")


if __name__ == "__main__":
    main()
