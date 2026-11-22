# #2853 : le texte d'un élément sélectionné (COLOR_HIGHLIGHTTEXT) se lit sur
# le violet de sélection (COLOR_HIGHLIGHT, COLOR_MENUHILIGHT). Le
# comportement est gardé par le test unitaire nommé.
src = (ROOT / "runtime" / "src" / "dlls" / "gui.rs").read_text(encoding="utf-8")
ok("fn texte_de_selection_lisible_sur_la_selection_2853" in src,
   "Couleurs système : texte de sélection lisible sur la sélection (#2853)")
