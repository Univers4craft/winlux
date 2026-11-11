# #1717 : effet Invert calculé (masque de l'empreinte du pinceau de Paint.NET).
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
ok("CLSID_INVERT" in src and "e.clsid == CLSID_INVERT" in src,
   "Direct2D : effet Invert calculé (#1717)")
