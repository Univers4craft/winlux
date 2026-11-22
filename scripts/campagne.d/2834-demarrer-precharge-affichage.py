# #2834 : le préchargement du menu Démarrer fait aussi, fenêtre cachée, le
# premier affichage des widgets ; le premier appui ne le paie plus. Mesure
# gardée par scripts/test-start-menu-preload.py (premier appui au moins deux
# fois plus rapide qu'un lancement à froid), sous Xvfb en CI.
menu_2834 = (SCRIPTS / "winlux-start-menu.py").read_text(encoding="utf-8")
ok("def prepare_hidden(self):" in menu_2834
   and (SCRIPTS / "test-start-menu-preload.py").exists(),
   "Démarrer préchargé : premier affichage fait pendant le préchargement (#2834)")
