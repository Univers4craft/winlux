# #1442 : la page Écran des Paramètres se reconstruit seule quand un écran
# est branché ou débranché (état display_* publié par winlux-control).
settings_py = (SCRIPTS / "winlux-settings.py").read_text(encoding="utf-8")
pages_py = (SCRIPTS / "winlux-settings-pages.py").read_text(encoding="utf-8")
ok("def display_signature" in pages_py and "EXTRA.display_signature(status)" in settings_py
   and (SCRIPTS / "test-display-page-follow.py").is_file(),
   "Paramètres > Écran suit un écran débranché sans rechargement (#1442)")
