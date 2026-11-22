# Fenêtre UAC d'un compte standard : choix de l'administrateur qui autorise (#2198).
_uac = (ROOT / "scripts/winlux-uac.py").read_text(encoding="utf-8")
ok("def admin_choices():" in _uac and "Gtk.ComboBoxText()" in _uac, "compte standard : liste des administrateurs dans la fenêtre UAC (#2198)")
# Comportement vérifié par test-uac.py (CI) : admin_choices et verify sur l'administrateur choisi.
ok("def verify(" in _uac and "#2198" in (ROOT / "scripts/test-uac.py").read_text(encoding="utf-8")
   and "scripts/test-uac.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"), "le mot de passe vérifié est celui de l'administrateur choisi (#2198)")
