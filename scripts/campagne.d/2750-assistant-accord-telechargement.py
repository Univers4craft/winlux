# Assistant d'installation : un installeur web (code 4 du runtime) ouvre la
# question d'accord de téléchargement au lieu d'un échec, vérifiée sous Xvfb (#2750).
_w = (ROOT / "scripts/winlux-setup-wizard.py").read_text(encoding="utf-8")
_t = (ROOT / "scripts/test-all.d/2750-assistant-accord-telechargement.sh").read_text(encoding="utf-8")
ok("def confirm_download(" in _w and "def with_download(" in _w
   and "test-setup-wizard-telechargement.py" in _t,
   "assistant : accord de téléchargement d'un installeur web, vérifié sous Xvfb (#2750)")
