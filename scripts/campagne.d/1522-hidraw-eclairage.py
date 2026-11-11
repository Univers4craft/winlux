# #1522 : hidraw ouvert aux logiciels d'éclairage, pour les seuls fabricants listés.
regle = (ROOT / "integration/71-winlux-eclairage.rules").read_text(encoding="utf-8")
lignes = [l for l in regle.splitlines() if l.strip() and not l.lstrip().startswith("#")]
ok(lignes and all('KERNELS=="*:' in l and "hidraw" in l for l in lignes),
   "éclairage : règle hidraw limitée aux fabricants listés, rien de générique (#1522)")
ok(not any(m in "\n".join(lignes) for m in ("usb_device", "bus/usb", "uhid")),
   "éclairage : usbfs et /dev/uhid restent fermés aux comptes (#1522)")
pilotes = (ROOT / "scripts/winlux-drivers.py").read_text(encoding="utf-8")
rgb = pilotes.split("RGB_VENDORS = (", 1)[1].split(")", 1)[0]
manettes = (ROOT / "integration/70-winlux-manettes.rules").read_text(encoding="utf-8")
ok(all(("*:%s:*" % v.strip(' "').upper()) in (lignes[0] + manettes) for v in rgb.split(",") if v.strip()),
   "éclairage : tous les fabricants RGB de winlux-drivers ont leur hidraw ouvert (#1522)")
ok("71-winlux-eclairage.rules" in (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
   and "71-winlux-eclairage.rules" in (ROOT / "integration/install.sh").read_text(encoding="utf-8"),
   "éclairage : règle installée par l'ISO et par install.sh (#1522)")
