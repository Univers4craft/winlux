# #1554 : GetDevice sur la vue contexte d'une cible 1.0 rend un ID2D1Device (canevas de Paint.NET).
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
ok("t.ext.device = new_res(" in src,
   "Direct2D : GetDevice d'une cible fenêtre (#1554)")
