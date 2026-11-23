# #3438 : une fenêtre possédée posée hors gestionnaire (bulle Chromium) est
# cachée avec sa propriétaire réduite et rendue à la restauration (Learn,
# Owned Windows) ; gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
ok("fn possedees_hors_gestionnaire_cachees_avec_leur_proprietaire_3438" in t, "fenêtres possédées hors gestionnaire cachées avec leur propriétaire réduite (#3438)")
