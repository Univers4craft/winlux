# #2938 : l'encodeur PNG de WIC garde les formats indexés (1, 2, 4 et
# 8 bits par pixel) et leur palette, comme celui de Windows ; Paint.NET
# refusait d'enregistrer en « Détecter automatiquement » (Indexed8).
# Comportement gardé par le test unitaire
# dlls::windowscodecs::encoder::tests::encodeur_png_indexe_8_bits.
enc_2938 = (ROOT / "runtime" / "src" / "dlls" / "windowscodecs_encoder.rs").read_text(encoding="utf-8")
ok("fn index_bits" in enc_2938 and "fn frame_set_palette" in enc_2938
   and "fn encodeur_png_indexe_8_bits" in enc_2938,
   "WIC : encodeur PNG indexé avec sa palette (#2938)")
