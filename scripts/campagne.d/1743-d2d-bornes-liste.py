# #1743 : bornes d'une liste de commandes = ce qui y est dessiné (masque du
# pinceau de Paint.NET trop petit, trait trop fin) ; ellipses rondes.
v11 = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
raster = (runtime_dlls / "d2d_raster.rs").read_text(encoding="utf-8")
# Noms : record_list_bounds et son test bornes_d_une_liste_de_commandes (#2711).
ok("fn record_list_bounds" in v11 and "fn bornes_d_une_liste_de_commandes" in v11,
   "Direct2D : GetImageLocalBounds d'une liste de commandes couvre son dessin (#1743)")
# Nom du test ellipse_au_moins_64_points (#2711).
ok("fn ellipse_au_moins_64_points" in v11,
   "Direct2D : ellipse aplatie en au moins 64 points (#1743)")
