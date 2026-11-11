# #1810 : un désinstalleur déjà lancé (launchers.json) ne garde pas d'entrée au menu Démarrer.
apps = (SCRIPTS / "winlux-applications.py").read_text(encoding="utf-8")
ok("def launched" in apps and "entries = launched(home)" in apps,
   "lancements de désinstalleurs écartés du menu Démarrer, restes nettoyés (#1810)")
