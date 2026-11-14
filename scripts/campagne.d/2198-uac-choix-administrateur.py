# Fenêtre UAC d'un compte standard : choix de l'administrateur qui autorise (#2198).
_uac = (ROOT / "scripts/winlux-uac.py").read_text(encoding="utf-8")
ok("def admin_choices():" in _uac and "Gtk.ComboBoxText()" in _uac, "compte standard : liste des administrateurs dans la fenêtre UAC (#2198)")
ok('chosen = result["user"]' in _uac and 'verify(chosen, entry.get_text())' in _uac and 'relay_check"](chosen' in _uac, "le mot de passe vérifié est celui de l'administrateur choisi (#2198)")
