# #1534 : le clavier simulé du banc (/dev/uhid) ne doit jamais pouvoir taper à la place de quelqu'un.
simulateur = (ROOT / "scripts/banc/winlux-uhid-simulateur.py").read_text(encoding="utf-8")
regles = [p for p in ROOT.rglob("*.rules") if "target" not in p.parts and ".git" not in p.parts]
ok(not any("uhid" in p.read_text(encoding="utf-8", errors="replace") for p in regles),
   "/dev/uhid reste fermé aux comptes : aucune règle udev ne l'ouvre (#1534)")
ok("winlux-uhid-simulateur" not in (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8"),
   "le clavier simulé du banc n'est pas installé dans l'ISO (#1534)")
ok("os.geteuid() != 0" in simulateur and "in_vm()" in simulateur and "UHID_INPUT" not in simulateur,
   "le clavier simulé tourne en root dans une VM seulement et n'envoie aucune frappe (#1534)")
