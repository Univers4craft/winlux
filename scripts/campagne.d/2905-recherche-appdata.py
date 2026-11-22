# #2905 : un fichier trouvé sous AppData s'affiche avec un nom de dossier
# lisible, jamais avec le chemin « AppData/…/Microsoft/Windows/Start Menu ».
# L'écran entier est vérifié sous Xvfb par scripts/test-start-search-appdata.py.
_menu_2905 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_2905")
_home_2905 = Path("/home/essai")
_lieux_2905 = [_menu_2905["file_place"](_home_2905 / rel, _home_2905) for rel in (
    "AppData/Roaming/Microsoft/Windows/Start Menu/Programs/PuTTY (64-bit)/PuTTY.lnk",
    "AppData/Local/Programs/PuTTY/putty.exe", "AppData/Roaming/Mozilla/Firefox/profiles.ini", "Documents/a.txt")]
ok(_lieux_2905 == ["Menu Démarrer", "Données des programmes › PuTTY", "Données des programmes › Firefox", "~/Documents"]
   and not any("microsoft" in l.casefold() or "appdata" in l.casefold() for l in _lieux_2905),
   "Recherche du menu Démarrer : dossiers AppData nommés, sans chemin ni marque Microsoft (#2905)")
