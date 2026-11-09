# #1277 : une fenêtre CW_USEDEFAULT suit la taille de l'écran (ruban de ScreenToGif sur une ligne).
ok("fn default_window_size" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
   and "taille_par_defaut_suit_la_zone_de_travail" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8"),
   "CW_USEDEFAULT donne les trois quarts de la zone de travail, au moins 800 × 600 (#1277)")
