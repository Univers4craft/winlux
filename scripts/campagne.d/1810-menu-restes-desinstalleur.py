# #1810 : un désinstalleur déjà lancé (launchers.json) ne garde pas d'entrée au menu Démarrer.
apps = (SCRIPTS / "winlux-applications.py").read_text(encoding="utf-8")
# Noms : launched et son usage par test-menu-desinstalleur.py (#2711).
ok("def launched" in apps and ".launched(" in (SCRIPTS / "test-menu-desinstalleur.py").read_text(encoding="utf-8"),
   "lancements de désinstalleurs écartés du menu Démarrer, restes nettoyés (#1810)")
