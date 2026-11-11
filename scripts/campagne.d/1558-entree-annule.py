# Boîtes « Désinstaller … ? » : Entrée déclenche Annuler, bouton par défaut par prudence (#1558).
ok("set_default_response(Gtk.ResponseType.CANCEL)" in (ROOT / "scripts/winlux-installed-apps.py").read_text()
   and "set_default_response(Gtk.ResponseType.CANCEL)" in (ROOT / "scripts/winlux-settings-pages.py").read_text(),
   "désinstallation : Entrée annule la boîte de confirmation (#1558)")
