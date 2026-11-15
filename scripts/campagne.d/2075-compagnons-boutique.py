# Boutique : virt-manager et Boxes arrivent avec QEMU et le pilote libvirt (#2075).
_c = (ROOT / "integration/store-catalog.txt").read_text(encoding="utf-8")
_s = (ROOT / "scripts/winlux-store.py").read_text(encoding="utf-8")
_r = (ROOT / "scripts/winlux-root.py").read_text(encoding="utf-8")
_comp = {l[11:].partition(":")[0]: l.partition(":")[2].split(",") for l in _c.splitlines() if l.startswith("compagnons=")}
ok(all("qemu-system-x86" in _comp.get(p, []) and "libvirt-daemon-driver-qemu" in _comp.get(p, []) for p in ("virt-manager", "gnome-boxes")),
   "virt-manager et Boxes installés avec QEMU et le pilote libvirt (#2075)")
ok(not any("libvirt-daemon-system" in v or any("systemd" in x for x in v) for v in _comp.values()),
   "aucun compagnon ne tire systemd (#2075)")
# #2199 : installés avec le logiciel (une seule autorisation), commandes admises
# par le relais root : vérifié par test-store.py et test-store-compagnons-autorisation.py.
ok("def apt_companions(" in _s and "compagnons de virt-manager (#2075)" in (ROOT / "scripts/test-store.py").read_text(encoding="utf-8"),
   "compagnons installés avec le logiciel, testés (#2075, #2199)")
