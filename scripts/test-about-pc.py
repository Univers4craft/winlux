#!/usr/bin/python3
"""Paramètres > À propos : section « Votre PC » sous la version et le noyau,
et l'en-tête « À propos de ce PC » avec le démarrage mesuré (#3053).

Deux fausses machines (sysfs, /proc, DMI, variables EFI) : une tablette
UEFI 32 bits sur batterie, Secure Boot désactivé, et un PC de bureau en BIOS
hérité. Vérifie le relevé pc_* de winlux-drivers, les lignes affichées dans
les quatre langues, et que ce qui n'existe pas sur la machine (batterie,
Secure Boot en BIOS hérité) n'a pas de ligne — masqué, jamais grisé. La
section est rendue dans GTK si un affichage est disponible.
"""
from importlib.machinery import SourceFileLoader
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        print(f"ÉCHEC : {label}", file=sys.stderr)
        raise SystemExit(1)
    checks += 1
    print(f"  OK     {label}")


def write(path, text, binary=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    if binary: path.write_bytes(text)
    else: path.write_text(text)


def common(root, cpu_name, threads):
    blocks = []
    for i in range(threads):
        blocks.append(f"processor\t: {i}\nvendor_id\t: GenuineIntel\nmodel name\t: {cpu_name}\n"
                      f"physical id\t: 0\ncore id\t: {i // 2}\nflags\t\t: fpu lm sse2\n")
    write(root / "proc/cpuinfo", "\n".join(blocks))
    write(root / "proc/meminfo", "MemTotal:        3930000 kB\nMemFree:  100 kB\n")


def tablet(root):
    """Tablette Atom : UEFI 32 bits, eMMC, Wi-Fi, batterie, Secure Boot désactivé."""
    common(root, "Intel(R) Atom(TM) x5-Z8350  CPU @ 1.44GHz", 4)
    dmi = root / "sys/class/dmi/id"
    write(dmi / "sys_vendor", "ASUSTeK COMPUTER INC.\n")
    write(dmi / "product_name", "T100HAN\n")
    write(dmi / "product_version", "1.0\n")
    write(dmi / "bios_vendor", "American Megatrends Inc.\n")
    write(dmi / "bios_version", "T100HAN.218\n")
    write(dmi / "bios_date", "05/18/2017\n")
    write(root / "sys/firmware/efi/fw_platform_size", "32\n")
    write(root / "sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c", b"\x06\x00\x00\x00\x00", binary=True)
    write(root / "sys/block/mmcblk0/size", str(61071360) + "\n")
    write(root / "sys/block/mmcblk0/removable", "0\n")
    write(root / "sys/block/mmcblk0/device/name", "DF4032\n")
    (root / "sys/block/mmcblk0/mmcblk0p2").mkdir(parents=True)
    write(root / "sys/block/loop0/size", "999999\n")
    write(root / "proc/mounts", "/dev/mmcblk0p2 / ext4 rw 0 0\n/dev/loop0 /snap squashfs ro 0 0\n")
    net = root / "sys/class/net/wlan0"
    write(net / "operstate", "up\n")
    (net / "wireless").mkdir(parents=True)
    (net / "device").mkdir(parents=True)
    write(root / "sys/class/net/lo/operstate", "unknown\n")
    write(root / "sys/class/net/docker0/operstate", "up\n")
    write(root / "proc/asound/cards", " 0 [bytcrrt5640    ]: bytcr-rt5640 - bytcr-rt5640\n                      ASUSTeK-T100HAN\n")
    bat = root / "sys/class/power_supply/BAT0"
    for name, value in (("type", "Battery"), ("present", "1"), ("capacity", "76"), ("status", "Discharging"),
                        ("energy_full", "28000000"), ("energy_full_design", "31000000"),
                        ("manufacturer", "ASUSTeK"), ("model_name", "T100HA")):
        write(bat / name, value + "\n")
    mouse = root / "sys/class/power_supply/hidpp_battery_0"
    for name, value in (("type", "Battery"), ("scope", "Device"), ("capacity", "40")):
        write(mouse / name, value + "\n")
    dev = root / "sys/bus/pci/devices/0000:00:02.0"
    for name, value in (("class", "0x030000"), ("vendor", "0x8086"), ("device", "0x22b0")):
        write(dev / name, value + "\n")


def desktop(root):
    """PC de bureau : BIOS hérité, disque dur + SSD, Ethernet, pas de batterie."""
    common(root, "AMD Ryzen 5 5600X 6-Core Processor", 12)
    write(root / "sys/class/dmi/id/sys_vendor", "To Be Filled By O.E.M.\n")
    write(root / "sys/class/dmi/id/product_name", "System Product Name\n")
    write(root / "sys/class/dmi/id/board_vendor", "Micro-Star International Co., Ltd.\n")
    write(root / "sys/class/dmi/id/board_name", "B550-A PRO (MS-7C56)\n")
    for name, sectors, rotational, model in (("sda", 3907029168, "1", "ST2000DM008-2FR1"),
                                             ("nvme0n1", 1000215216, "0", "Samsung SSD 980 1TB")):
        write(root / f"sys/block/{name}/size", f"{sectors}\n")
        write(root / f"sys/block/{name}/queue/rotational", rotational + "\n")
        write(root / f"sys/block/{name}/device/model", model + "\n")
    write(root / "sys/class/net/enp5s0/operstate", "up\n")
    (root / "sys/class/net/enp5s0/device").mkdir(parents=True)
    write(root / "proc/asound/cards", " 0 [Generic        ]: HDA-Intel - HD-Audio Generic\n")


def fake_bin(directory):
    directory.mkdir(parents=True, exist_ok=True)
    tool = directory / "lspci"
    tool.write_text("#!/bin/sh\necho '00:02.0 VGA compatible controller: Intel Corporation Atom/Celeron/Pentium Processor x5-E8000/J3xxx/N3xxx Integrated Graphics Controller (rev 35)'\n")
    tool.chmod(0o755)


def status(root, bindir):
    env = {**os.environ, "WINLUX_DRIVERS_ROOT": str(root), "WINLUX_MACHINE": "x86_64",
           "WINLUX_KERNEL_RELEASE": "7.0.0-14-generic", "PATH": f"{bindir}:/usr/bin:/bin"}
    out = subprocess.run([sys.executable, "-B", str(HERE / "winlux-drivers.py"), "status"], env=env,
                         capture_output=True, text=True, check=True).stdout
    return out


def load(name, path, env):
    saved = {key: os.environ.get(key) for key in env}
    os.environ.update(env)
    try:
        loader = SourceFileLoader(name, str(path))
        spec = importlib.util.spec_from_loader(name, loader)
        module = importlib.util.module_from_spec(spec)
        loader.exec_module(module)
        return module
    finally:
        for key, value in saved.items():
            if value is None: os.environ.pop(key, None)
            else: os.environ[key] = value


DISPLAY = {"display_count": "1", "display_1_name": "DSI-1", "display_1_mode": "1280x800", "display_1_rate": "60.00",
           "display_1_internal": "1", "network": "wlan0 192.168.1.42/24", "network_connected": "1"}


BOOT_MARKS = "init 1.42\nrecovery 1.60\ndrivers 3.95\nhardware 4.80\nservices 5.30\nxorg 6.05\nsession 7.10\ndesktop 7.38\n"


def boot_root(work, marks=BOOT_MARKS, history="1700000000 9100\n1700086400 7200\n1700172800 7380\n"):
    root = work / "demarrage"
    write(root / "run/winlux/boot-times", marks)
    write(root / "var/lib/winlux/boot-history", history)
    return root


def about_data(work, reports):
    """À propos de ce PC (#3053) : temps de démarrage mesuré, tuiles, Copier."""
    print("À propos de ce PC : données (#3053)")
    root = boot_root(work)
    fr = load("hw_apropos", HERE / "winlux-hardware-page.py",
              {"WINLUX_LANG": "fr", "WINLUX_DRIVERS_REPORT": str(reports["tablette"]), "WINLUX_BOOT_ROOT": str(root)})
    boot = fr.boot_facts()
    print(f"    démarrage relevé : {boot}")
    ok(boot["total_ms"] == 7380 and fr.seconds(boot["total_ms"]) == "7,4 s",
       "démarrage : total mesuré par l'init jusqu'au bureau (7,4 s)")
    ok(boot["phases"][0] == ("init", 1420) and ("drivers", 2350) in boot["phases"]
       and sum(ms for _, ms in boot["phases"]) == 7380, "démarrage : étapes entre deux repères, somme = total")
    ok(boot["slowest"] == ("drivers", 2350), "démarrage : étape la plus longue (hors noyau)")
    ok(boot["previous_ms"] == 7200 and boot["best_ms"] == 7200 and boot["count"] == 3
       and boot["history"] == [9100, 7200, 7380], "démarrage : précédent, meilleur, historique sans compter deux fois ce démarrage")
    unrecorded = fr.boot_facts(boot_root(work / "x", history="1 8000\n"))
    ok(unrecorded["history"] == [8000, 7380] and unrecorded["count"] == 2, "démarrage pas encore enregistré : ajouté à l'historique")
    ok(fr.boot_facts(boot_root(work / "y", marks="init 1.4\nxorg 6.0\n")) == {}, "démarrage pas fini : aucun temps inventé")
    ok(fr.boot_facts(work / "absent") == {}, "aucun repère : aucun temps")
    ok(fr.boot_facts(boot_root(work / "z", marks="init\nx y\nok -1\ndesktop 3.5\n", history="bad\n"))["total_ms"] == 3500,
       "repères illisibles ignorés")

    tiles = fr.highlights(fr.report())
    print(f"    tuiles tablette : {tiles}")
    ok([t[0] for t in tiles] == ["cpu", "memory", "drive", "gpu"], "tablette : processeur, mémoire, stockage, carte graphique")
    ok(tiles[0][2] == "Intel Atom x5-Z8350" and tiles[0][3] == "2 cœurs, 4 threads", "processeur : nom court, cœurs et threads")
    ok(tiles[2][2] == "31,3 Go" and tiles[2][3].startswith("Mémoire eMMC"), "stockage : taille et type")
    ok(tiles[3][2] == "Intel Atom/Celeron/Pentium Processor x5-E8000/J3xxx/N3xxx Integrated Graphics", "carte graphique : nom raccourci")
    desk = load("hw_apropos_bureau", HERE / "winlux-hardware-page.py",
                {"WINLUX_LANG": "fr", "WINLUX_DRIVERS_REPORT": str(reports["bureau"])})
    tiles = desk.highlights(desk.report())
    print(f"    tuiles bureau : {tiles}")
    ok(("drive", "Stockage", "2,5 To", "2 disques") in tiles, "bureau : deux disques additionnés")
    ok("gpu" not in [t[0] for t in tiles], "carte graphique non détectée : pas de tuile")
    usb = dict(desk.report(), pc_disk_count="3", pc_disk2_kind="usb", pc_disk2_size="64000000000")
    ok(desk.highlights(usb) == tiles, "clé USB branchée : pas comptée dans le stockage du PC")
    ok(fr.short_cpu("11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz") == "11th Gen Intel Core i5-1135G7"
       and fr.short_cpu("AMD Ryzen 5 5600X 6-Core Processor") == "AMD Ryzen 5 5600X 6-Core", "nom court du processeur")
    text = fr.about_text(fr.report(), {"version": "0.2.21", "kernel": "7.0.0-14-generic", **DISPLAY}, boot)
    print(text)
    ok("Modèle : T100HAN (1.0)" in text and text.count("Modèle") == 1 and "Winlux 0.2.21 · noyau 7.0.0-14-generic" in text
       and "Temps de démarrage : 7,4 s" in text and "Secure Boot : Désactivé" in text, "Copier : ce que la page montre")
    for lang in ("en", "es", "de"):
        other = load(f"hw_apropos_{lang}", HERE / "winlux-hardware-page.py", {"WINLUX_LANG": lang})
        ok(other._t("Démarrage") != "Démarrage" and other._t("Copier") != "Copier", f"{lang} : page traduite")
    brand = re.compile(r"microsoft|\bedge\b|\bbing\b", re.I)
    source = (HERE / "winlux-hardware-page.py").read_text()
    added = source[source.index("# À propos de ce PC (#3053)"):source.index("# ── Page Matériel et pilotes")]
    ok(not brand.search(added), "aucune marque Microsoft dans les textes de la page")


def about_window(work, reports):
    """La vraie fenêtre Paramètres : page 9 affichée aussitôt, complétée ensuite."""
    import runpy
    import time
    print("À propos de ce PC : fenêtre (#3053)")
    slow = work / "bin" / "winlux-drivers-lent"
    calls = work / "appels-drivers"
    slow.write_text(f"#!/bin/sh\necho appel >> {calls}\nsleep 1\ncat {reports['tablette']}\n")
    slow.chmod(0o755)
    missing = work / "pas-encore" / "drivers.txt"
    os.environ.update(WINLUX_LANG="fr", WINLUX_DRIVERS_REPORT=str(missing), WINLUX_BOOT_ROOT=str(boot_root(work / "fenetre")))
    settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="apropos_test")
    Gtk = settings["Gtk"]
    hardware = settings["EXTRA"].hardware_page()
    hardware.tool = lambda: [str(slow)]
    window = settings["Settings"]()
    window.show_all()

    def walk(widget):
        yield widget
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): yield from walk(child)

    def labels():
        return [w.get_text() for w in walk(window.body) if isinstance(w, Gtk.Label)]

    def pump(cond=lambda: False, seconds=6.0):
        end = time.monotonic() + seconds
        while time.monotonic() < end and not cond():
            while Gtk.events_pending(): Gtk.main_iteration_do(False)
            time.sleep(0.01)
        while Gtk.events_pending(): Gtk.main_iteration_do(False)
        return cond()

    start = time.monotonic()
    window.open_page(9)
    ms = (time.monotonic() - start) * 1000
    print(f"    page 9 ouverte en {ms:.0f} ms, relevé du matériel absent ; textes : {labels()[:12]}")
    ok(ms < 500, f"À propos s'ouvre sans attendre winlux-drivers ({ms:.0f} ms)")
    ok("Détection du matériel…" in labels() and "7,4 s" in labels(), "« Détection du matériel… », démarrage déjà affiché")
    filled = pump(lambda: "Intel Atom x5-Z8350" in labels())
    print(f"    après réponse : {labels()[:16]}")
    ok(filled and "Détection du matériel…" not in labels(), "les tuiles apparaissent quand le relevé a répondu")
    ok(any(text.startswith("ASUSTeK COMPUTER INC. T100HAN") for text in labels()), "en-tête : fabricant et modèle")
    ok("Votre PC" in labels(), "« Votre PC » complété aussi")
    body = window.body.get_children()
    hero = next(w for w in walk(window.body) if w.get_name() == "about-hero")
    top = next(child for child in body if hero in list(walk(child)))
    ok(body.index(top) < body.index(window.state_card), "en-tête et démarrage au-dessus de la carte Version / Noyau")
    button = next(w for w in walk(hero) if isinstance(w, Gtk.Button))
    button.clicked()
    pump(seconds=0.2)
    from gi.repository import Gdk
    copied = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text() or ""
    print(f"    presse-papiers : {copied!r}")
    ok("Nom du PC : " in copied and "Temps de démarrage : 7,4 s" in copied, "Copier met la fiche du PC dans le presse-papiers")
    pump(seconds=1.5)
    ok(calls.read_text().count("appel") == 1, "winlux-drivers lancé une seule fois")

    calls.unlink()
    window.open_page(9)
    window.open_page(11)
    shown = labels()
    pump(seconds=2.0)
    ok(window.current == 11 and labels() == shown, "une réponse tardive ne touche pas la page affichée ensuite")

    missing.parent.mkdir(parents=True, exist_ok=True)
    missing.write_text(reports["tablette"].read_text())
    calls.unlink(missing_ok=True)
    start = time.monotonic()
    window.open_page(9)
    ms = (time.monotonic() - start) * 1000
    pump(seconds=1.2)
    ok("Intel Atom x5-Z8350" in labels() and not calls.exists(), f"relevé présent : tout affiché d'emblée, sans commande ({ms:.0f} ms)")
    window.destroy()


