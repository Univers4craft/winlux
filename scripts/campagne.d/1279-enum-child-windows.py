# EnumChildWindows énumère les descendants ; l'onglet « Home » de Sumatra PDF
# redessine la zone du document.
ok("fn descendants" in (runtime_dlls / "user32.rs").read_text()
   and "enum_child_windows_parcourt_tous_les_descendants" in (SCRIPTS / "test-all.d" / "1279-enum-child-windows.sh").read_text()
   and "invalidation_apres_validation_repeint" in (runtime_dlls / "gui.rs").read_text(),
   "EnumChildWindows et invalidation après ValidateRect : accueil de Sumatra PDF (#1279)")
