# #2866 : une fenêtre dont la zone client est vide ne reçoit pas WM_PAINT
# (Learn, « The Update Region ») ; un Label de Paint.NET de taille nulle
# créait sinon une Bitmap 0×N dans son OnPaint et l'enregistrement plantait.
# Comportement gardé par le test unitaire
# dlls::gui::tests::pas_de_wm_paint_pour_une_zone_client_vide.
gui_2866 = (ROOT / "runtime" / "src" / "dlls" / "gui.rs").read_text(encoding="utf-8")
ok("fn client_is_empty" in gui_2866 and "fn pas_de_wm_paint_pour_une_zone_client_vide" in gui_2866,
   "Fenêtres : pas de WM_PAINT pour une zone client vide (#2866)")
