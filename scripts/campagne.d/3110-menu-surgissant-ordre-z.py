# #3110 : un menu surgissant override-redirect visible passe au-dessus de la
# fenêtre principale dans l'ordre Z (EnumWindows, WindowFromPoint) : Qt y
# cherche la fenêtre sous le pointeur et livrait le clic du menu à la
# principale. Le comportement est gardé par le test unitaire nommé.
_gui_3110 = (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
print("      test : menu_surgissant_visible_au_dessus_des_fenetres_gerees")
ok("fn menu_surgissant_visible_au_dessus_des_fenetres_gerees" in _gui_3110,
   "ordre Z : un menu surgissant visible passe au-dessus de la fenêtre principale (#3110)")
