# #1749 : cible Direct2D sur un bitmap WIC 128bppPRGBAFloat acceptée
# (Inverser les couleurs et les effets GPU de Paint.NET plantaient).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("FMT_PRGBA_FLOAT128 => WicLayout::PrgbaF32" in src,
   "Direct2D : cible WIC 128bppPRGBAFloat acceptée (#1749)")
