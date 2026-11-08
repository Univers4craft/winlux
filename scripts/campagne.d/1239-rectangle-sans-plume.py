# Rectangle sans plume : rempli depuis (left, top) ; les lettres « G/D »
# des vumètres d'Audacity ne sont plus collées à une ligne noire.
ok("let inset = i32::from(outline.is_some());" in (runtime_dlls / "gdi32.rs").read_text()
   and "rectangle_sans_plume_commence_au_coin_haut_gauche" in (SCRIPTS / "test-all.d" / "1239-rectangle-sans-plume.sh").read_text(),
   "Rectangle sans plume rempli depuis le coin haut gauche : « G/D » des vumètres d'Audacity (#1239)")
