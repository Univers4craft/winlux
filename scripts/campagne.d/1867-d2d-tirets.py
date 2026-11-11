# #1867 : les styles de trait en tirets sont appliqués (contour de la
# sélection de Paint.NET invisible après un trait au pinceau).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
raster = (runtime_dlls / "d2d_raster.rs").read_text(encoding="utf-8")
ok("pub fn dashed" in raster and "v11::stroke_pattern(style)" in src,
   "Direct2D : tirets des styles de trait appliqués (#1867)")
