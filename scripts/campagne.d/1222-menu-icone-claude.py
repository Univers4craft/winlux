# Menu WS_POPUP outil non activable (Chromium) hors gestionnaire de fenêtres.
ok("fn floating_popup(" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
   and "menu_dessine_par_le_programme_hors_gestionnaire" in "".join(p.read_text(encoding="utf-8") for p in (SCRIPTS / "test-all.d").glob("122*.sh")),
   "Menu de l'icône de Claude : ni recentré ni dans la barre des tâches (#1222)")
