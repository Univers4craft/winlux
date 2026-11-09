# #1384 : WIC crée des bitmaps 16 bits et flottants et transforme les couleurs.
ok("fn layout(fmt: &Guid)" in (runtime_dlls / "wic_pixels.rs").read_text(encoding="utf-8")
   and "fn color_transform_initialize" in (runtime_dlls / "windowscodecs.rs").read_text(encoding="utf-8"),
   "WIC : formats larges et transformation de couleur (#1384)")
