# VLC : la fenêtre possédée détruite avec le focus le rend à son propriétaire.
ok("owner_to_refocus(hwnd)" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
   and "focus_rendu_au_proprietaire_a_la_destruction" in (SCRIPTS / "test-all.d" / "1311-vlc-barre-de-menus.sh").read_text(encoding="utf-8"),
   "VLC : barre de menus redessinée après la boîte Ouvrir un fichier (#1311)")
