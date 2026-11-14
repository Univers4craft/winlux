# Assistant d'installation : un installateur tué par un signal est dit arrêté par le système, pas « code -9 » (#2327).
_w = (ROOT / "scripts/winlux-setup-wizard.py").read_text(encoding="utf-8")
_t = (ROOT / "scripts/test-setup-wizard.sh").read_text(encoding="utf-8")
ok("def failure_message(" in _w and "#2327" in _t,
   "assistant : échec par signal nommé (arrêt par le système), vérifié sous Xvfb (#2327)")
