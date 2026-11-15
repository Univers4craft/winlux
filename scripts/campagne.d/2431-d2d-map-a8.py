# #2431 : Map d'un bitmap A8 rend un octet d'alpha par pixel.
d = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
i = d.index("unsafe extern \"win64\" fn bitmap_map(")
corps = d[i:d.index("\n}\n", i)]
ok("b.alpha_only" in corps and "pitch: b.w," in corps,
   "Map d'un bitmap A8 : un octet par pixel, pas de la ligne = largeur (#2431)")
