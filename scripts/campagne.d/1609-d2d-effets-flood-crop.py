# #1609 : Flood, Crop et Premultiply sont calculés (canevas de Paint.NET : tuiles unies tirées d'un Flood 1x1).
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
ok("CLSID_FLOOD" in src and "CLSID_CROP" in src and "fn builtin_render" in src,
   "Direct2D : effets Flood, Crop et Premultiply calculés (#1609)")
