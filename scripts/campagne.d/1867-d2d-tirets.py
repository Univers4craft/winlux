# #1867 : les styles de trait en tirets sont appliqués (contour de la
# sélection de Paint.NET invisible après un trait au pinceau).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
raster = (runtime_dlls / "d2d_raster.rs").read_text(encoding="utf-8")
# Noms : dashed, v11::stroke_pattern et le test trait_en_tirets_dans_une_cible (#2711).
ok("pub fn dashed" in raster and "v11::stroke_pattern" in src and "fn trait_en_tirets_dans_une_cible" in src,
   "Direct2D : tirets des styles de trait appliqués (#1867)")