def main():
    work = Path(tempfile.mkdtemp())
    bindir = work / "bin"
    fake_bin(bindir)
    tablet_root, desktop_root = work / "tablette", work / "bureau"
    tablet(tablet_root)
    desktop(desktop_root)

    print("Relevé pc_* de winlux-drivers")
    reports = {}
    for name, root in (("tablette", tablet_root), ("bureau", desktop_root)):
        path = work / f"{name}.txt"
        path.write_text(status(root, bindir))
        reports[name] = path
    t = dict(line.split("=", 1) for line in reports["tablette"].read_text().splitlines() if "=" in line)
    d = dict(line.split("=", 1) for line in reports["bureau"].read_text().splitlines() if "=" in line)
    ok(t["pc_vendor"] == "ASUSTeK COMPUTER INC." and t["pc_model"] == "T100HAN", "tablette : fabricant et modèle (DMI)")
    ok(t["pc_firmware"] == "uefi" and t["pc_firmware_bits"] == "32", "tablette : UEFI 32 bits")
    ok(t["pc_secure_boot"] == "0", "tablette : Secure Boot lu dans la variable EFI (désactivé)")
    ok(t["pc_cpu_cores"] == "2" and t["pc_cpu_threads"] == "4", "processeur : cœurs et threads")
    ok(t["pc_disk_count"] == "1" and t["pc_disk0_kind"] == "emmc" and t["pc_disk0_free"] != "", "eMMC avec place libre, loop ignoré")
    ok(t["pc_net_count"] == "1" and t["pc_net0_kind"] == "wifi", "réseau : carte Wi-Fi seule (ni lo ni pont virtuel)")
    ok(t["pc_battery_count"] == "1" and t["pc_battery0_health"] == "90", "batterie du PC (pas celle de la souris), santé 90 %")
    ok(t["pc_sound"] == "bytcr-rt5640" and "Integrated Graphics" in t["pc_gpus"], "carte son et carte graphique")
    ok(d["pc_vendor"].startswith("Micro-Star") and d["pc_model"].startswith("B550"), "bureau : valeurs O.E.M. remplacées par la carte mère")
    ok(d["pc_firmware"] == "bios" and d["pc_secure_boot"] == "" and d["pc_battery_count"] == "0", "bureau : BIOS hérité, pas de batterie")
    ok(d["pc_disk_count"] == "2" and {d["pc_disk0_kind"], d["pc_disk1_kind"]} == {"nvme", "hdd"}, "bureau : SSD NVMe et disque dur")

    print("Lignes « Votre PC » (4 langues)")
    expected = {"fr": ("Votre PC", "UEFI 32 bits", "Batterie", "76 % · sur batterie · santé 90 %", "Désactivé", "3,7 Go"),
                "en": ("Your PC", "UEFI 32-bit", "Battery", "76 % · on battery · health 90 %", "Off", "3.7 GB"),
                "es": ("Su PC", "UEFI de 32 bits", "Batería", "76 % · con batería · salud 90 %", "Desactivado", "3,7 GB"),
                "de": ("Ihr PC", "UEFI 32 Bit", "Akku", "76 % · Akkubetrieb · Zustand 90 %", "Deaktiviert", "3,7 GB")}
    modules = {}
    for lang, (title, uefi, battery, battery_text, off, ram) in expected.items():
        module = load(f"hw_{lang}", HERE / "winlux-hardware-page.py", {"WINLUX_LANG": lang, "WINLUX_DRIVERS_REPORT": str(reports["tablette"])})
        modules[lang] = module
        rows = dict(module.pc_rows(module.report(), DISPLAY))
        values = " | ".join(rows.values())
        ok(module._t("Votre PC") == title, f"{lang} : titre « {title} »")
        ok(uefi in values and battery in rows and battery_text in values, f"{lang} : micrologiciel et batterie")
        ok(rows.get(module._t("Secure Boot")) == off and ram in values, f"{lang} : Secure Boot et mémoire")
        ok("1280 × 800" in values and "192.168.1.42" in values, f"{lang} : écran et adresse réseau")
        missing = [key for key in module._TR if len(module._TR[key]) != 3 or not all(module._TR[key])]
        ok(not missing, f"{lang} : chaque texte a ses trois traductions")
    fr = load("hw_bureau", HERE / "winlux-hardware-page.py", {"WINLUX_LANG": "fr", "WINLUX_DRIVERS_REPORT": str(reports["bureau"])})
    rows = dict(fr.pc_rows(fr.report(), {}))
    ok("Batterie" not in rows and "Secure Boot" not in rows, "bureau : pas de ligne Batterie ni Secure Boot (masquées)")
    ok(rows.get("Micrologiciel") == "BIOS hérité", "bureau : BIOS hérité")
    ok(any("Samsung SSD 980 1TB · SSD NVMe" in v for v in rows.values()) and any("Disque dur · 2 To" in v for v in rows.values()),
       "bureau : disques avec modèle, type et taille")
    ok(not any(not value for value in rows.values()), "aucune ligne vide")

    about_data(work, reports)

    try:
        import gi
        gi.require_version("Gtk", "3.0")
        from gi.repository import Gtk
        if not Gtk.init_check()[0]: raise RuntimeError("pas d'affichage")
    except Exception as error:
        print(f"  SAUTÉ  rendu GTK ({error})")
        print(f"À propos / Votre PC : {checks} vérifications.")
        return

    class Window:
        def __init__(self):
            self.body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)

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
            parent.pack_start(line, False, False, 0)

    def texts(widget):
        out = [widget.get_text()] if isinstance(widget, Gtk.Label) else []
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): out += texts(child)
        return out

    for lang, module in modules.items():
        window = Window()
        module.pc_section(window, DISPLAY)
        shown = texts(window.body)
        ok(shown[0] == expected[lang][0] and "T100HAN (1.0)" in shown, f"{lang} : section GTK rendue")
    empty = load("hw_vide", HERE / "winlux-hardware-page.py", {"WINLUX_LANG": "fr", "WINLUX_DRIVERS_REPORT": str(work / "absent.txt")})
    empty.tool = lambda: None
    window = Window()
    empty.pc_section(window, {})
    ok(not texts(window.body), "rien de détecté : pas de section vide")
    about_window(work, reports)
    print(f"À propos / Votre PC : {checks} vérifications.")


if __name__ == "__main__":
    main()
