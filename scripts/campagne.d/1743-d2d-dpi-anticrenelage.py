# #1743 : DPI des cibles, SetAntialiasMode et D2D1_PRIMITIVE_BLEND appliqués
# (trait du pinceau de Paint.NET gris clair au lieu de noir).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
raster = (runtime_dlls / "d2d_raster.rs").read_text(encoding="utf-8")
ok("fn props_dpi" in src and "dip_scale()" in src and "set_antialias as *const ()" in src,
   "Direct2D : DPI de la cible et anticrénelage appliqués (#1743)")
ok("pub fn fill_mode" in raster and "aliased" in raster,
   "Direct2D : remplissage crénelé et mélange des primitives (#1743)")
