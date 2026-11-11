# #1669 : effet Opacity calculé, et journal Direct2D de chaque effet et de chaque remplissage (empreinte du pinceau de Paint.NET).
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
d2d = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("CLSID_OPACITY" in src and "profondeur {} →" in src and "fn trace_fill" in d2d,
   "Direct2D : effet Opacity et journal des effets et remplissages (#1669)")
