# #1749 : un effet pris comme nœud d'un graphe personnalisé lit les entrées
# du graphe (Inverser les couleurs de Paint.NET laissait la toile intacte).
v11 = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
fx = (runtime_dlls / "d2d1_effects.rs").read_text(encoding="utf-8")
ok("fn image_of_effect_with" in v11 and "GRAPH_INPUTS" in v11
   and "image_of_effect_with(fx, inputs" in fx,
   "Direct2D : effet-nœud calculé sur les entrées de son graphe (#1749)")
