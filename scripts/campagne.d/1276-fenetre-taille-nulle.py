# Fenêtre de premier niveau créée en 0×0 : taille gardée, non montrée tant
# qu'elle n'a pas de surface (TApplication de WinSCP, #1276).
ok("fn empty_top_level" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "fn fenetre_de_taille_nulle_reste_nulle" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8"),
   "Fenêtre de premier niveau 0×0 gardée sans surface (#1276)")
