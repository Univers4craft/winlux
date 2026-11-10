# #1542 : la présentation DXGI couvre la taille réelle de la fenêtre X
# (XGetGeometry) et présente deux fois quand elle a changé, pour que Mesa
# relise la taille : plus d'image noire, mélangée ou décalée après Agrandir.
dxgi = (ROOT / "runtime/src/dlls/dxgi.rs").read_text(encoding="utf-8")
ok("unsafe fn drawable_size(" in dxgi
   and "fn image_presentee_couvre_la_fenetre_redimensionnee_x11()" in dxgi,
   "Claude agrandi : image présentée sur toute la fenêtre (#1542)")
