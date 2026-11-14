# Boutique : virt-manager et Boxes arrivent avec QEMU et le pilote libvirt (#2075).
_c = (ROOT / "integration/store-catalog.txt").read_text(encoding="utf-8")
_s = (ROOT / "scripts/winlux-store.py").read_text(encoding="utf-8")
_r = (ROOT / "scripts/winlux-root.py").read_text(encoding="utf-8")
_comp = {l[11:].partition(":")[0]: l.partition(":")[2].split(",") for l in _c.splitlines() if l.startswith("compagnons=")}
ok(all("qemu-system-x86" in _comp.get(p, []) and "libvirt-daemon-driver-qemu" in _comp.get(p, []) for p in ("virt-manager", "gnome-boxes")),
   "virt-manager et Boxes installés avec QEMU et le pilote libvirt (#2075)")
ok(not any("libvirt-daemon-system" in v or any("systemd" in x for x in v) for v in _comp.values()),
   "aucun compagnon ne tire systemd (#2075)")
ok('"--mark-auto", "--", *extras' in _s and '"--mark-auto"' in _r, "compagnons marqués automatiques, admis par le relais root (#2075)")
