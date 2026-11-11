# #1748 : pas d'entrée « Uninstall » seule dans le menu Démarrer.
apps = (SCRIPTS / "winlux-applications.py").read_text(encoding="utf-8")
ok("def is_uninstaller" in apps and "is_uninstaller(path.stem, executable)" in apps,
   "désinstalleurs écartés des raccourcis du menu Démarrer (#1748)")
ok("def menu_name" in apps and "GENERIC_NAMES" in apps, "nom générique précédé du nom de son dossier (#1748)")
ok((SCRIPTS / "test-menu-desinstalleur.py").is_file(), "test du menu Démarrer avec Uninstall.lnk de WinRAR (#1748)")
