# #1476 : un WM_SIZE venu du gestionnaire de fenêtres est précédé de
# WM_WINDOWPOSCHANGED (Chromium y redimensionne son rendu).
# #1477 : un clic sur la barre maison d'une fenêtre sans cadre passe par
# WM_NCHITTEST et devient WM_NCLBUTTONDOWN, qui lance le déplacement.
src = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok("unsafe fn geometry_changed(" in src and "BACKEND_GEOMETRY" in src,
   "Claude agrandi : contenu redimensionné (#1476)")
ok("unsafe fn caption_click(" in src and "fn fenetre_sans_cadre_agrandie_et_glissee()" in src,
   "Claude : glisser sa barre de titre déplace la fenêtre (#1477)")
