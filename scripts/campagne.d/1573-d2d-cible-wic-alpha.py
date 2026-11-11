# #1573 : CreateWicBitmapRenderTarget accepte 8bppAlpha et 32bppPRGBA (masque du pinceau de Paint.NET).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("enum WicLayout" in src and "FMT_ALPHA8 => WicLayout::Alpha8" in src,
   "Direct2D : cible WIC en alpha seul et RGBA (#1573)")
